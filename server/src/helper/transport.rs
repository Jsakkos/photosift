//! How a helper run reaches Claude. `ClaudeCli` spawns the official `claude`
//! CLI headless with PhotoSift's MCP endpoint attached and nothing else: no
//! built-in tools, no user settings, plugins, hooks, slash commands or memory.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use uuid::Uuid;

use super::kill_tree::KillTree;
use super::stream::{parse_line, RunEvent, RunResult};
use crate::plog;

/// `--permission-prompts` first shipped in this CLI version.
const PERMISSION_PROMPTS_SINCE: (u32, u32, u32) = (2, 1, 259);
/// `auth status` and `--version` answer in well under a second; a hung CLI
/// must not stall the helper thread.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// stderr lines kept for the error message when the CLI dies without a result.
const STDERR_TAIL: usize = 12;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// `auth` describes the login, e.g. `claude.ai (pro)`.
    Ready {
        auth: String,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMode {
    New(Uuid),
    /// A session id from an earlier run.
    Resume(String),
}

/// Which of the app's own runs a child is, named in its MCP URL so the
/// server can refuse what the run may not do even if the CLI's own deny list
/// fails (`mcp::post`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperClient {
    Background,
    Chat,
    Eval,
}

impl HelperClient {
    pub fn as_str(self) -> &'static str {
        match self {
            HelperClient::Background => "background",
            HelperClient::Chat => "chat",
            HelperClient::Eval => "eval",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "background" => Some(HelperClient::Background),
            "chat" => Some(HelperClient::Chat),
            "eval" => Some(HelperClient::Eval),
            _ => None,
        }
    }

    /// Tools this client may not call, as the CLI names them.
    pub fn denied(self) -> &'static [&'static str] {
        match self {
            HelperClient::Chat => super::CHAT_DENY,
            HelperClient::Background | HelperClient::Eval => super::BACKGROUND_DENY,
        }
    }

    /// Whether this client may call PhotoSift's tool `tool`.
    pub fn may_call(self, tool: &str) -> bool {
        !self
            .denied()
            .iter()
            .any(|d| d.strip_prefix("mcp__photosift__") == Some(tool))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    TimedOut(Duration),
    Cancelled,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::Cancelled => f.write_str("stopped by you"),
            StopReason::TimedOut(d) if d.as_secs() >= 60 => write!(
                f,
                "the run took longer than {} minutes and was stopped",
                d.as_secs() / 60
            ),
            StopReason::TimedOut(d) => write!(
                f,
                "the run took longer than {} seconds and was stopped",
                d.as_secs_f64().ceil()
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum StopState {
    #[default]
    Running,
    Stopped(StopReason),
    Finished,
}

/// Shared between a run and whoever may stop it (the cancel route, the
/// run's own timeout watchdog).
#[derive(Debug, Clone, Default)]
pub struct StopSignal(Arc<(Mutex<StopState>, Condvar)>);

impl StopSignal {
    fn lock(&self) -> MutexGuard<'_, StopState> {
        self.0 .0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Asks the run to stop; false when it already ended or was stopped.
    pub fn cancel(&self) -> bool {
        let mut s = self.lock();
        if *s != StopState::Running {
            return false;
        }
        *s = StopState::Stopped(StopReason::Cancelled);
        self.0 .1.notify_all();
        true
    }

    pub fn reason(&self) -> Option<StopReason> {
        match *self.lock() {
            StopState::Stopped(r) => Some(r),
            _ => None,
        }
    }

    fn finish(&self) {
        let mut s = self.lock();
        if *s == StopState::Running {
            *s = StopState::Finished;
        }
        self.0 .1.notify_all();
    }

    /// Blocks until the run is stopped or finished, or `timeout` passes (which
    /// stops it). `Some` when the child must be killed.
    fn watch(&self, timeout: Duration) -> Option<StopReason> {
        let deadline = Instant::now() + timeout;
        let mut s = self.lock();
        loop {
            match *s {
                StopState::Stopped(r) => return Some(r),
                StopState::Finished => return None,
                StopState::Running => {}
            }
            let now = Instant::now();
            if now >= deadline {
                let r = StopReason::TimedOut(timeout);
                *s = StopState::Stopped(r);
                return Some(r);
            }
            s = self
                .0
                 .1
                .wait_timeout(s, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub run_id: i64,
    pub system_prompt_path: PathBuf,
    pub prompt: String,
    pub model: String,
    pub max_turns: u32,
    pub max_budget_usd: f64,
    pub session: SessionMode,
    /// Sets the deny list and the client named in the MCP URL.
    pub client: HelperClient,
    /// The per-process secret that marks the MCP URL as the app's own child.
    pub mcp_token: String,
    /// Wall-clock limit; the child's process tree is killed when it passes.
    pub timeout: Duration,
    pub stop: StopSignal,
    /// An empty directory outside any repository, so no project settings,
    /// `CLAUDE.md` or auto-memory keyed on a git root reach the child.
    pub cwd: PathBuf,
}

pub type RunOutcome = RunResult;

pub trait Transport: Send + Sync {
    fn availability(&self) -> Availability;
    /// Runs to completion, calling `on_event` for every parsed stream event.
    /// `Ok` whenever the CLI reported a result, including an error result.
    fn run(
        &self,
        req: &RunRequest,
        on_event: &mut dyn FnMut(RunEvent),
    ) -> anyhow::Result<RunOutcome>;
}

/// Never available: `helper_transport = "off"`, or a value PhotoSift does
/// not know.
pub struct Off {
    reason: String,
}

impl Off {
    pub fn new() -> Self {
        Self::because("helper turned off in settings")
    }

    pub fn because(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl Default for Off {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for Off {
    fn availability(&self) -> Availability {
        Availability::Unavailable {
            reason: self.reason.clone(),
        }
    }

    fn run(&self, _: &RunRequest, _: &mut dyn FnMut(RunEvent)) -> anyhow::Result<RunOutcome> {
        anyhow::bail!("{}", self.reason)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliAuth {
    /// The user's own `claude auth login`. `ANTHROPIC_API_KEY` is removed from
    /// the child's environment so a stray key never switches billing.
    Subscription,
    /// `ANTHROPIC_API_KEY` passed through from PhotoSift's environment.
    ApiKey,
}

pub struct ClaudeCli {
    bin: Option<PathBuf>,
    auth: CliAuth,
    mcp_url: String,
    /// Extra child environment (tests point the fake CLI at its script).
    env: Vec<(String, String)>,
    /// Set from `claude --version` at availability time.
    permission_prompts: AtomicBool,
}

impl ClaudeCli {
    pub fn new(bin: Option<PathBuf>, auth: CliAuth, port: u16, env: Vec<(String, String)>) -> Self {
        Self {
            bin,
            auth,
            mcp_url: format!("http://127.0.0.1:{port}/mcp"),
            env,
            permission_prompts: AtomicBool::new(false),
        }
    }

    /// `.cmd`/`.bat` shims (npm installs) go through `cmd.exe /c`: std does
    /// that itself for those extensions, with the escaping cmd needs, and
    /// refuses an argument it cannot pass safely.
    fn command(&self, bin: &Path) -> Command {
        let mut cmd = Command::new(bin);
        cmd.env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1")
            .stdin(Stdio::null());
        if self.auth == CliAuth::Subscription {
            cmd.env_remove("ANTHROPIC_API_KEY");
        }
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // The release is a GUI-subsystem exe: a console child would flash a window.
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// The argv after the program name. The prompt follows `-p` directly so
    /// no variadic option (`--tools`, `--allowedTools`) can swallow it.
    pub fn run_args(&self, req: &RunRequest) -> Vec<String> {
        let url = format!(
            "{}?client=helper&kind={}&token={}",
            self.mcp_url,
            req.client.as_str(),
            req.mcp_token
        );
        let mcp = json!({"mcpServers": {"photosift": {"type": "http", "url": url}}});
        let mut a: Vec<String> = vec![
            "-p".into(),
            req.prompt.clone(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            mcp.to_string(),
            "--tools".into(),
            String::new(),
            "--allowedTools".into(),
            "mcp__photosift__*".into(),
            "--disallowedTools".into(),
            req.client.denied().join(","),
            "--permission-mode".into(),
            "dontAsk".into(),
        ];
        if self.permission_prompts.load(Ordering::Relaxed) {
            a.extend(["--permission-prompts".into(), "none".into()]);
        }
        a.extend([
            "--setting-sources".into(),
            "local".into(),
            "--disable-slash-commands".into(),
            "--system-prompt-file".into(),
            req.system_prompt_path.to_string_lossy().into_owned(),
            "--model".into(),
            req.model.clone(),
            "--max-turns".into(),
            req.max_turns.to_string(),
            "--max-budget-usd".into(),
            req.max_budget_usd.to_string(),
        ]);
        match &req.session {
            SessionMode::New(id) => a.extend(["--session-id".into(), id.to_string()]),
            SessionMode::Resume(id) => a.extend(["--resume".into(), id.clone()]),
        }
        a
    }

    fn probe(&self, bin: &Path, args: &[&str]) -> anyhow::Result<(ExitStatus, String)> {
        let mut cmd = self.command(bin);
        cmd.args(args);
        let (status, stdout, _) = output_within(cmd, PROBE_TIMEOUT)?;
        Ok((status, stdout))
    }
}

impl Transport for ClaudeCli {
    fn availability(&self) -> Availability {
        let unavailable = |reason: String| Availability::Unavailable { reason };
        let Some(bin) = &self.bin else {
            return unavailable(
                "the claude CLI was not found on PATH; install Claude Code or set PHOTOSIFT_CLAUDE_BIN"
                    .into(),
            );
        };
        if !bin.is_file() {
            return unavailable(format!("the claude CLI was not found at {}", bin.display()));
        }
        if self.auth == CliAuth::ApiKey
            && std::env::var_os("ANTHROPIC_API_KEY").is_none_or(|k| k.is_empty())
        {
            return unavailable(
                "helper_transport is cli-api-key but ANTHROPIC_API_KEY is not set for PhotoSift"
                    .into(),
            );
        }
        match self.probe(bin, &["--version"]) {
            Ok((status, out)) if status.success() => {
                let newer = parse_version(&out).is_some_and(|v| v >= PERMISSION_PROMPTS_SINCE);
                self.permission_prompts.store(newer, Ordering::Relaxed);
            }
            Ok((status, _)) => return unavailable(format!("`claude --version` failed ({status})")),
            Err(e) => return unavailable(format!("could not run the claude CLI: {e:#}")),
        }
        if self.auth == CliAuth::ApiKey {
            return Availability::Ready {
                auth: "api key".into(),
            };
        }
        let signed_out = || {
            unavailable(
                "Claude is signed out; run `claude auth login` in a terminal, then retry".into(),
            )
        };
        match self.probe(bin, &["auth", "status"]) {
            Ok((status, out)) => {
                let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
                let method = v["authMethod"].as_str().unwrap_or("none");
                if !status.success() || method == "none" || v["loggedIn"] == false {
                    return signed_out();
                }
                let auth = match v["subscriptionType"].as_str() {
                    Some(sub) => format!("{method} ({sub})"),
                    None => method.to_string(),
                };
                Availability::Ready { auth }
            }
            Err(e) => unavailable(format!("`claude auth status` failed: {e:#}")),
        }
    }

    fn run(
        &self,
        req: &RunRequest,
        on_event: &mut dyn FnMut(RunEvent),
    ) -> anyhow::Result<RunOutcome> {
        let bin = self
            .bin
            .as_deref()
            .ok_or_else(|| anyhow!("the claude CLI was not found"))?;
        if let Some(reason) = req.stop.reason() {
            anyhow::bail!("{reason}");
        }
        let mut cmd = self.command(bin);
        cmd.args(self.run_args(req))
            .current_dir(&req.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .with_context(|| format!("starting {}", bin.display()))?;
        let stdout = child.stdout.take().context("child stdout")?;
        let stderr = child.stderr.take().context("child stderr")?;
        let child = Arc::new(KillOnDrop::new(child));
        let watchdog = {
            let (child, stop, timeout) = (Arc::clone(&child), req.stop.clone(), req.timeout);
            std::thread::spawn(move || {
                if stop.watch(timeout).is_some() {
                    child.kill();
                }
            })
        };
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let stderr_thread = {
            let tail = Arc::clone(&tail);
            let run_id = req.run_id;
            std::thread::spawn(move || {
                for line in lossy_lines(stderr) {
                    // May echo model output; only debug builds log it.
                    if cfg!(debug_assertions) {
                        plog!("helper", "run {run_id} stderr: {line}");
                    }
                    let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                    t.push_back(line);
                    if t.len() > STDERR_TAIL {
                        t.pop_front();
                    }
                }
            })
        };
        let mut result = None;
        for line in lossy_lines(stdout) {
            if let Some(event) = parse_line(&line) {
                if let RunEvent::Result(r) = &event {
                    result = Some(r.clone());
                }
                on_event(event);
            }
        }
        req.stop.finish();
        let _ = watchdog.join();
        let status = child.wait().context("waiting for claude")?;
        let _ = stderr_thread.join();
        if result.is_none() {
            if let Some(reason) = req.stop.reason() {
                anyhow::bail!("{reason}");
            }
        }
        result.ok_or_else(|| {
            let tail = tail.lock().unwrap_or_else(|e| e.into_inner());
            let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
            anyhow!(
                "claude exited ({status}) without a result{}",
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(": {}", tail.join(" | "))
                }
            )
        })
    }
}

/// The child in its own job object: `kill` (and dropping it while the child
/// still runs) kills the child and everything it started.
struct KillOnDrop {
    child: Mutex<Child>,
    tree: Option<KillTree>,
}

impl KillOnDrop {
    fn new(child: Child) -> Self {
        let tree = KillTree::adopt(&child)
            .map_err(|e| plog!("helper", "no job object for the claude child: {e}"))
            .ok();
        Self {
            child: Mutex::new(child),
            tree,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Child> {
        self.child.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn kill(&self) {
        if let Some(tree) = &self.tree {
            tree.terminate();
        }
        let _ = self.lock().kill();
    }

    fn try_wait(&self) -> std::io::Result<Option<ExitStatus>> {
        self.lock().try_wait()
    }

    fn wait(&self) -> std::io::Result<ExitStatus> {
        self.lock().wait()
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Ok(None) = self.try_wait() {
            self.kill();
            let _ = self.wait();
        }
    }
}

fn lossy_lines(r: impl Read) -> impl Iterator<Item = String> {
    let mut reader = BufReader::new(r);
    std::iter::from_fn(move || {
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(String::from_utf8_lossy(&buf).trim_end().to_string()),
        }
    })
}

/// Runs `cmd` to completion with captured output, killing it after `timeout`.
fn output_within(
    mut cmd: Command,
    timeout: Duration,
) -> anyhow::Result<(ExitStatus, String, String)> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let child = KillOnDrop::new(child);
    let read = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut r) = r {
                let mut buf = Vec::new();
                let _ = r.read_to_end(&mut buf);
                s = String::from_utf8_lossy(&buf).into_owned();
            }
            s
        })
    };
    let out = read(stdout.map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = read(stderr.map(|s| Box::new(s) as Box<dyn Read + Send>));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            anyhow::bail!("no answer within {}s", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok((
        status,
        out.join().unwrap_or_default(),
        err.join().unwrap_or_default(),
    ))
}

/// The first `x.y.z` in `claude --version` output (`2.1.240 (Claude Code)`).
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    s.split_whitespace().find_map(|word| {
        let mut parts = word.split('.').map(|p| p.parse::<u32>().ok());
        match (parts.next()?, parts.next()?, parts.next()?, parts.next()) {
            (Some(a), Some(b), Some(c), None) => Some((a, b, c)),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("2.1.240 (Claude Code)\n"), Some((2, 1, 240)));
        assert_eq!(parse_version("claude 2.1.259"), Some((2, 1, 259)));
        assert_eq!(parse_version("garbage"), None);
        assert!(parse_version("2.1.259").unwrap() >= PERMISSION_PROMPTS_SINCE);
        assert!(parse_version("2.1.240").unwrap() < PERMISSION_PROMPTS_SINCE);
        assert!(parse_version("2.2.0").unwrap() >= PERMISSION_PROMPTS_SINCE);
    }
}
