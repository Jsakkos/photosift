use std::net::SocketAddr;

use photosift::config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_root = config::data_root();
    let port = config::load(&data_root)?.port;
    let app = photosift::build_app(&data_root, config::web_dist())?;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!(
        "PhotoSift listening on http://{addr} (data root {})",
        data_root.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}
