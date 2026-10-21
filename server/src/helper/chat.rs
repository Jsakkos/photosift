//! Chat: the user talks to the same Claude helper about one shoot. A turn is
//! a `claude -p` of kind `chat` on a thread of its own, resuming the shoot's
//! session. It may change suggestions and, when asked, the taste profile; it
//! cannot set outcomes (`CHAT_DENY`, also refused by the server). Message text is never logged.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use uuid::Uuid;

use super::stream::RunEvent;
use super::transport::{HelperClient, RunRequest, SessionMode};
use super::{describe_error, prompt, Ctx, Shared};
use crate::db::helper::{self, RunKind, RunStatus};
use crate::events::ServerEvent;
use crate::plog;

/// Longest message the user may send.
pub const MESSAGE_MAX: usize = 4000;

/// Marks a shoot as mid-turn; dropping it frees the shoot for the next one.
pub(super) struct Turn {
    shared: Arc<Shared>,
    shoot_id: i64,
}

impl Turn {
    /// `None` while the shoot already has a turn in flight.
    pub(super) fn begin(shared: &Arc<Shared>, shoot_id: i64) -> Option<Self> {
        let mut busy = shared.chatting.lock().unwrap_or_else(|e| e.into_inner());
        busy.insert(shoot_id).then(|| Self {
            shared: Arc::clone(shared),
            shoot_id,
        })
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        self.shared
            .chatting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.shoot_id);
    }
}

pub(super) fn spawn(ctx: Arc<Ctx>, turn: Turn, text: String) {
    let shoot_id = turn.shoot_id;
    let spawned = std::thread::Builder::new()
        .name("helper-chat".into())
        .spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| answer(&ctx, turn, &text)));
            let failure = match outcome {
                Ok(Ok(())) => return,
                Ok(Err(reason)) => reason,
                Err(_) => {
                    plog!("helper", "a chat turn of shoot {shoot_id} panicked");
                    "the helper crashed".to_string()
                }
            };
            say(&ctx, shoot_id, format!("Couldn't reach Claude: {failure}"));
        });
    if let Err(e) = spawned {
        plog!("helper", "could not start a chat thread: {e}");
    }
}

/// Stores a helper message and announces it.
fn say(ctx: &Ctx, shoot_id: i64, text: String) {
    if let Err(e) = helper::add_chat_message(&ctx.db, shoot_id, "helper", &text) {
        plog!("helper", "storing a chat reply for shoot {shoot_id}: {e:#}");
    }
    ctx.events.send(ServerEvent::ChatMessage {
        shoot_id,
        role: "helper".into(),
        text,
    });
}

/// One turn. `Err` is the reason to show the user. The shoot is free for the
/// next message before the turn is reported finished.
fn answer(ctx: &Ctx, turn: Turn, text: &str) -> Result<(), String> {
    let shoot_id = turn.shoot_id;
    let run_id = helper::create_run(
        &ctx.db,
        shoot_id,
        RunKind::Chat,
        Some(&ctx.settings.helper_model),
        Some(&prompt::PROMPT_VERSION),
    )
    .map_err(|e| format!("{e:#}"))?;
    let result = run(ctx, shoot_id, run_id, text);
    match &result {
        Ok(()) => {
            let _ = helper::end_open_run(&ctx.db, run_id, RunStatus::Done, None);
        }
        Err(reason) => {
            let _ = helper::end_open_run(&ctx.db, run_id, RunStatus::Failed, Some(reason));
            plog!("helper", "chat run {run_id} failed: {reason}");
        }
    }
    // After the status is written: a message sent right after the reply must
    // find the done session, not start a fresh one.
    drop(turn);
    result
}

fn run(ctx: &Ctx, shoot_id: i64, run_id: i64, text: &str) -> Result<(), String> {
    let session = match helper::chat_session(&ctx.db, shoot_id).map_err(|e| format!("{e:#}"))? {
        Some(id) => SessionMode::Resume(id),
        None => SessionMode::New(Uuid::new_v4()),
    };
    let session_id = match &session {
        SessionMode::New(id) => id.to_string(),
        SessionMode::Resume(id) => id.clone(),
    };
    helper::set_run_session(&ctx.db, run_id, &session_id).map_err(|e| format!("{e:#}"))?;

    let (system, user) =
        prompt::build_chat(&ctx.db, shoot_id, run_id, text).map_err(|e| format!("{e:#}"))?;
    let dir = ctx.runs_dir.join(run_id.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| format!("preparing the chat: {e}"))?;
    let system_prompt_path = dir.join("system.md");
    std::fs::write(&system_prompt_path, system).map_err(|e| format!("preparing the chat: {e}"))?;
    let req = RunRequest {
        run_id,
        system_prompt_path,
        prompt: user,
        model: ctx.settings.helper_model.clone(),
        max_turns: ctx.settings.helper_max_turns,
        max_budget_usd: ctx.settings.helper_max_budget_usd,
        session,
        client: HelperClient::Chat,
        mcp_token: ctx.mcp_token.clone(),
        cwd: ctx.work_dir.clone(),
    };
    if !helper::start_queued_run(&ctx.db, run_id).map_err(|e| format!("{e:#}"))? {
        return Err("the chat run was interrupted".into());
    }
    plog!(
        "helper",
        "chat run {run_id} for shoot {shoot_id} started ({})",
        req.model
    );

    let mut rate_limited = false;
    let mut mcp_ok = true;
    let outcome = ctx.transport.run(&req, &mut |event| match event {
        RunEvent::Init { mcp_ok: ok, .. } => mcp_ok = ok,
        RunEvent::Retry { error, .. } => rate_limited |= error.contains("rate_limit"),
        RunEvent::AssistantText(text) if !text.trim().is_empty() => say(ctx, shoot_id, text),
        RunEvent::AssistantText(_) | RunEvent::Result(_) => {}
    });
    let result = outcome.map_err(|e| format!("{e:#}"))?;
    if let Err(e) = helper::finish_run(&ctx.db, run_id, &result.usage) {
        plog!("helper", "recording usage of chat run {run_id}: {e:#}");
    }
    plog!(
        "helper",
        "chat run {run_id} ended: {} (${:.3}, {} turns, {} in / {} out / {} cache-read tokens)",
        result.subtype,
        result.usage.cost_usd,
        result.usage.num_turns,
        result.usage.input_tokens,
        result.usage.output_tokens,
        result.usage.cache_read_tokens
    );
    if result.is_error {
        return Err(describe_error(&result, rate_limited, req.max_budget_usd));
    }
    if !mcp_ok {
        return Err("PhotoSift's MCP endpoint did not connect to claude".into());
    }
    Ok(())
}
