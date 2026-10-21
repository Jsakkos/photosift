mod common;

use common::{spawn_server, spawn_server_with_web};
use photosift::config::Settings;

#[tokio::test]
async fn health_reports_ok() {
    let server = spawn_server().await;
    let resp = reqwest::get(format!("{}/api/health", server.base_url))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})
    );
}

#[tokio::test]
async fn settings_defaults_written() {
    let server = spawn_server().await;
    let path = server.data_root.path().join("settings.toml");
    assert!(path.exists(), "settings.toml should be written on startup");
    let settings: Settings = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    // Debug builds must not collide with the logon-started release: another
    // port and a library inside the dev data root.
    if cfg!(debug_assertions) {
        assert_eq!(
            settings.library_root,
            server.data_root.path().join("library")
        );
        assert_eq!(settings.port, 7421);
    } else {
        assert_eq!(
            settings.library_root,
            std::path::PathBuf::from(r"E:\photos\DSLR")
        );
        assert_eq!(settings.port, 7420);
    }
    assert_eq!(settings.event_gap_minutes, 120);
    assert_eq!(settings.everyday_max_photos, 25);
    assert_eq!(settings.moment_gap_seconds, 90);
    assert_eq!(settings.burst_phash_max, 12);
    assert_eq!(settings.burst_window_seconds, 10);
}

#[tokio::test]
async fn serves_index_when_web_dist_present() {
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(
        dist.path().join("index.html"),
        "<html><head><title>PhotoSift</title></head></html>",
    )
    .unwrap();
    let server = spawn_server_with_web(Some(dist.path().to_path_buf())).await;

    let root = reqwest::get(format!("{}/", server.base_url)).await.unwrap();
    assert_eq!(root.status(), 200);
    assert!(root.text().await.unwrap().contains("PhotoSift"));

    let client_route = reqwest::get(format!("{}/review/123", server.base_url))
        .await
        .unwrap();
    assert_eq!(client_route.status(), 200);
    assert!(client_route.text().await.unwrap().contains("PhotoSift"));

    let unknown_api = reqwest::get(format!("{}/api/nope", server.base_url))
        .await
        .unwrap();
    assert_eq!(unknown_api.status(), 404);
}

#[test]
fn existing_settings_keep_their_values() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("settings.toml"),
        "library_root = 'D:\\pics'\nport = 7420\n",
    )
    .unwrap();
    let settings = photosift::config::load(root.path()).unwrap();
    assert_eq!(settings.library_root, std::path::PathBuf::from(r"D:\pics"));
    assert_eq!(settings.port, 7420);
}
