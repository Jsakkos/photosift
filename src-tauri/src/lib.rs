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

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init());

    // Dev-only MCP bridge for Claude-driven UI verification. The Tauri
    // plugin listens on 127.0.0.1:4000; the Node bridge
    // `tauri-plugin-mcp-server` (launched by Claude Code per
    // `.mcp.json`) connects over TCP and relays take_screenshot,
    // query_page, execute_js, etc. Not compiled into release builds.
    #[cfg(debug_assertions)]
    {
        log::info!("dev build: registering tauri-plugin-mcp on 127.0.0.1:4000");
        builder = builder.plugin(tauri_plugin_mcp::init_with_config(
            tauri_plugin_mcp::PluginConfig::new("photosift".to_string())
                .start_socket_server(true)
                .tcp_localhost(4000),
        ));
    }

    let builder = builder
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
            use crate::ai::eye::EyeStateProvider;
            use crate::ai::eye_onnx::OnnxEyeProvider;
            use crate::ai::face::{FaceProvider, YuNetProvider};
            use crate::ai::mock::{MockEyeProvider, MockFaceProvider};
            use crate::ai::mouth::{MockMouthProvider, MouthStateProvider};
            use crate::ai::mouth_onnx::OnnxMouthProvider;
            use crate::ai::{
                ensure_models_on_disk, AiProviderStatus, EyeProviderKind, MouthProviderKind,
            };
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

            // Extract bundled models (YuNet is always present; eye
            // classifier is optional and drops in if the user places
            // an `eye_state.onnx` in the same dir).
            let models_dir = match ensure_models_on_disk() {
                Ok(d) => Some(d),
                Err(e) => {
                    log::error!("Model extraction failed; disabling AI: {}", e);
                    None
                }
            };

            // Real face provider via YuNet with graceful fallback to mock if
            // model extraction or ORT init fails (e.g. onnxruntime dylib
            // missing). Surface the actual backend via ai_status so the UI
            // can badge it.
            let (face_provider, face_status): (Box<dyn FaceProvider>, AiProviderStatus) =
                match models_dir.as_ref() {
                    Some(dir) => match YuNetProvider::load(&dir.join("yunet.onnx"), true) {
                        Ok((yunet, status)) => (Box::new(yunet), status),
                        Err(e) => {
                            log::error!(
                                "YuNet load failed; disabling real face detection: {}",
                                e
                            );
                            (
                                Box::new(MockFaceProvider::default()),
                                AiProviderStatus::Disabled,
                            )
                        }
                    },
                    None => (
                        Box::new(MockFaceProvider::default()),
                        AiProviderStatus::Disabled,
                    ),
                };

            // Eye provider: drop in a real ONNX classifier the moment
            // a user places `eye_state.onnx` under ~/.photosift/models/.
            // Until then, the mock stays live and the UI hides its
            // alternating-0/1 signal via the eyeProvider === "mock" gate.
            let (eye_provider, eye_kind): (Box<dyn EyeStateProvider>, EyeProviderKind) =
                match models_dir.as_ref() {
                    Some(dir) => {
                        let eye_path = dir.join("eye_state.onnx");
                        if eye_path.exists() {
                            match OnnxEyeProvider::load(&eye_path) {
                                Ok(p) => (Box::new(p), EyeProviderKind::Onnx),
                                Err(e) => {
                                    log::error!(
                                        "Eye ONNX load failed at {}; falling back to mock: {}",
                                        eye_path.display(),
                                        e
                                    );
                                    (
                                        Box::new(MockEyeProvider::default()),
                                        EyeProviderKind::Mock,
                                    )
                                }
                            }
                        } else {
                            log::info!(
                                "No eye classifier at {} — using mock (drop in an ONNX file and restart to enable)",
                                eye_path.display()
                            );
                            (Box::new(MockEyeProvider::default()), EyeProviderKind::Mock)
                        }
                    }
                    None => (Box::new(MockEyeProvider::default()), EyeProviderKind::Mock),
                };

            let app_handle = app.handle().clone();
            let analyzed_for_cb = analyzed.clone();
            let failed_for_cb = failed.clone();
            let total_for_cb = total.clone();

            // Mouth provider: same pattern as eye. Drop a
            // `mouth_state.onnx` into ~/.photosift/models/ to swap in a
            // real smile/open-mouth classifier.
            let (mouth_provider, mouth_kind): (Box<dyn MouthStateProvider>, MouthProviderKind) =
                match models_dir.as_ref() {
                    Some(dir) => {
                        let mouth_path = dir.join("mouth_state.onnx");
                        if mouth_path.exists() {
                            match OnnxMouthProvider::load(&mouth_path) {
                                Ok(p) => (Box::new(p), MouthProviderKind::Onnx),
                                Err(e) => {
                                    log::error!(
                                        "Mouth ONNX load failed at {}; falling back to mock: {}",
                                        mouth_path.display(),
                                        e
                                    );
                                    (
                                        Box::new(MockMouthProvider::default()),
                                        MouthProviderKind::Mock,
                                    )
                                }
                            }
                        } else {
                            log::info!(
                                "No mouth classifier at {} — using mock",
                                mouth_path.display()
                            );
                            (
                                Box::new(MockMouthProvider::default()),
                                MouthProviderKind::Mock,
                            )
                        }
                    }
                    None => (
                        Box::new(MockMouthProvider::default()),
                        MouthProviderKind::Mock,
                    ),
                };

            // Cat detector: Tiny-YOLOv3 (COCO, class 15 = cat) loaded
            // from `~/.photosift/models/cat_detector.onnx` when present.
            // Falls back to the mock (no cats) if the file is missing or
            // fails to load, mirroring the eye/mouth hot-swap pattern.
            let cat_provider: Box<dyn crate::ai::cat::CatDetectorProvider> =
                match models_dir.as_ref() {
                    Some(dir) => {
                        let cat_path = dir.join("cat_detector.onnx");
                        if cat_path.exists() {
                            match crate::ai::cat::OnnxCatDetector::load(&cat_path) {
                                Ok(p) => Box::new(p),
                                Err(e) => {
                                    log::error!(
                                        "Cat detector ONNX load failed at {}; falling back to mock: {}",
                                        cat_path.display(),
                                        e
                                    );
                                    Box::new(crate::ai::cat::MockCatDetector)
                                }
                            }
                        } else {
                            log::info!(
                                "No cat detector at {} — using mock",
                                cat_path.display()
                            );
                            Box::new(crate::ai::cat::MockCatDetector)
                        }
                    }
                    None => Box::new(crate::ai::cat::MockCatDetector),
                };

            let spawned = crate::ai::spawn_worker(
                db_path,
                face_provider,
                eye_provider,
                mouth_provider,
                cat_provider,
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
                s.ai_status = face_status;
                s.ai_eye_provider = eye_kind;
                s.ai_mouth_provider = mouth_kind;
            }

            log::info!("AI provider status: {:?}", face_status);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::shoots::list_shoots,
            commands::shoots::get_shoot,
            commands::shoots::delete_shoot,
            commands::import::start_import,
            commands::import::cancel_import,
            commands::scan::scan_folder,
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
            commands::export::export_publish_direct,
            commands::ai::get_ai_status,
            commands::ai::cancel_ai_analysis,
            commands::ai::reanalyze_shoot,
            commands::ai::get_faces_for_photo,
            commands::ai::get_heatmap,
            commands::ai::get_shoot_sharpness_percentiles,
        ]);

    let builder = protocol::register_protocol(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
