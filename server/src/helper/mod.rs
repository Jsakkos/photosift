//! The Claude helper: it proposes outcomes, the user decides.
//!
//! `HelperService` owns one worker thread that runs one `claude -p` at a time.
//! A run only writes through PhotoSift's MCP tools, and the background deny
//! list keeps it away from `set_outcome`: nothing it does touches an outcome,
//! a sidecar or the Recycle Bin.

mod chat;
pub mod learning;
pub mod prompt;
pub mod stream;
pub mod transport;

use std::collections::{HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use uuid::Uuid;

use crate::config::Settings;
use crate::db::{
    helper::{self, RunKind, RunScope, RunStatus},
    review, DbPool,
};
use crate::events::{EventHub, ServerEvent};
use crate::plog;
use stream::{RunEvent, RunResult};
use transport::{Availability, RunRequest, SessionMode, Transport};

/// Tools a background run may not call: the user decides outcomes and owns
/// the taste profile, and the app opens the run itself.
pub const BACKGROUND_DENY: &[&str] = &[
    "mcp__photosift__set_outcome",
    "mcp__photosift__set_taste_profile",
    "mcp__photosift__start_run",
];

/// Tools a chat turn may not call. `set_taste_profile` is allowed: the user
/// asked for the change in their own words.
pub const CHAT_DENY: &[&str] = &["mcp__photosift__set_outcome", "mcp__photosift__start_run"];

/// Run directories (the system prompt each run saw) are kept this long.
const RUN_DIR_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);
/// Longest error text stored on a run.
const ERROR_MAX: usize = 500;
const CHECKING: &str = "checking whether the claude CLI is installed and signed in";

