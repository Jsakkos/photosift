use std::{
    net::SocketAddr,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
    time::Duration,
};

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

/// A plain thread: each tick may block on a slow card scan. Panics are caught
/// per tick (a lister panic would otherwise end detection for good; scan
/// panics are already contained inside `Watcher::tick`).
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
            let card = Arc::new(scan::scan(source, &db, &settings)?);
            // Same cache `POST /api/scan` fills, so the UI can import the
            // announced scan by id.
            scans.insert(card.clone());
            Ok(card)
        },
    )
    .with_registry(state.cards.clone());
    let spawned = std::thread::Builder::new()
        .name("card-watcher".into())
        .spawn(move || loop {
            if catch_unwind(AssertUnwindSafe(|| watcher.tick())).is_err() {
                eprintln!("card watcher tick panicked; continuing");
            }
            std::thread::sleep(CARD_POLL);
        });
    if let Err(e) = spawned {
        eprintln!("could not start card watcher: {e}");
    }
}
