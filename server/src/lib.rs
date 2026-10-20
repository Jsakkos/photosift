pub mod api;
pub mod config;
pub mod db;
pub mod enrich;
pub mod events;
pub mod ingest;
pub mod media;
pub mod outcome;
pub mod trash;
pub mod xmp;

use std::{path::Path, path::PathBuf, sync::Arc};

use api::AppState;
use events::EventHub;

/// Builds the full app (API + optional static web) rooted at `data_root`.
/// Taking paths explicitly keeps tests free of process-global env mutation.
/// `port` is the port the listener is bound to: the Host/Origin guard only
/// accepts requests addressed to it.
pub fn build_app(
    data_root: &Path,
    web_dist: Option<PathBuf>,
    port: u16,
) -> anyhow::Result<axum::Router> {
    let settings = config::load(data_root)?;
    let db = db::open(&data_root.join("photosift.db"))?;
    let events = EventHub::new();
    let enrich =
        enrich::job::EnrichWorker::start(db.clone(), events.clone(), data_root, settings.clone());
    let state = AppState {
        settings: Arc::new(settings),
        db,
        port,
        data_root: data_root.to_path_buf(),
        events,
        enrich,
        scans: Default::default(),
        import_busy: Default::default(),
    };
    Ok(api::router(state, web_dist))
}
