//! The evaluation harness (dev only): scores picks for past shoots against
//! what the user actually exported to each shoot's `Export\` folder.
//!
//! It never writes into a library folder. Originals are indexed in place
//! (`ImportMode::IndexInPlace`), `Export\` is never indexed, and the app
//! state runs with `Settings::read_only`, which refuses sidecars and the
//! Recycle Bin. Everything it writes lives under its eval dir: the scratch
//! data root `root\` (database, previews, settings) and `results.jsonl`.

pub mod exports;
pub mod score;
pub mod sweep;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::api::AppState;
use crate::config::{self, Settings};
use crate::db::helper::{self as helper_db, RunKind, RunRow, RunStatus};
use crate::db::{photos, review, DbPool};
use crate::ingest::events::EventKind;
use crate::ingest::job::{self, EventSpec, ImportMode, ImportRequest, JobCtx};
use crate::ingest::scan::{self, ItemKind, ScanOptions};
use crate::outcome::Outcome;
use crate::{plog, Overrides};
use exports::PhotoKey;
use score::Score;

/// Never indexed as originals: it holds the user's edits, i.e. the answers.
pub const EXCLUDE_DIRS: &[&str] = &["Export"];

const SCAN_OPTIONS: ScanOptions = ScanOptions {
    exclude_dirs: EXCLUDE_DIRS,
};

/// Generous: a few hundred RAWs enrich in minutes even on CPU.
const ENRICH_TIMEOUT: Duration = Duration::from_secs(2 * 3600);

#[derive(Debug, Clone)]
pub struct RunOptions {
    /// `<eval dir>`: holds `root\` (the scratch data root) and `results.jsonl`.
    pub eval_dir: PathBuf,
    /// Shoot folders, in order.
    pub shoots: Vec<PathBuf>,
    /// One helper run per shoot instead of the signals baseline.
    pub helper: bool,
    pub max_shoots: Option<usize>,
    pub label: String,
    /// `read_only` is forced on whatever this says.
    pub overrides: Overrides,
    /// How long to wait for one helper run.
    pub helper_timeout: Duration,
}

/// One scored shoot: a line of `results.jsonl` and of the printed table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShootRow {
    pub label: String,
    /// `baseline` or `helper`.
    pub mode: String,
    pub prompt_version: Option<String>,
    pub model: Option<String>,
    pub settings_hash: String,
    /// First 8 hex chars of the SHA-256 of the folder path; never the path.
    pub shoot_hash: String,
    pub shoot_id: i64,
    pub photos: usize,
    pub truth: usize,
    /// Export files no original could be found for, by name.
    pub unmatched: Vec<String>,
    pub keep: usize,
    pub score: Score,
    /// The helper's album proposals alone (not for the baseline).
    pub album: Option<Score>,
    pub run_id: Option<i64>,
    pub run_status: Option<String>,
    pub run_error: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cost_usd: f64,
    /// Local time the row was scored.
    pub at: String,
}

/// A listed shoot that could not be scored, by hash only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub shoot_hash: String,
    pub reason: String,
}

/// What `run_detailed` produced.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub rows: Vec<ShootRow>,
    pub skipped: Vec<Skipped>,
}

/// `PHOTOSIFT_EVAL_DIR` (empty = unset), else `<repo>\.photosift-eval`.
pub fn default_eval_dir() -> PathBuf {
    match std::env::var_os("PHOTOSIFT_EVAL_DIR") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => repo_dir().join(".photosift-eval"),
    }
}

/// `<repo>\tests\eval\shoots.txt` (gitignored).
pub fn default_shoots_file() -> PathBuf {
    repo_dir().join("tests").join("eval").join("shoots.txt")
}

fn repo_dir() -> PathBuf {
    let server = Path::new(env!("CARGO_MANIFEST_DIR"));
    server.parent().unwrap_or(server).to_path_buf()
}

/// One folder per line; blank lines and `#` comments are skipped.
pub fn read_shoots_file(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .map(|l| l.trim().trim_matches('"').trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from)
        .collect())
}

/// First 8 hex chars of the SHA-256 of the folder path as given (trailing
/// separators dropped).
pub fn shoot_hash(folder: &Path) -> String {
    hex8(normalize(folder).to_string_lossy().as_bytes())
}

