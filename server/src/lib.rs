pub mod api;
pub mod config;
pub mod db;
pub mod drives;
pub mod enrich;
pub mod events;
pub mod helper;
pub mod ingest;
pub mod instance;
pub mod logging;
pub mod mcp;
pub mod media;
pub mod outcome;
#[doc(hidden)]
pub mod testsupport;
pub mod trash;
pub mod xmp;

use std::{path::Path, path::PathBuf, sync::Arc};

use api::AppState;
use events::EventHub;
use helper::transport::{ClaudeCli, CliAuth, Off, Transport};

/// What tests (and only tests) set per server instead of process env.
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    /// Replaces `config::claude_bin()` (`PHOTOSIFT_CLAUDE_BIN`, then `PATH`).
    pub claude_bin: Option<PathBuf>,
    /// Extra environment for every `claude` child.
    pub claude_env: Vec<(String, String)>,
}

/// Opens the database, starts the enrich worker and assembles shared state.
/// Split from `build_app` so the binary can hand the same state (db, settings,
/// scan cache, event hub) to the card watcher.
pub fn build_state(data_root: &Path, port: u16) -> anyhow::Result<AppState> {
    build_state_with(data_root, port, Overrides::default())
}

pub fn build_state_with(
    data_root: &Path,
    port: u16,
    overrides: Overrides,
) -> anyhow::Result<AppState> {
    let settings = Arc::new(config::load(data_root)?);
    let db = db::open(&data_root.join("photosift.db"))?;
    // Card thumbnails belong to scans, which live only in memory.
    let _ = std::fs::remove_dir_all(data_root.join("cache").join("scan"));
    let events = EventHub::new();
    let helper = helper::HelperService::start(
        db.clone(),
        events.clone(),
        data_root,
        settings.clone(),
        helper_transport(&settings, port, overrides),
    );
    let after_enriched = {
        let helper = helper.clone();
        move |shoot_id| helper.enqueue(shoot_id)
    };
    let enrich = enrich::job::EnrichWorker::start(
        db.clone(),
        events.clone(),
        data_root,
        (*settings).clone(),
        Box::new(after_enriched),
    );
    Ok(AppState {
        settings,
        helper,
        db,
        port,
        data_root: data_root.to_path_buf(),
        events,
        enrich,
        scans: Default::default(),
        cards: Default::default(),
        import_busy: Default::default(),
    })
}

/// The transport `helper_transport` names; `port` is where `/mcp` listens.
fn helper_transport(settings: &config::Settings, port: u16, o: Overrides) -> Arc<dyn Transport> {
    let cli = |auth| {
        let bin = o.claude_bin.clone().or_else(config::claude_bin);
        Arc::new(ClaudeCli::new(bin, auth, port, o.claude_env.clone())) as Arc<dyn Transport>
    };
    match settings.helper_transport.as_str() {
        "cli" => cli(CliAuth::Subscription),
        "cli-api-key" => cli(CliAuth::ApiKey),
        "off" => Arc::new(Off::new()),
        other => Arc::new(Off::because(format!(
            "unknown helper_transport {other:?} in settings (use cli, cli-api-key or off)"
        ))),
    }
}

/// Builds the full app (API + optional static web) rooted at `data_root`.
/// Taking paths explicitly keeps tests free of process-global env mutation.
/// `port` is the port the listener is bound to: the Host/Origin guard only
/// accepts requests addressed to it. Does not start the card watcher, so
/// tests stay deterministic; `main` does that.
pub fn build_app(
    data_root: &Path,
    web_dist: Option<PathBuf>,
    port: u16,
) -> anyhow::Result<axum::Router> {
    build_app_with(data_root, web_dist, port, Overrides::default())
}

pub fn build_app_with(
    data_root: &Path,
    web_dist: Option<PathBuf>,
    port: u16,
    overrides: Overrides,
) -> anyhow::Result<axum::Router> {
    Ok(api::router(
        build_state_with(data_root, port, overrides)?,
        web_dist,
    ))
}
