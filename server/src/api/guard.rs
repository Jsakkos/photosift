//! Browser-facing defences for a localhost server. Any website can make the
//! user's browser talk to 127.0.0.1, so the server itself must refuse requests
//! that did not originate from its own pages:
//! - the `Host` check stops DNS rebinding (a hostile name resolving to 127.0.0.1);
//! - the `Origin` check stops cross-site POSTs and WebSocket hijacking.

use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use super::AppState;

/// Vite's dev server origin; only trusted in debug builds.
const DEV_PORT: u16 = 5173;

fn host_ok(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

fn origin_allowed(origin: &str, port: u16) -> bool {
    let origin = origin.to_ascii_lowercase();
    let mut ports = vec![port];
    if cfg!(debug_assertions) {
        ports.push(DEV_PORT);
    }
    ports.iter().any(|p| {
        origin == format!("http://127.0.0.1:{p}") || origin == format!("http://localhost:{p}")
    })
}

fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// Router-level middleware: applies to API, `/ws` and static files alike.
pub async fn check(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    if !host_ok(headers, state.port) {
        return forbidden("unexpected Host header");
    }
    let reads_only = matches!(*req.method(), Method::GET | Method::HEAD);
    if !reads_only || is_websocket_upgrade(headers) {
        // No Origin (curl, scripts) is fine: only a browser, which always
        // sends one on these requests, can be carrying a hostile page's intent.
        if let Some(origin) = headers.get(header::ORIGIN) {
            let allowed = origin.to_str().is_ok_and(|o| origin_allowed(o, state.port));
            if !allowed {
                return forbidden("cross-origin request refused");
            }
        }
    }
    next.run(req).await
}

fn forbidden(why: &'static str) -> Response {
    (StatusCode::FORBIDDEN, why).into_response()
}