fn hex8(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn normalize(folder: &Path) -> PathBuf {
    folder.components().collect()
}

/// Hash of every setting that can change a score (`read_only` is never
/// serialized).
fn settings_hash(settings: &Settings) -> String {
    hex8(toml::to_string(settings).unwrap_or_default().as_bytes())
}

/// The app state the harness runs on: `root`'s settings with the automatic
/// helper off and a library root inside `root`, and `read_only` on.
pub fn eval_state(root: &Path, port: u16, overrides: Overrides) -> anyhow::Result<AppState> {
    let mut settings = config::load(root)?;
    let library = root.join("library");
    if settings.helper_enabled || settings.library_root != library {
        // Explicit eval runs only; no path that could lead into the library.
        settings.helper_enabled = false;
        settings.library_root = library;
        let path = root.join("settings.toml");
        fs::write(&path, toml::to_string_pretty(&settings)?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    crate::build_state_with(
        root,
        port,
        Overrides {
            read_only: true,
            ..overrides
        },
    )
}

fn shoot_by_folder(db: &DbPool, folder: &str) -> anyhow::Result<Option<i64>> {
    db.with(|c| {
        c.query_row("SELECT id FROM shoots WHERE folder = ?1", [folder], |r| {
            r.get(0)
        })
        .optional()
    })
}

/// Indexes the photos under `folder` (not `Export\`, not videos) in place as
/// one shoot whose folder is `folder`, and returns its id. A folder indexed
/// by an earlier invocation is reused as is. Blocking.
pub fn index_in_place(state: &AppState, folder: &Path) -> anyhow::Result<i64> {
    let folder = normalize(folder);
    let folder_str = folder.to_string_lossy().into_owned();
    let hash = shoot_hash(&folder);
    if let Some(id) = shoot_by_folder(&state.db, &folder_str)? {
        plog!("eval", "shoot {hash}: already indexed as shoot {id}");
        return Ok(id);
    }
    let scan = scan::scan_with_options(&folder, &state.db, &state.settings, &SCAN_OPTIONS)?;
    for w in &scan.warnings {
        plog!("eval", "shoot {hash}: {w}");
    }
    let item_idxs: Vec<usize> = scan
        .items
        .iter()
        .filter(|it| it.kind == ItemKind::Photo)
        .map(|it| it.idx)
        .collect();
    if item_idxs.is_empty() {
        bail!("no photos found");
    }
    plog!("eval", "shoot {hash}: indexing {} photos", item_idxs.len());
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "eval".into());
    let req = ImportRequest {
        events: vec![EventSpec {
            item_idxs,
            kind: EventKind::Named,
            name,
        }],
        camera_offsets: HashMap::new(),
    };
    job::validate(&scan, &req).map_err(|e| anyhow!(e))?;
    let ctx = JobCtx {
        db: state.db.clone(),
        events: state.events.clone(),
        enrich: state.enrich.clone(),
        library_root: state.settings.library_root.clone(),
        cache_root: state.data_root.join("cache"),
    };
    job::run_with_mode(&ctx, &scan.scan_id, &scan, &req, ImportMode::IndexInPlace)
        .map_err(|e| anyhow!("indexing failed: {e}"))?;
    shoot_by_folder(&state.db, &folder_str)?
        .context("indexing recorded no shoot (was every photo a duplicate of another shoot?)")
}

fn wait_for_enrichment(state: &AppState, shoot_id: i64) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        let enriched = review::get_shoot(&state.db, shoot_id)?.is_some_and(|s| s.enriched);
        // The queue also covers moment grouping, which runs after the last photo.
        if enriched && state.enrich.status().queue == 0 {
            return Ok(());
        }
        if started.elapsed() > ENRICH_TIMEOUT {
            bail!("enrichment did not finish in {ENRICH_TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn photo_keys(db: &DbPool, shoot_id: i64) -> anyhow::Result<Vec<PhotoKey>> {
    let ids: Vec<i64> = db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id FROM photos WHERE shoot_id = ?1 AND trashed_at IS NULL \
             AND (raw_path IS NOT NULL OR jpeg_path IS NOT NULL) ORDER BY id",
        )?;
        let ids = stmt.query_map([shoot_id], |r| r.get(0))?.collect();
        ids
    })?;
    let mut keys = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(p) = photos::get_photo(db, id)? else {
            continue;
        };
        let Some(stem) = p
            .raw_path
            .as_deref()
            .or(p.jpeg_path.as_deref())
            .and_then(|f| Path::new(f).file_stem())
            .map(|s| s.to_string_lossy().into_owned())
        else {
            continue;
        };
        keys.push(PhotoKey {
            id,
            stem,
            capture_time: p.capture_time_orig,
            camera: p.camera,
        });
    }
    Ok(keys)
}