/// What `GET /api/status` reports as `helper`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HelperStatusInfo {
    pub available: bool,
    pub reason: Option<String>,
    /// The `helper_transport` setting.
    pub transport: String,
    /// Runs waiting or running.
    pub queue: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum EnqueueError {
    #[error("{0}")]
    Unavailable(String),
    #[error("shoot {0} already has a helper run queued or running")]
    Conflict(i64),
    #[error("no shoot {0}")]
    NotFound(i64),
    #[error("the helper is still answering the last chat message for shoot {0}")]
    ChatBusy(i64),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Queued after enrichment (and kept in `helper_pending` until the worker
    /// takes it). Its run row is created only when the worker finds
    /// unproposed photos, so a follow-up that an earlier run already covered
    /// leaves no row and spawns no `claude`. `retry`: re-queued for a run a
    /// restart interrupted; its run is not retried again.
    Automatic { retry: bool },
    /// The run button or the eval harness: the row exists from the request,
    /// and the run reviews the whole shoot.
    Explicit(i64),
}

struct Job {
    shoot_id: i64,
    origin: Origin,
}

#[derive(Default)]
struct State {
    pending: VecDeque<Job>,
    active: bool,
    shutdown: bool,
    /// `None` until the first check has finished.
    availability: Option<Availability>,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    /// Serialises the "is one open?" check with creating the run.
    enqueue: Mutex<()>,
    /// Shoots with a chat turn in flight (chat turns run on their own threads).
    chatting: Mutex<HashSet<i64>>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct Ctx {
    db: DbPool,
    events: EventHub,
    settings: Arc<Settings>,
    transport: Arc<dyn Transport>,
    /// `<data root>\helper\runs\<run_id>\system.md`.
    runs_dir: PathBuf,
    /// The child's cwd: empty and outside any repository.
    work_dir: PathBuf,
}

/// Stops the worker thread when the last `HelperService` clone goes away.
struct ShutdownOnDrop(Arc<Shared>);

impl Drop for ShutdownOnDrop {
    fn drop(&mut self) {
        self.0.lock().shutdown = true;
        self.0.wake.notify_all();
    }
}

/// Cheap to clone; all clones feed the same worker thread.
#[derive(Clone)]
pub struct HelperService {
    shared: Arc<Shared>,
    ctx: Arc<Ctx>,
    _shutdown: Arc<ShutdownOnDrop>,
}

impl HelperService {
    /// Interrupts runs left open by an earlier process, prepares
    /// `<data root>\helper\`, and starts the worker, which first checks that
    /// the transport is available. The transport already carries the `/mcp`
    /// URL (see `lib::build_state`).
    ///
    /// Re-queued at startup: the automatic jobs that were waiting
    /// (`helper_pending`), and once each, the shoots whose background run this
    /// startup interrupted. Nothing else: a sweep of every shoot with
    /// unproposed photos would spend the user's allowance on old shoots.
    pub fn start(
        db: DbPool,
        events: EventHub,
        data_root: &Path,
        settings: Arc<Settings>,
        transport: Arc<dyn Transport>,
    ) -> Self {
        match helper::interrupt_open_runs(&db) {
            Ok(runs) => {
                for run in runs {
                    if run.kind == RunKind::Background.as_str() && !run.retry {
                        if let Err(e) = helper::add_pending(&db, run.shoot_id, true) {
                            plog!("helper", "re-queueing shoot {}: {e:#}", run.shoot_id);
                        }
                    }
                    plog!(
                        "helper",
                        "run {} of shoot {} was {} at shutdown; marked interrupted",
                        run.id,
                        run.shoot_id,
                        run.status
                    );
                    events.send(ServerEvent::HelperStatus {
                        shoot_id: run.shoot_id,
                        run_id: run.id,
                        status: RunStatus::Interrupted.as_str().into(),
                        error: None,
                    });
                }
            }
            Err(e) => plog!("helper", "could not interrupt open runs: {e:#}"),
        }
        let helper_dir = data_root.join("helper");
        let runs_dir = helper_dir.join("runs");
        let work_dir = helper_dir.join("work");
        prepare_dirs(&runs_dir, &work_dir);

        let mut state = State::default();
        if settings.helper_enabled {
            match helper::pending_shoots(&db) {
                Ok(shoots) => {
                    for (shoot_id, retry) in shoots {
                        plog!("helper", "shoot {shoot_id}: re-queued its pending run");
                        state.pending.push_back(Job {
                            shoot_id,
                            origin: Origin::Automatic { retry },
                        });
                    }
                }
                Err(e) => plog!("helper", "could not read pending runs: {e:#}"),
            }
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(state),
            wake: Condvar::new(),
            enqueue: Mutex::new(()),
            chatting: Mutex::new(HashSet::new()),
        });
        let ctx = Arc::new(Ctx {
            db,
            events,
            settings,
            transport,
            runs_dir,
            work_dir,
        });
        let (thread_shared, thread_ctx) = (Arc::clone(&shared), Arc::clone(&ctx));
        let spawned = std::thread::Builder::new()
            .name("helper".into())
            .spawn(move || worker_loop(&thread_shared, &thread_ctx));
        if let Err(e) = spawned {
            plog!("helper", "could not start the helper thread: {e}");
        }
        Self {
            _shutdown: Arc::new(ShutdownOnDrop(Arc::clone(&shared))),
            shared,
            ctx,
        }
    }

    pub fn status(&self) -> HelperStatusInfo {
        let state = self.shared.lock();
        let (available, reason) = match &state.availability {
            Some(Availability::Ready { .. }) => (true, None),
            Some(Availability::Unavailable { reason }) => (false, Some(reason.clone())),
            None => (false, Some(CHECKING.to_string())),
        };
        HelperStatusInfo {
            available,
            reason,
            transport: self.ctx.settings.helper_transport.clone(),
            queue: state.pending.len() + usize::from(state.active),
        }
    }

    /// Why the helper cannot run, once a check has said so (not while the
    /// first check is still in flight).
    pub fn unavailable_reason(&self) -> Option<String> {
        match &self.shared.lock().availability {
            Some(Availability::Unavailable { reason }) => Some(reason.clone()),
            _ => None,
        }
    }

    /// Checks the transport again (blocking: it may spawn `claude`), e.g.
    /// after the user signed in and pressed Retry.
    pub fn refresh_availability(&self) -> Availability {
        let a = self.ctx.transport.availability();
        log_availability(&a);
        self.shared.lock().availability = Some(a.clone());
        a
    }

    /// The automatic run after enrichment. Skipped when `helper_enabled` is
    /// off, the helper is known to be unavailable, a run is already waiting
    /// for the shoot, or nothing is unproposed. A shoot whose run is in
    /// progress gets a follow-up queued, so photos imported meanwhile are not
    /// missed; the worker checks again before starting it.
    pub fn enqueue(&self, shoot_id: i64) {
        if !self.ctx.settings.helper_enabled || self.unavailable_reason().is_some() {
            return;
        }
        let _serial = self
            .shared
            .enqueue
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let waiting = self
            .shared
            .lock()
            .pending
            .iter()
            .any(|j| j.shoot_id == shoot_id);
        let skip = waiting
            || match helper::open_run(&self.ctx.db, shoot_id) {
                Ok(Some(run)) if run.status == RunStatus::Queued.as_str() => true,
                Ok(_) => match helper::automatic_scope(&self.ctx.db, shoot_id) {
                    Ok(scope) => scope == RunScope::Nothing,
                    Err(e) => {
                        plog!("helper", "checking shoot {shoot_id} for new photos: {e:#}");
                        true
                    }
                },
                Err(e) => {
                    plog!("helper", "checking shoot {shoot_id} for open runs: {e:#}");
                    true
                }
            };
        if !skip {
            if let Err(e) = helper::add_pending(&self.ctx.db, shoot_id, false) {
                plog!(
                    "helper",
                    "recording the pending run of shoot {shoot_id}: {e:#}"
                );
            }
            self.shared.lock().pending.push_back(Job {
                shoot_id,
                origin: Origin::Automatic { retry: false },
            });
            self.shared.wake.notify_one();
        }
    }

    /// An explicit run (the run button, the eval harness): not gated by
    /// `helper_enabled`, refused while one is queued or running for the shoot.
    pub fn enqueue_kind(&self, shoot_id: i64, kind: RunKind) -> Result<i64, EnqueueError> {
        if let Some(reason) = self.unavailable_reason() {
            return Err(EnqueueError::Unavailable(reason));
        }
        let _serial = self
            .shared
            .enqueue
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if review::get_shoot(&self.ctx.db, shoot_id)?.is_none() {
            return Err(EnqueueError::NotFound(shoot_id));
        }
        if helper::open_run(&self.ctx.db, shoot_id)?.is_some() {
            return Err(EnqueueError::Conflict(shoot_id));
        }
        let run_id = create_queued(&self.ctx, shoot_id, kind)?;
        self.shared.lock().pending.push_back(Job {
            shoot_id,
            origin: Origin::Explicit(run_id),
        });
        self.shared.wake.notify_one();
        Ok(run_id)
    }
}

impl HelperService {
    /// A chat message from the user: stored now, answered on a thread of its
    /// own so it never waits behind a background run. One turn per shoot at
    /// a time. Not gated by `helper_enabled`.
    pub fn chat(&self, shoot_id: i64, text: &str) -> Result<(), EnqueueError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(EnqueueError::Invalid("the message is empty".into()));
        }
        if text.chars().count() > chat::MESSAGE_MAX {
            return Err(EnqueueError::Invalid(format!(
                "the message is longer than {} characters",
                chat::MESSAGE_MAX
            )));
        }
        if let Some(reason) = self.unavailable_reason() {
            return Err(EnqueueError::Unavailable(reason));
        }
        if review::get_shoot(&self.ctx.db, shoot_id)?.is_none() {
            return Err(EnqueueError::NotFound(shoot_id));
        }
        let turn =
            chat::Turn::begin(&self.shared, shoot_id).ok_or(EnqueueError::ChatBusy(shoot_id))?;
        helper::add_chat_message(&self.ctx.db, shoot_id, "user", text)?;
        chat::spawn(Arc::clone(&self.ctx), turn, text.to_string());
        Ok(())
    }
}

