mod ai;
mod commands;
mod db;
mod ingest;
mod metadata;
mod pipeline;
mod state;

use pipeline::protocol;
use state::AppState;
use std::sync::Mutex;
use tauri::Emitter;
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
        .setup(|app| {
            use crate::ai::mock::{MockEyeProvider, MockFaceProvider};
            use crate::ai::AiProviderStatus;
            use std::sync::atomic::Ordering;

            let state = app.state::<Mutex<AppState>>();
            let db_path = crate::db::schema::global_db_path();

            // Snapshot atomics we need to read from the progress callback.
            let (cancel, analyzed, failed, total) = {
                let s = state.lock().expect("state lock");
                (
                    s.ai_cancel.clone(),
                    s.ai_analyzed.clone(),
                    s.ai_failed.clone(),
                    s.ai_total.clone(),
                )
            };

            let app_handle = app.handle().clone();
            let analyzed_for_cb = analyzed.clone();
            let failed_for_cb = failed.clone();
            let total_for_cb = total.clone();

            let spawned = crate::ai::spawn_worker(
                db_path,
                Box::new(MockFaceProvider::default()),
                Box::new(MockEyeProvider::default()),
                cancel,
                analyzed,
                failed,
                move |photo_id, ok| {
                    let done = analyzed_for_cb.load(Ordering::SeqCst)
                        + failed_for_cb.load(Ordering::SeqCst);
                    let total_v = total_for_cb.load(Ordering::SeqCst);
                    let failed_v = failed_for_cb.load(Ordering::SeqCst);
                    let _ = app_handle.emit(
                        "ai-progress",
                        serde_json::json!({
                            "photoId": photo_id,
                            "ok": ok,
                            "done": done,
                            "total": total_v,
                            "failed": failed_v,
                        }),
                    );
                },
            );

            {
                let mut s = state.lock().expect("state lock");
                s.ai_worker = Some(spawned.handle);
                s.ai_status = AiProviderStatus::Cpu;
            }

            Ok(())
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
            commands::ai::get_ai_status,
            commands::ai::cancel_ai_analysis,
            commands::ai::reanalyze_shoot,
            commands::ai::get_faces_for_photo,
            commands::ai::get_heatmap,
        ]);

    let builder = protocol::register_protocol(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