/// The top frame of each of the `k` bursts whose top `signal_score` is
/// highest (bursts without a score last, then in timeline order).
pub fn baseline_keep(review: &review::ShootReview, k: usize) -> HashSet<i64> {
    let mut tops: Vec<(Option<f64>, usize, i64)> = review
        .moments
        .iter()
        .flat_map(|m| &m.bursts)
        .enumerate()
        .filter_map(|(order, b)| {
            let id = *b.photo_ids.first()?;
            let score = review.photos.get(&id).and_then(|p| p.signal_score);
            Some((score, order, id))
        })
        .collect();
    tops.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => y.total_cmp(&x).then(a.1.cmp(&b.1)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });
    tops.into_iter().take(k).map(|(_, _, id)| id).collect()
}

/// Every live photo's (moment, burst) as the review screen lays them out.
pub fn burst_of(review: &review::ShootReview) -> HashMap<i64, (i64, i64)> {
    let mut out = HashMap::new();
    for m in &review.moments {
        for b in &m.bursts {
            for &id in &b.photo_ids {
                out.insert(id, (m.idx, b.idx));
            }
        }
    }
    out
}

/// Every `Export\` directory under `folder`, at any depth and in any case
/// (the same rule the scan's exclusion uses), sorted. An export directory's
/// own subfolders are not searched again: `match_exports` walks them.
fn find_export_dirs(folder: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).with_context(|| "reading the shoot folder")?;
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let is_export = EXCLUDE_DIRS
                .iter()
                .any(|x| entry.file_name().to_string_lossy().eq_ignore_ascii_case(x));
            if is_export {
                found.push(entry.path());
            } else {
                pending.push(entry.path());
            }
        }
    }
    if found.is_empty() {
        bail!("no Export folder; list only shoots that have one");
    }
    found.sort();
    Ok(found)
}

/// `match_exports` over every export directory, merged.
fn match_all_exports(export_dirs: &[PathBuf], keys: &[PhotoKey]) -> exports::ExportMatch {
    let mut all = exports::ExportMatch::default();
    for dir in export_dirs {
        let m = exports::match_exports(dir, keys);
        all.matched.extend(m.matched);
        all.unmatched.extend(m.unmatched);
    }
    all
}