/// A new `queued` run row, announced.
fn create_queued(ctx: &Ctx, shoot_id: i64, kind: RunKind) -> anyhow::Result<i64> {
    let run_id = helper::create_run(
        &ctx.db,
        shoot_id,
        kind,
        Some(&ctx.settings.helper_model),
        Some(&prompt::PROMPT_VERSION),
    )?;
    emit(ctx, shoot_id, run_id, RunStatus::Queued, None);
    Ok(run_id)
}

fn emit(ctx: &Ctx, shoot_id: i64, run_id: i64, status: RunStatus, error: Option<String>) {
    ctx.events.send(ServerEvent::HelperStatus {
        shoot_id,
        run_id,
        status: status.as_str().into(),
        error,
    });
}

fn log_availability(a: &Availability) {
    match a {
        Availability::Ready { auth } => plog!("helper", "claude CLI ready ({auth})"),
        Availability::Unavailable { reason } => plog!("helper", "helper unavailable: {reason}"),
    }
}

/// Empties the work dir (the child's cwd) and drops run dirs older than
/// `RUN_DIR_MAX_AGE`.
fn prepare_dirs(runs_dir: &Path, work_dir: &Path) {
    let _ = std::fs::remove_dir_all(work_dir);
    for dir in [runs_dir, work_dir] {
        if let Err(e) = std::fs::create_dir_all(dir) {
            plog!("helper", "creating {}: {e}", dir.display());
        }
    }
    let Ok(entries) = std::fs::read_dir(runs_dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > RUN_DIR_MAX_AGE);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn worker_loop(shared: &Shared, ctx: &Ctx) {
    let a = ctx.transport.availability();
    log_availability(&a);
    shared.lock().availability = Some(a);
    loop {
        let job = {
            let mut state = shared.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(job) = state.pending.pop_front() {
                    state.active = true;
                    break job;
                }
                state = shared.wake.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        };
        let availability = shared.lock().availability.clone();
        let mut started = None;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            run_job(ctx, &job, availability, &mut started)
        }));
        if outcome.is_err() {
            plog!("helper", "a run of shoot {} panicked", job.shoot_id);
            if let Some(run) = started {
                fail(ctx, &run, "the helper runner crashed".into());
            }
        }
        shared.lock().active = false;
    }
}

