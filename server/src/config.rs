use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "settings.toml";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub library_root: PathBuf,
    pub port: u16,
    pub event_gap_minutes: u32,
    pub everyday_max_photos: usize,
    pub moment_gap_seconds: i64,
    pub burst_phash_max: u32,
    pub burst_window_seconds: i64,
    /// Gates only the automatic run after enrichment; the run button and the
    /// eval harness still work when it is off.
    pub helper_enabled: bool,
    /// `"cli"` (the user's own `claude` login), `"cli-api-key"` (the same CLI
    /// with `ANTHROPIC_API_KEY` from PhotoSift's environment) or `"off"`.
    pub helper_transport: String,
    pub helper_model: String,
    /// Passed as `--max-turns`; its semantics are untested on the CLI, so the
    /// budget below is the cap that matters.
    pub helper_max_turns: u32,
    pub helper_max_budget_usd: f64,
    pub helper_frames_per_burst: u32,
    /// Set only by the eval harness (never read from or written to
    /// `settings.toml`): no sidecar is written and nothing is sent to the
    /// Recycle Bin, so a past shoot indexed in place stays untouched.
    #[serde(skip)]
    pub read_only: bool,
}

impl Settings {
    /// Defaults for a data root. Debug builds get another port and a library
    /// inside their own data root, so `cargo run` and `npm run dev` never
    /// collide with (or write into) the logon-started release.
    pub fn defaults_for(root: &Path) -> Self {
        let (library_root, port) = if cfg!(debug_assertions) {
            (root.join("library"), 7421)
        } else {
            (PathBuf::from(r"E:\photos\DSLR"), 7420)
        };
        Self {
            library_root,
            port,
            event_gap_minutes: 120,
            everyday_max_photos: 25,
            moment_gap_seconds: 90,
            burst_phash_max: 12,
            burst_window_seconds: 10,
            helper_enabled: true,
            helper_transport: "cli".into(),
            helper_model: "sonnet".into(),
            helper_max_turns: 120,
            helper_max_budget_usd: 3.0,
            helper_frames_per_burst: 3,
            read_only: false,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::defaults_for(&data_root())
    }
}

/// Resolves the data root: `PHOTOSIFT_HOME` (empty = unset), else
/// `~/.photosift-dev` in debug builds and `~/.photosift` in release.
pub fn data_root() -> PathBuf {
    match std::env::var_os("PHOTOSIFT_HOME") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => {
            let dir = if cfg!(debug_assertions) {
                ".photosift-dev"
            } else {
                ".photosift"
            };
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(dir)
        }
    }
}

/// Built web UI: `PHOTOSIFT_WEB_DIST` (empty = unset), else `<exe dir>\web`.
/// Returns None when the directory does not exist so the server still runs API-only.
pub fn web_dist() -> Option<PathBuf> {
    let dir = match std::env::var_os("PHOTOSIFT_WEB_DIST") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => std::env::current_exe().ok()?.parent()?.join("web"),
    };
    dir.is_dir().then_some(dir)
}

/// The `claude` CLI: `PHOTOSIFT_CLAUDE_BIN` (empty = unset), else the first
/// `claude.exe`, then `claude.cmd`, on `PATH`. The path is not checked here;
/// the helper reports a missing file as unavailable.
pub fn claude_bin() -> Option<PathBuf> {
    match std::env::var_os("PHOTOSIFT_CLAUDE_BIN") {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => {
            let path = std::env::var_os("PATH")?;
            ["claude.exe", "claude.cmd"].iter().find_map(|name| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join(name))
                    .find(|p| p.is_file())
            })
        }
    }
}

/// Loads `<root>/settings.toml`, writing defaults first if it is missing.
pub fn load(root: &Path) -> anyhow::Result<Settings> {
    std::fs::create_dir_all(root)
        .with_context(|| format!("creating data root {}", root.display()))?;
    let path = root.join(SETTINGS_FILE);
    if !path.exists() {
        let text = toml::to_string_pretty(&Settings::defaults_for(root))?;
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}