/// Queues one eval run through the real helper and waits for it to end.
fn helper_run(state: &AppState, shoot_id: i64, timeout: Duration) -> anyhow::Result<RunRow> {
    helper_db::clear_shoot_proposals(&state.db, shoot_id)?;
    let run_id = state
        .helper
        .enqueue_kind(shoot_id, RunKind::Eval)
        .map_err(|e| anyhow!("starting the helper run: {e}"))?;
    let started = Instant::now();
    let finals =
        [RunStatus::Done, RunStatus::Failed, RunStatus::Interrupted].map(RunStatus::as_str);
    loop {
        let run = helper_db::get_run(&state.db, run_id)?.context("the run row vanished")?;
        // `finish_run` makes the run done before the runner records its
        // usage; the worker is idle only once that is written too.
        if finals.contains(&run.status.as_str()) && state.helper.status().queue == 0 {
            return helper_db::get_run(&state.db, run_id)?.context("the run row vanished");
        }
        if started.elapsed() > timeout {
            bail!("helper run {run_id} did not finish in {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn eval_shoot(
    state: &AppState,
    folder: &Path,
    opts: &RunOptions,
    settings_hash: &str,
) -> anyhow::Result<ShootRow> {
    let hash = shoot_hash(folder);
    let export_dirs = find_export_dirs(folder)?;
    let shoot_id = index_in_place(state, folder)?;
    plog!("eval", "shoot {hash}: waiting for enrichment");
    wait_for_enrichment(state, shoot_id)?;

    let matched = match_all_exports(&export_dirs, &photo_keys(&state.db, shoot_id)?);
    let truth: HashSet<i64> = matched.matched.iter().map(|(_, id)| *id).collect();
    let review = review::shoot_review(&state.db, shoot_id)?.context("the shoot vanished")?;
    let bursts = burst_of(&review);

    let (keep, album, run) = if opts.helper {
        plog!("eval", "shoot {hash}: helper run");
        let run = helper_run(state, shoot_id, opts.helper_timeout)?;
        // Proposals of earlier runs stay on photos this run did not cover.
        let proposals = helper_db::shoot_proposals(&state.db, shoot_id)?;
        let of_run: Vec<_> = proposals.values().filter(|p| p.run_id == run.id).collect();
        let keep = of_run
            .iter()
            .filter(|p| matches!(p.outcome, Outcome::Album | Outcome::Library))
            .map(|p| p.photo_id)
            .collect();
        let album = of_run
            .iter()
            .filter(|p| p.outcome == Outcome::Album)
            .map(|p| p.photo_id)
            .collect();
        (keep, Some(album), Some(run))
    } else {
        (baseline_keep(&review, truth.len()), None, None)
    };

    let row = ShootRow {
        label: opts.label.clone(),
        mode: if opts.helper { "helper" } else { "baseline" }.into(),
        prompt_version: run.as_ref().and_then(|r| r.prompt_version.clone()),
        model: run.as_ref().and_then(|r| r.model.clone()),
        settings_hash: settings_hash.to_string(),
        shoot_hash: hash,
        shoot_id,
        photos: review.photos.len(),
        truth: truth.len(),
        unmatched: matched.unmatched,
        keep: keep.len(),
        score: score::score(&keep, &truth, &bursts),
        album: album.map(|a: HashSet<i64>| score::score(&a, &truth, &bursts)),
        run_id: run.as_ref().map(|r| r.id),
        run_status: run.as_ref().map(|r| r.status.clone()),
        run_error: run.as_ref().and_then(|r| r.error.clone()),
        input_tokens: run.as_ref().and_then(|r| r.input_tokens).unwrap_or(0),
        output_tokens: run.as_ref().and_then(|r| r.output_tokens).unwrap_or(0),
        cache_read_tokens: run.as_ref().and_then(|r| r.cache_read_tokens).unwrap_or(0),
        cost_usd: run.as_ref().and_then(|r| r.cost_usd).unwrap_or(0.0),
        at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    Ok(row)
}

fn append_result(eval_dir: &Path, row: &ShootRow) -> anyhow::Result<()> {
    let path = eval_dir.join("results.jsonl");
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    writeln!(f, "{}", serde_json::to_string(row)?)?;
    Ok(())
}

/// Indexes, enriches and scores each shoot (up to `max_shoots`) on an
/// in-process server bound to a free `127.0.0.1` port, so the helper's
/// `/mcp` calls reach the same router the app serves. A shoot that fails is
/// logged and skipped. Each scored row is appended to `results.jsonl` as soon
/// as it is ready.
pub async fn run(opts: &RunOptions) -> anyhow::Result<Vec<ShootRow>> {
    Ok(run_detailed(opts).await?.rows)
}

/// `run`, also reporting the shoots that were skipped and why.
pub async fn run_detailed(opts: &RunOptions) -> anyhow::Result<RunOutcome> {
    let root = opts.eval_dir.join("root");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let state = {
        let (root, overrides) = (root.clone(), opts.overrides.clone());
        tokio::task::spawn_blocking(move || eval_state(&root, port, overrides)).await??
    };
    let app = crate::api::router(state.clone(), None);
    let server = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            plog!("eval", "server stopped: {e}");
        }
    });
    let opts = opts.clone();
    let result = tokio::task::spawn_blocking(move || run_shoots(&state, &opts)).await;
    server.abort();
    result?
}

fn run_shoots(state: &AppState, opts: &RunOptions) -> anyhow::Result<RunOutcome> {
    let settings_hash = settings_hash(&state.settings);
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for folder in opts
        .shoots
        .iter()
        .take(opts.max_shoots.unwrap_or(usize::MAX))
    {
        match eval_shoot(state, folder, opts, &settings_hash) {
            Ok(row) => {
                append_result(&opts.eval_dir, &row)?;
                rows.push(row);
            }
            Err(e) => {
                let shoot_hash = shoot_hash(folder);
                plog!("eval", "shoot {shoot_hash} skipped: {e:#}");
                skipped.push(Skipped {
                    shoot_hash,
                    reason: format!("{e:#}"),
                });
            }
        }
    }
    Ok(RunOutcome { rows, skipped })
}

/// `skipped N: <hash>: <reason>` lines, or nothing when none were skipped.
pub fn format_skipped(skipped: &[Skipped]) -> String {
    if skipped.is_empty() {
        return String::new();
    }
    let mut out = format!("skipped {}:\n", skipped.len());
    for s in skipped {
        out.push_str(&format!("  {}: {}\n", s.shoot_hash, s.reason));
    }
    out
}

/// The per-shoot table `run` prints.
pub fn format_table(rows: &[ShootRow]) -> String {
    let mut out = format!(
        "{:<8} {:>6} {:>5} {:>5} {:>5} {:>5} {:>4} {:>6} {:>5} {:>6} {:>6} {:>6} {:>9} {:>7}\n",
        "shoot",
        "photos",
        "truth",
        "unm",
        "keep",
        "exact",
        "near",
        "missed",
        "false",
        "prec",
        "rec",
        "rec~",
        "tokens",
        "cost"
    );
    for r in rows {
        let s = &r.score;
        out.push_str(&format!(
            "{:<8} {:>6} {:>5} {:>5} {:>5} {:>5} {:>4} {:>6} {:>5} {:>6.2} {:>6.2} {:>6.2} {:>9} {:>7.3}\n",
            r.shoot_hash,
            r.photos,
            r.truth,
            r.unmatched.len(),
            r.keep,
            s.exact,
            s.near,
            s.missed,
            s.false_picks,
            s.precision_strict,
            s.recall_strict,
            s.recall_lenient,
            r.input_tokens + r.output_tokens,
            r.cost_usd,
        ));
    }
    for r in rows.iter().filter(|r| !r.unmatched.is_empty()) {
        out.push_str(&format!(
            "{} unmatched: {}\n",
            r.shoot_hash,
            r.unmatched.join(", ")
        ));
    }
    out
}

/// Means per label from `results.jsonl`, side by side. A shoot scored more
/// than once under one label counts once, by its latest row. No labels =
/// every label in the file.
pub fn report(eval_dir: &Path, labels: &[String]) -> anyhow::Result<String> {
    let path = eval_dir.join("results.jsonl");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut order: Vec<String> = Vec::new();
    let mut latest: BTreeMap<(String, String), ShootRow> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: ShootRow = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                plog!("eval", "skipping a results line: {e}");
                continue;
            }
        };
        if !order.contains(&row.label) {
            order.push(row.label.clone());
        }
        latest.insert((row.label.clone(), row.shoot_hash.clone()), row);
    }
    let labels: Vec<String> = if labels.is_empty() {
        order
    } else {
        labels.to_vec()
    };
    type Metric = (&'static str, fn(&ShootRow) -> f64);
    let metrics: [Metric; 15] = [
        ("photos", |r| r.photos as f64),
        ("truth", |r| r.truth as f64),
        ("unmatched", |r| r.unmatched.len() as f64),
        ("keep", |r| r.keep as f64),
        ("exact", |r| r.score.exact as f64),
        ("near", |r| r.score.near as f64),
        ("missed", |r| r.score.missed as f64),
        ("false", |r| r.score.false_picks as f64),
        ("precision", |r| r.score.precision_strict),
        ("recall", |r| r.score.recall_strict),
        ("recall~", |r| r.score.recall_lenient),
        ("album recall~", |r| {
            r.album.as_ref().map_or(f64::NAN, |a| a.recall_lenient)
        }),
        ("tokens", |r| (r.input_tokens + r.output_tokens) as f64),
        ("cache read", |r| r.cache_read_tokens as f64),
        ("cost $", |r| r.cost_usd),
    ];
    let mut out = format!("{:<14}", "label");
    for l in &labels {
        out.push_str(&format!(" {l:>16}"));
    }
    out.push('\n');
    let rows_of = |label: &String| -> Vec<&ShootRow> {
        latest
            .iter()
            .filter(|((l, _), _)| l == label)
            .map(|(_, r)| r)
            .collect()
    };
    out.push_str(&format!("{:<14}", "shoots"));
    for l in &labels {
        out.push_str(&format!(" {:>16}", rows_of(l).len()));
    }
    out.push('\n');
    for (name, f) in metrics {
        out.push_str(&format!("{name:<14}"));
        for l in &labels {
            let values: Vec<f64> = rows_of(l)
                .into_iter()
                .map(f)
                .filter(|v| !v.is_nan())
                .collect();
            if values.is_empty() {
                out.push_str(&format!(" {:>16}", "-"));
            } else {
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                out.push_str(&format!(" {mean:>16.3}"));
            }
        }
        out.push('\n');
    }
    Ok(out)
}
