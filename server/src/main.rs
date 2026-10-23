// Release builds use the GUI subsystem so the logon task opens no console
// window; `instance::redirect_stdio_if_detached` keeps output in a log file.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::{
    net::SocketAddr,
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    process::ExitCode,
    sync::Arc,
    time::Duration,
};

use photosift::{
    api, config,
    drives::{list_removable_drives, Watcher},
    ingest::scan,
    instance,
};

/// How long a second launch waits for a half-started first instance to answer.
const HEALTH_PATIENCE: Duration = Duration::from_secs(3);
const CARD_POLL: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> ExitCode {
    let data_root = config::data_root();
    let detached = instance::redirect_stdio_if_detached(&data_root);
    match run(&data_root, detached).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let message = format!("{e:#}");
            photosift::plog!("main", "PhotoSift cannot start: {message}");
            // A detached release has no console, so the log alone would leave
            // the user with a launch that silently did nothing.
            if detached {
                instance::show_error_dialog("PhotoSift cannot start", &message);
            }
            ExitCode::FAILURE
        }
    }
}

async fn run(data_root: &Path, detached: bool) -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let (open, no_browser) = (flag("--open"), flag("--no-browser"));

    let port = config::load(data_root)?.port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let url = format!("http://{addr}/");
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            // Binding is the single-instance lock: a failed bind plus a
            // PhotoSift health answer means we are the second launch.
            if instance::wait_for_health(addr, HEALTH_PATIENCE) {
                println!("PhotoSift is already running at {url}");
                if open && !no_browser {
                    if let Err(e) = instance::open_browser(&url) {
                        photosift::plog!("main", "could not open the browser: {e}");
                    }
                }
                return Ok(());
            }
            anyhow::bail!(
                "cannot listen on port {port} ({e}); the port is held by something that is not PhotoSift (no /api/health answer). Free it or change `port` in {}",
                data_root.join("settings.toml").display()
            );
        }
    };
    // Only the instance that owns the port may rotate the shared log.
    if detached {
        instance::rotate_detached_log(data_root);
    }
    let state = photosift::build_state(data_root, listener.local_addr()?.port())?;
    spawn_card_watcher(&state);
    let app = api::router(state, config::web_dist());
    photosift::plog!(
        "main",
        "PhotoSift {} listening on http://{addr} (data root {})",
        env!("CARGO_PKG_VERSION"),
        data_root.display()
    );
    if open && !no_browser {
        if let Err(e) = instance::open_browser(&url) {
            photosift::plog!("main", "could not open the browser: {e}");
        }
    }
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
                photosift::plog!("main", "card watcher tick panicked; continuing");
            }
            std::thread::sleep(CARD_POLL);
        });
    if let Err(e) = spawned {
        photosift::plog!("main", "could not start card watcher: {e}");
    }
}