/// A job with its run row.
struct Started {
    shoot_id: i64,
    run_id: i64,
}

/// Ends the run as `failed` unless it already ended, and says so.
fn fail(ctx: &Ctx, run: &Started, error: String) {
    let error = truncate(&error, ERROR_MAX);
    match helper::end_open_run(&ctx.db, run.run_id, RunStatus::Failed, Some(&error)) {
        Ok(true) => {
            plog!("helper", "run {} failed: {error}", run.run_id);
            emit(
                ctx,
                run.shoot_id,
                run.run_id,
                RunStatus::Failed,
                Some(error),
            );
        }
        Ok(false) => {}
        Err(e) => plog!("helper", "marking run {} failed: {e:#}", run.run_id),
    }
}

/// `started` is set as soon as the job has a run row, so a panic can fail it.
fn run_job(
    ctx: &Ctx,
    job: &Job,
    availability: Option<Availability>,
    started: &mut Option<Started>,
) {
    let unavailable = match availability {
        Some(Availability::Unavailable { reason }) => Some(reason),
        _ => None,
    };
    // `focus`: None reviews the whole shoot, Some(ids) only the new photos.
    let (run_id, focus) = match job.origin {
        Origin::Explicit(run_id) => (run_id, None),
        Origin::Automatic { retry } => {
            // Left in `helper_pending`: the next start tries again.
            if unavailable.is_some() {
                return;
            }
            let retry = match helper::take_pending(&ctx.db, job.shoot_id) {
                Ok(row) => retry || row.unwrap_or(false),
                Err(e) => {
                    plog!(
                        "helper",
                        "clearing the pending run of shoot {}: {e:#}",
                        job.shoot_id
                    );
                    retry
                }
            };
            let focus = match helper::automatic_scope(&ctx.db, job.shoot_id) {
                Ok(RunScope::Nothing) => {
                    plog!(
                        "helper",
                        "shoot {}: nothing new since the last run; no run started",
                        job.shoot_id
                    );
                    return;
                }
                Ok(RunScope::WholeShoot) => None,
                Ok(RunScope::NewPhotos(ids)) => Some(ids),
                Err(e) => {
                    plog!(
                        "helper",
                        "checking shoot {} for new photos: {e:#}",
                        job.shoot_id
                    );
                    return;
                }
            };
            let created = create_queued(ctx, job.shoot_id, RunKind::Background).and_then(|id| {
                if retry {
                    helper::mark_retry(&ctx.db, id)?;
                }
                Ok(id)
            });
            match created {
                Ok(run_id) => (run_id, focus),
                Err(e) => {
                    plog!("helper", "creating a run for shoot {}: {e:#}", job.shoot_id);
                    return;
                }
            }
        }
    };
    let run = started.insert(Started {
        shoot_id: job.shoot_id,
        run_id,
    });
    if let Some(reason) = unavailable {
        fail(ctx, run, reason);
        return;
    }
    let prepared = (|| -> anyhow::Result<RunRequest> {
        let (system, user) = prompt::build(
            &ctx.db,
            run.shoot_id,
            run.run_id,
            focus.as_deref(),
            ctx.settings.helper_frames_per_burst,
        )?;
        let dir = ctx.runs_dir.join(run.run_id.to_string());
        std::fs::create_dir_all(&dir)?;
        let system_prompt_path = dir.join("system.md");
        std::fs::write(&system_prompt_path, system)?;
        Ok(RunRequest {
            run_id: run.run_id,
            system_prompt_path,
            prompt: user,
            model: ctx.settings.helper_model.clone(),
            max_turns: ctx.settings.helper_max_turns,
            max_budget_usd: ctx.settings.helper_max_budget_usd,
            session: SessionMode::New(Uuid::new_v4()),
            deny_tools: BACKGROUND_DENY.to_vec(),
            cwd: ctx.work_dir.clone(),
        })
    })();
    let req = match prepared {
        Ok(r) => r,
        Err(e) => {
            fail(ctx, run, format!("preparing the run: {e:#}"));
            return;
        }
    };
    match helper::start_queued_run(&ctx.db, run.run_id) {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            plog!("helper", "starting run {}: {e:#}", run.run_id);
            return;
        }
    }
    emit(ctx, run.shoot_id, run.run_id, RunStatus::Running, None);
    plog!(
        "helper",
        "run {} for shoot {} started ({}, budget ${})",
        run.run_id,
        run.shoot_id,
        req.model,
        req.max_budget_usd
    );

    let mut rate_limited = false;
    let mut mcp_ok = true;
    let outcome = ctx.transport.run(&req, &mut |event| match event {
        RunEvent::Init { tools, mcp_ok: ok } => {
            mcp_ok = ok;
            if !ok {
                plog!(
                    "helper",
                    "run {}: PhotoSift's MCP server did not connect",
                    run.run_id
                );
            } else if cfg!(debug_assertions) {
                plog!("helper", "run {}: tools {}", run.run_id, tools.join(", "));
            }
        }
        RunEvent::Retry { attempt, error } => {
            rate_limited |= error.contains("rate_limit");
            plog!(
                "helper",
                "run {}: API retry {attempt} ({error})",
                run.run_id
            );
        }
        RunEvent::AssistantText(text) => {
            if cfg!(debug_assertions) {
                plog!("helper", "run {}: {}", run.run_id, truncate(&text, 300));
            }
        }
        RunEvent::Result(_) => {}
    });
    let result = match outcome {
        Ok(r) => r,
        Err(e) => {
            fail(ctx, run, format!("{e:#}"));
            return;
        }
    };
    if let Err(e) = helper::finish_run(&ctx.db, run.run_id, &result.usage) {
        plog!("helper", "recording usage of run {}: {e:#}", run.run_id);
    }
    plog!(
        "helper",
        "run {} ended: {} (${:.3}, {} turns, {} in / {} out / {} cache-read tokens)",
        run.run_id,
        result.subtype,
        result.usage.cost_usd,
        result.usage.num_turns,
        result.usage.input_tokens,
        result.usage.output_tokens,
        result.usage.cache_read_tokens
    );
    let error = if result.is_error {
        Some(describe_error(&result, rate_limited, req.max_budget_usd))
    } else if !mcp_ok {
        Some("PhotoSift's MCP endpoint did not connect to claude".into())
    } else {
        Some(format!(
            "the helper ended without calling finish_run: {}",
            if result.result.is_empty() {
                "(no message)"
            } else {
                &result.result
            }
        ))
    };
    // `fail` leaves a run that `finish_run` already made `done` alone.
    if let Some(error) = error {
        fail(ctx, run, error);
    }
}

fn describe_error(r: &RunResult, rate_limited: bool, budget: f64) -> String {
    let text = r.result.to_lowercase();
    let detail = if r.result.is_empty() {
        r.subtype.clone()
    } else {
        format!("{}: {}", r.subtype, r.result)
    };
    if rate_limited || text.contains("usage limit") || text.contains("rate limit") {
        format!("Claude usage limit reached; try again when it resets ({detail})")
    } else if r.subtype.contains("budget") {
        format!("the run hit its ${budget} budget ({detail})")
    } else {
        format!("claude reported an error ({detail})")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}
