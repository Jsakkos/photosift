mod commands;
mod db;
mod ingest;
mod metadata;
mod pipeline;
mod state;

use pipeline::protocol;
use state::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .try_init();

    let app_state = AppState::new();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Mutex::new(app_state))
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state: tauri::State<'_, Mutex<AppState>> =
                    window.state::<Mutex<AppState>>();
                let queue = state.lock().ok().map(|s| s.xmp_queue.clone());
                drop(state);
                if let Some(q) = queue {
                    q.drain();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::shoots::list_shoots,
            commands::shoots::get_shoot,
            commands::import::start_import,
            commands::import::cancel_import,
            commands::image::get_image_list,
            commands::image::get_image_metadata,
            commands::rating::set_rating,
            commands::culling::set_flag,
            commands::culling::set_destination,
            commands::culling::bulk_set_flag,
            commands::culling::undo_last,
            commands::culling::get_view_cursor,
            commands::culling::set_view_cursor,
            commands::culling::get_groups_for_shoot,
            commands::culling::set_group_cover,
            commands::culling::create_group_from_photos,
            commands::culling::ungroup_photos,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::recluster_shoot,
            commands::export::export_xmp,
        ]);

    let builder = protocol::register_protocol(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
