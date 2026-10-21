// Shared by several test binaries; each uses only a subset.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub struct TestServer {
    pub base_url: String,
    pub data_root: TempDir,
    /// Aborts the accept loop, which drops the app once open connections close.
    pub task: tokio::task::AbortHandle,
}

pub async fn spawn_server() -> TestServer {
    spawn_server_with_web(None).await
}

/// Starts the real router in-process on an ephemeral port. The data root and
/// web dist are passed explicitly so parallel tests never share process env.
pub async fn spawn_server_with_web(web_dist: Option<PathBuf>) -> TestServer {
    let data_root = tempfile::tempdir().expect("temp data root");
    serve(data_root, web_dist).await
}

/// Like `spawn_server`, but settings.toml points the library at `library`
/// so imports land in a temp dir instead of the real photo library.
pub async fn spawn_server_with_library(library: &Path) -> TestServer {
    let data_root = tempfile::tempdir().expect("temp data root");
    let settings = photosift::config::Settings {
        library_root: library.to_path_buf(),
        ..Default::default()
    };
    std::fs::write(
        data_root.path().join("settings.toml"),
        toml::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("write settings");
    serve(data_root, None).await
}

/// Like `spawn_server_with_library`, plus the two face models copied from
/// `models_from` into the server's `<data root>\models`. Real models live
/// outside git; this is for `#[ignore]`d golden tests only.
pub async fn spawn_server_with_models(library: &Path, models_from: &Path) -> TestServer {
    let data_root = tempfile::tempdir().expect("temp data root");
    let settings = photosift::config::Settings {
        library_root: library.to_path_buf(),
        ..Default::default()
    };
    std::fs::write(
        data_root.path().join("settings.toml"),
        toml::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("write settings");
    let dest = data_root.path().join("models");
    std::fs::create_dir_all(&dest).expect("models dir");
    for name in [
        "face_detection_yunet_2023mar.onnx",
        "face_recognition_sface_2021dec.onnx",
    ] {
        std::fs::copy(models_from.join(name), dest.join(name)).unwrap_or_else(|e| {
            panic!("{name} in {models_from:?}: {e}; run scripts/fetch-models.ps1")
        });
    }
    serve(data_root, None).await
}

/// Starts a fresh app on the same data root, as after an app restart.
pub async fn respawn_server(old: TestServer) -> TestServer {
    serve(old.data_root, None).await
}

/// The helper's `claude` and extra child environment for one server.
#[derive(Clone)]
pub struct HelperSetup {
    pub claude_bin: PathBuf,
    pub env: Vec<(String, String)>,
}

/// Like `spawn_server_with_library`, with `edit` applied to the settings and
/// the helper running `setup.claude_bin` (normally the `fake-claude` binary).
pub async fn spawn_server_with_helper(
    library: &Path,
    edit: impl FnOnce(&mut photosift::config::Settings),
    setup: HelperSetup,
) -> TestServer {
    let data_root = tempfile::tempdir().expect("temp data root");
    let mut settings = photosift::config::Settings {
        library_root: library.to_path_buf(),
        ..Default::default()
    };
    edit(&mut settings);
    std::fs::write(
        data_root.path().join("settings.toml"),
        toml::to_string_pretty(&settings).expect("serialize settings"),
    )
    .expect("write settings");
    let overrides = photosift::Overrides {
        claude_bin: Some(setup.claude_bin),
        claude_env: setup.env,
        ..Default::default()
    };
    serve_with(data_root, None, overrides).await
}

/// Stops `old` (as far as a test can: its listener; the app and its helper
/// thread go once the caller has closed its connections) and starts a fresh
/// app with `setup` on the same data root, as after an app restart.
pub async fn restart_server_with_helper(old: TestServer, setup: HelperSetup) -> TestServer {
    old.task.abort();
    let overrides = photosift::Overrides {
        claude_bin: Some(setup.claude_bin),
        claude_env: setup.env,
        ..Default::default()
    };
    serve_with(old.data_root, None, overrides).await
}

async fn serve(data_root: TempDir, web_dist: Option<PathBuf>) -> TestServer {
    // Never the real `claude` on PATH: a test import would start a real run.
    let overrides = photosift::Overrides {
        claude_bin: Some(data_root.path().join("no-claude-in-tests.exe")),
        claude_env: Vec::new(),
        ..Default::default()
    };
    serve_with(data_root, web_dist, overrides).await
}

async fn serve_with(
    data_root: TempDir,
    web_dist: Option<PathBuf>,
    overrides: photosift::Overrides,
) -> TestServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    let app = photosift::build_app_with(data_root.path(), web_dist, addr.port(), overrides)
        .expect("build app");
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    })
    .abort_handle();
    TestServer {
        base_url: format!("http://{addr}"),
        data_root,
        task,
    }
}

#[allow(unused_imports)]
pub use photosift::testsupport::*;
