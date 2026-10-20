mod guard;
mod import;
mod media;
mod photos;
mod scan;
mod shoots;
mod trash;
mod ws;

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use axum::{
    extract::State,
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

use crate::{
    config::Settings, db::DbPool, drives::CardRegistry, enrich::job::EnrichWorker,
    events::EventHub, ingest::scan::CardScan,
};

/// Scans kept for the import step; older ones are dropped, which just makes
/// the client scan again.
const SCAN_CACHE_LEN: usize = 8;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub data_root: PathBuf,
    pub db: DbPool,
    /// The port actually bound; the Host/Origin guard compares against it.
    pub port: u16,
    pub events: EventHub,
    pub enrich: EnrichWorker,
    pub scans: ScanCache,
    /// Cards the watcher has announced and that are still inserted.
    pub cards: CardRegistry,
    /// Set while an import job runs; see `api::import`.
    pub import_busy: Arc<AtomicBool>,
}

type ScanMap = (HashMap<String, Arc<CardScan>>, VecDeque<String>);

#[derive(Clone, Default)]
pub struct ScanCache {
    inner: Arc<Mutex<ScanMap>>,
}

impl ScanCache {
    pub fn insert(&self, scan: Arc<CardScan>) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (map, order) = &mut *guard;
        order.push_back(scan.scan_id.clone());
        map.insert(scan.scan_id.clone(), scan);
        while order.len() > SCAN_CACHE_LEN {
            if let Some(old) = order.pop_front() {
                map.remove(&old);
            }
        }
    }

    pub fn get(&self, scan_id: &str) -> Option<Arc<CardScan>> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.0.get(scan_id).cloned()
    }
}

/// Handler error with an HTTP status and a `{"error": ...}` body.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

pub fn router(state: AppState, web_dist: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/cards", get(cards))
        .route("/scan", post(scan::scan))
        .route("/scan/{scan_id}", get(scan::get_scan))
        .route("/scan/{scan_id}/events", post(scan::regroup))
        .route("/slug", post(scan::slug))
        .route("/settings", get(settings))
        .route("/import", post(import::start))
        .route("/import/current", get(import::current))
        .route("/import/{job_id}", get(import::job_state))
        .route("/shoots", get(shoots::list))
        .route("/shoots/{id}/review", get(shoots::review))
        .route("/shoots/{id}/trash", post(trash::trash_shoot))
        .route("/photos/{id}/outcome", post(photos::set_outcome))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state.clone());
    let guard = middleware::from_fn_with_state(state.clone(), guard::check);
    let app = Router::new()
        .route("/ws", get(ws::handler).with_state(state.clone()))
        // Top level, before the SPA fallback, so a missing id is a 404.
        .route("/media/preview/{id}", get(media::preview))
        .route("/media/thumb/{id}", get(media::thumb))
        .route("/media/scan/{scan_id}/{idx}", get(media::scan_thumb))
        .with_state(state)
        .nest("/api", api);
    let app = match web_dist {
        // Client-side routes (e.g. /review/123) fall back to index.html with 200.
        Some(dist) => app.fallback_service(
            ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html"))),
        ),
        None => app,
    };
    // Last, so it wraps every route including /ws and the static fallback.
    app.layer(guard)
}

async fn status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let s = state.enrich.status();
    Json(json!({
        "ai_backend": s.ai_backend,
        "embeddings": s.embeddings,
        "queue": s.queue,
    }))
}

/// The read-only settings the import page needs to preview folder paths.
async fn settings(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({ "library_root": state.settings.library_root }))
}

/// Cards announced so far and still inserted: lets a tab opened after the
/// `card_detected` broadcast still show the import banner.
async fn cards(State(state): State<AppState>) -> Json<Vec<crate::events::CardScanSummary>> {
    Json(state.cards.list())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
}
