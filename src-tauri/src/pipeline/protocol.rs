use crate::pipeline::decoder::{decode_to_jpeg, DecodeTier};
use crate::state::AppState;
use std::sync::Mutex;
use tauri::Manager;

/// Register the `photosift://` custom protocol for serving images and thumbnails.
pub fn register_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("photosift", move |ctx, request| {
        let uri = request.uri().to_string();

        let path = uri
            .strip_prefix("photosift://localhost")
            .or_else(|| uri.strip_prefix("https://photosift.localhost"))
            .unwrap_or("");

        let app_handle = ctx.app_handle();
        let state = app_handle.state::<Mutex<AppState>>();
        let app_state = match state.lock() {
            Ok(s) => s,
            Err(_) => {
                return http::Response::builder()
                    .status(500)
                    .body(b"State lock failed".to_vec())
                    .unwrap();
            }
        };

        let db: &crate::db::schema::Database = match &app_state.db {
            Some(db) => db,
            None => {
                return http::Response::builder()
                    .status(404)
                    .body(b"No project open".to_vec())
                    .unwrap();
            }
        };

        // Route: /thumb/{id}
        if let Some(id_str) = path.strip_prefix("/thumb/") {
            let id_str = id_str.split('?').next().unwrap_or(id_str);
            if let Ok(image_id) = id_str.parse::<i64>() {
                if let Ok(Some(thumb)) = db.get_thumbnail(image_id) {
                    return http::Response::builder()
                        .status(200)
                        .header("Content-Type", "image/jpeg")
                        .header("Cache-Control", "max-age=3600")
                        .body(thumb)
                        .unwrap();
                }
            }
            return http::Response::builder()
                .status(404)
                .body(b"Thumbnail not found".to_vec())
                .unwrap();
        }

        // Route: /image/{id}?tier=...
        if let Some(rest) = path.strip_prefix("/image/") {
            let id_str = rest.split('?').next().unwrap_or(rest);
            let tier_str = uri
                .split("tier=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .unwrap_or("preview");

            let tier = match tier_str {
                "embedded" => DecodeTier::Embedded,
                "full" => DecodeTier::Full,
                _ => DecodeTier::Preview,
            };

            if let Ok(image_id) = id_str.parse::<i64>() {
                // Check LRU cache for preview tier
                if tier == DecodeTier::Preview {
                    if let Some(cached) = app_state.cache.get(image_id) {
                        return http::Response::builder()
                            .status(200)
                            .header("Content-Type", "image/jpeg")
                            .header("Content-Length", cached.len().to_string())
                            .header("X-Cache", "hit")
                            .body(cached)
                            .unwrap();
                    }
                }

                if let Ok(img) = db.get_image_by_id(image_id) {
                    let filepath = std::path::Path::new(&img.filepath);
                    let quality = match tier {
                        DecodeTier::Embedded => 85,
                        DecodeTier::Preview => 90,
                        DecodeTier::Full => 95,
                    };

                    match decode_to_jpeg(filepath, tier, quality) {
                        Ok(jpeg_bytes) => {
                            if tier == DecodeTier::Preview {
                                app_state.cache.put(image_id, jpeg_bytes.clone());
                            }

                            return http::Response::builder()
                                .status(200)
                                .header("Content-Type", "image/jpeg")
                                .header("Content-Length", jpeg_bytes.len().to_string())
                                .header("X-Cache", "miss")
                                .body(jpeg_bytes)
                                .unwrap();
                        }
                        Err(e) => {
                            log::error!("Decode failed for {}: {}", img.filepath, e);
                        }
                    }
                }
            }

            return http::Response::builder()
                .status(404)
                .body(b"Image not found".to_vec())
                .unwrap();
        }

        http::Response::builder()
            .status(404)
            .body(b"Unknown route".to_vec())
            .unwrap()
    })
}
