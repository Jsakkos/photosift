use crate::state::AppState;
use std::sync::Mutex;
use tauri::Manager;

pub fn register_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("photosift", move |ctx, request| {
        let uri = request.uri().to_string();

        let path = uri
            .strip_prefix("photosift://localhost")
            .or_else(|| uri.strip_prefix("https://photosift.localhost"))
            .unwrap_or("");

        let app_handle = ctx.app_handle();
        let state = app_handle.state::<Mutex<AppState>>();

        // Route: /thumb/{id} — read from file cache via photos.thumb_path
        if let Some(id_str) = path.strip_prefix("/thumb/") {
            let id_str = id_str.split('?').next().unwrap_or(id_str);
            if let Ok(image_id) = id_str.parse::<i64>() {
                let thumb_path = {
                    let app_state = match state.lock() {
                        Ok(s) => s,
                        Err(_) => return error_response(500, "Lock failed"),
                    };
                    app_state
                        .db
                        .as_ref()
                        .and_then(|db| db.get_photo_by_id(image_id).ok())
                        .map(|p| p.thumb_path.clone())
                };
                if let Some(tp) = thumb_path {
                    if let Ok(data) = std::fs::read(&tp) {
                        return jpeg_response(data);
                    }
                }
            }
            return jpeg_response(PLACEHOLDER_JPEG.to_vec());
        }

        // Route: /image/{id} — read preview via photos.preview_path
        if let Some(rest) = path.strip_prefix("/image/") {
            let id_str = rest.split('?').next().unwrap_or(rest);

            if let Ok(image_id) = id_str.parse::<i64>() {
                // 1. In-memory LRU cache (instant)
                if let Ok(app_state) = state.lock() {
                    if let Some(cached) = app_state.cache.get(image_id) {
                        return jpeg_response(cached);
                    }
                }

                // 2. Look up preview_path from DB (race-safe across shoot switches)
                let (preview_path, raw_path) = {
                    let app_state = match state.lock() {
                        Ok(s) => s,
                        Err(_) => return error_response(500, "Lock failed"),
                    };
                    let paths = app_state
                        .db
                        .as_ref()
                        .and_then(|db| db.get_photo_by_id(image_id).ok())
                        .map(|p| (p.preview_path.clone(), p.raw_path.clone()));
                    match paths {
                        Some(p) => p,
                        None => return error_response(404, "Photo not found"),
                    }
                };
                // Mutex released — all I/O below is lock-free

                // 3. Read from preview file cache
                if let Ok(data) = std::fs::read(&preview_path) {
                    if let Ok(app_state) = state.lock() {
                        app_state.cache.put(image_id, data.clone());
                        let idx = app_state
                            .image_ids
                            .iter()
                            .position(|&id| id == image_id)
                            .unwrap_or(0);
                        app_state.prefetch.prefetch_around(idx, 1);
                    }
                    return jpeg_response(data);
                }

                // 4. Fallback: decode from original RAW file
                let file_path = std::path::Path::new(&raw_path);
                match crate::pipeline::decoder::decode_to_jpeg(
                    file_path,
                    crate::pipeline::decoder::DecodeTier::Embedded,
                    85,
                ) {
                    Ok(jpeg_bytes) => {
                        if let Ok(app_state) = state.lock() {
                            app_state.cache.put(image_id, jpeg_bytes.clone());
                        }
                        return jpeg_response(jpeg_bytes);
                    }
                    Err(e) => {
                        log::error!("Decode failed for {}: {}", raw_path, e);
                    }
                }
            }
            return error_response(404, "Image not found");
        }

        error_response(404, "Unknown route")
    })
}

fn jpeg_response(data: Vec<u8>) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(200)
        .header("Content-Type", "image/jpeg")
        .header("Content-Length", data.len().to_string())
        .body(data)
        .unwrap()
}

fn error_response(status: u16, msg: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .body(msg.as_bytes().to_vec())
        .unwrap()
}

static PLACEHOLDER_JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
    0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43,
    0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09,
    0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12,
    0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20,
    0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29,
    0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
    0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03,
    0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D,
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
    0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
    0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72,
    0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45,
    0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
    0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75,
    0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3,
    0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6,
    0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9,
    0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
    0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4,
    0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01,
    0x00, 0x00, 0x3F, 0x00, 0x7B, 0x94, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xD9,
];
