pub mod api;
pub mod config;
pub mod db;
pub mod drives;
pub mod enrich;
pub mod events;
pub mod ingest;
pub mod media;
pub mod outcome;
#[doc(hidden)]
pub mod testsupport;
pub mod trash;
pub mod xmp;

use std::{path::Path, path::PathBuf, sync::Arc};

use api::AppState;
use events::EventHub;

/// Opens the database, starts the enrich worker and assembles shared state.
/// Split from `build_app` so the binary can hand the same state (db, settings,
/// scan cache, event hub) to the card watcher.
pub fn build_state(data_root: &Path, port: u16) -> anyhow::Result<AppState> {
    let settings = config::load(data_root)?;
    let db = db::open(&data_root.join("photosift.db"))?;
    let events = EventHub::new();
    let enrich =
        enrich::job::EnrichWorker::start(db.clone(), events.clone(), data_root, settings.clone());
    Ok(AppState {
        settings: Arc::new(settings),
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
    Ok(api::router(build_state(data_root, port)?, web_dist))
}
