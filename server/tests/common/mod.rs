use std::path::PathBuf;

use tempfile::TempDir;

pub struct TestServer {
    pub base_url: String,
    pub data_root: TempDir,
}

pub async fn spawn_server() -> TestServer {
    spawn_server_with_web(None).await
}

/// Starts the real router in-process on an ephemeral port. The data root and
/// web dist are passed explicitly so parallel tests never share process env.
pub async fn spawn_server_with_web(web_dist: Option<PathBuf>) -> TestServer {
    let data_root = tempfile::tempdir().expect("temp data root");
    let app = photosift::build_app(data_root.path(), web_dist).expect("build app");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    TestServer {
        base_url: format!("http://{addr}"),
        data_root,
    }
}
