mod common;

use common::{spawn_server, spawn_server_with_web, TestServer};
use reqwest::header::{HOST, ORIGIN};
use serde_json::json;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Error as WsError};

fn port(server: &TestServer) -> u16 {
    server.base_url.rsplit(':').next().unwrap().parse().unwrap()
}

fn import_body() -> serde_json::Value {
    json!({ "scan_id": "nope", "events": [], "camera_offsets": {} })
}

#[tokio::test]
async fn wrong_host_is_refused_on_every_route() {
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(dist.path().join("index.html"), "<html>PhotoSift</html>").unwrap();
    let server = spawn_server_with_web(Some(dist.path().to_path_buf())).await;
    let client = reqwest::Client::new();

    // DNS rebinding: the browser connects to 127.0.0.1 but sends the attacker's name.
    for path in ["/api/health", "/", "/review/1", "/ws"] {
        let resp = client
            .get(format!("{}{path}", server.base_url))
            .header(HOST, format!("evil.example:{}", port(&server)))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 403, "{path}");
    }
    // The right name on the wrong port is also not us.
    let resp = client
        .get(format!("{}/api/health", server.base_url))
        .header(HOST, "127.0.0.1:1")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn both_loopback_hosts_are_accepted() {
    let server = spawn_server().await;
    let client = reqwest::Client::new();
    for host in ["127.0.0.1", "localhost", "LOCALHOST"] {
        let resp = client
            .get(format!("{}/api/health", server.base_url))
            .header(HOST, format!("{host}:{}", port(&server)))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "{host}");
    }
}

#[tokio::test]
async fn foreign_origin_cannot_post() {
    let server = spawn_server().await;
    let client = reqwest::Client::new();
    for origin in [
        "http://evil.example",
        "https://127.0.0.1:1",
        "http://127.0.0.1:1",
        "null",
    ] {
        let resp = client
            .post(format!("{}/api/import", server.base_url))
            .header(ORIGIN, origin)
            .json(&import_body())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 403, "{origin}");
    }
}

#[tokio::test]
async fn same_origin_and_originless_posts_are_allowed() {
    let server = spawn_server().await;
    let client = reqwest::Client::new();
    let p = port(&server);

    // Reaching the handler (404 for the unknown scan) proves the guard let it through.
    let curl_style = client
        .post(format!("{}/api/import", server.base_url))
        .json(&import_body())
        .send()
        .await
        .unwrap();
    assert_eq!(curl_style.status(), 404);

    for origin in [
        format!("http://127.0.0.1:{p}"),
        format!("http://localhost:{p}"),
    ] {
        let resp = client
            .post(format!("{}/api/import", server.base_url))
            .header(ORIGIN, origin.clone())
            .json(&import_body())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404, "{origin}");
    }
}

#[tokio::test]
async fn plain_gets_ignore_origin() {
    // Cross-origin reads are already blocked by the browser (no CORS headers),
    // and the web app's own asset requests may carry odd Origins.
    let server = spawn_server().await;
    let resp = reqwest::Client::new()
        .get(format!("{}/api/health", server.base_url))
        .header(ORIGIN, "http://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

async fn ws_connect(
    server: &TestServer,
    origin: Option<&str>,
) -> Result<(), tokio_tungstenite::tungstenite::Error> {
    let url = format!("{}/ws", server.base_url).replacen("http://", "ws://", 1);
    let mut req = url.into_client_request().unwrap();
    if let Some(o) = origin {
        req.headers_mut().insert(ORIGIN, o.parse().unwrap());
    }
    tokio_tungstenite::connect_async(req).await.map(|_| ())
}

#[tokio::test]
async fn websocket_upgrade_from_a_foreign_origin_is_refused() {
    let server = spawn_server().await;
    match ws_connect(&server, Some("http://evil.example")).await {
        Err(WsError::Http(resp)) => assert_eq!(resp.status(), 403),
        other => panic!("expected a 403 handshake rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn websocket_from_our_origin_or_no_origin_connects() {
    let server = spawn_server().await;
    ws_connect(&server, None).await.unwrap();
    let own = format!("http://127.0.0.1:{}", port(&server));
    ws_connect(&server, Some(&own)).await.unwrap();
}

#[tokio::test]
async fn unc_scan_sources_are_rejected() {
    let server = spawn_server().await;
    let client = reqwest::Client::new();
    for source in [
        r"\\attacker\share",
        "//attacker/share",
        r"\\?\UNC\attacker\share",
    ] {
        let resp = client
            .post(format!("{}/api/scan", server.base_url))
            .json(&json!({ "source": source }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "{source}");
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("UNC"), "{body}");
    }
}
