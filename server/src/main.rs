use std::{net::SocketAddr, sync::Arc, time::Duration};

use photosift::{
    api, config,
    drives::{list_removable_drives, Watcher},
    ingest::scan,
};

const CARD_POLL: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_root = config::data_root();
    let port = config::load(&data_root)?.port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let state = photosift::build_state(&data_root, listener.local_addr()?.port())?;
    spawn_card_watcher(&state);
    let app = api::router(state, config::web_dist());
    println!(
        "PhotoSift listening on http://{addr} (data root {})",
        data_root.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

/// A plain thread: each tick may block on a slow card scan.
fn spawn_card_watcher(state: &api::AppState) {
    let (db, settings, scans) = (
        state.db.clone(),
        state.settings.clone(),
        state.scans.clone(),
    );
    let mut watcher = Watcher::new(
        Box::new(list_removable_drives),
        state.events.clone(),
        move |source| {
            let card = scan::scan(source, &db, &settings)?;
            // Same cache `POST /api/scan` fills, so the UI can import the
            // announced scan by id.
            scans.insert(Arc::new(card.clone()));
            Ok(card)
        },
    );
    std::thread::spawn(move || loop {
        watcher.tick();
        std::thread::sleep(CARD_POLL);
    });
}
