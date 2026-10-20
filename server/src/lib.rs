pub mod api;
pub mod config;
pub mod db;
pub mod media;
pub mod outcome;

use std::{path::Path, path::PathBuf, sync::Arc};

use api::AppState;

/// Builds the full app (API + optional static web) rooted at `data_root`.
/// Taking paths explicitly keeps tests free of process-global env mutation.
pub fn build_app(data_root: &Path, web_dist: Option<PathBuf>) -> anyhow::Result<axum::Router> {
    let settings = config::load(data_root)?;
    let db = db::open(&data_root.join("photosift.db"))?;
    let state = AppState {
        settings: Arc::new(settings),
        db,
        data_root: data_root.to_path_buf(),
    };
    Ok(api::router(state, web_dist))
}
