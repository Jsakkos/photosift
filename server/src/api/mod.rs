use std::{path::PathBuf, sync::Arc};

use axum::{http::StatusCode, routing::get, Json, Router};
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

use crate::{config::Settings, db::DbPool};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub data_root: PathBuf,
    pub db: DbPool,
}

pub fn router(state: AppState, web_dist: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state);
    let app = Router::new().nest("/api", api);
    match web_dist {
        // Client-side routes (e.g. /review/123) fall back to index.html with 200.
        Some(dist) => app.fallback_service(
            ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html"))),
        ),
        None => app,
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
}
