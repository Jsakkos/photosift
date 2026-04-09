use crate::db::schema::Database;
use crate::metadata::exif;
use crate::metadata::xmp;
use crate::pipeline::embedded;
use crate::state::AppState;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub folder_path: String,
    pub image_count: usize,
    pub last_viewed_index: usize,
}

#[tauri::command]
pub fn open_project(
    folder_path: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<ProjectInfo, String> {
    let folder = PathBuf::from(&folder_path);
    if !folder.is_dir() {
        return Err("Not a valid directory".into());
    }

    let photosift_dir = folder.join(".photosift");
    fs::create_dir_all(&photosift_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(photosift_dir.join("previews")).map_err(|e| e.to_string())?;

    let db_path = photosift_dir.join("cache.sqlite");
    let db = Database::open(&db_path).map_err(|e| e.to_string())?;

    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && embedded::is_supported_image(path))
        .collect();

    files.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .cmp(b.file_name().unwrap_or_default())
    });

    for (idx, path) in files.iter().enumerate() {
        let filepath = path.to_string_lossy().to_string();
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if db.image_exists(&filepath).unwrap_or(false) {
            continue;
        }

        let file_size = fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);
        let _ = db.insert_image(&filepath, &filename, "", file_size, idx as i32);
    }

    let all_images = db.get_all_images().map_err(|e| e.to_string())?;
    let image_ids: Vec<i64> = all_images.iter().map(|img| img.id).collect();
    let image_count = image_ids.len();
    let last_viewed = read_last_viewed(&photosift_dir.join("project.json")).unwrap_or(0);

    let prefetch_images: Vec<(i64, PathBuf)> = all_images
        .iter()
        .map(|img| (img.id, PathBuf::from(&img.filepath)))
        .collect();

    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    app_state.db = Some(db);
    app_state.project_folder = Some(folder);
    app_state.image_ids = image_ids.clone();
    app_state.current_index = last_viewed;
    app_state.preview_dir = Some(photosift_dir.join("previews"));
    app_state.prefetch.set_images(prefetch_images);
    app_state.prefetch.set_preview_dir(photosift_dir.join("previews"));
    drop(app_state);

    // Background: extract previews in PARALLEL with rayon, then thumbnails
    let db_path2 = db_path.clone();
    let preview_dir = photosift_dir.join("previews");
    let images_to_process: Vec<(i64, PathBuf)> = all_images
        .into_iter()
        .map(|img| (img.id, PathBuf::from(img.filepath)))
        .collect();

    std::thread::spawn(move || {
        // Pass 1: Extract previews in parallel (rayon uses all CPU cores)
        let start = std::time::Instant::now();
        images_to_process.par_iter().for_each(|(id, path)| {
            let preview_path = preview_dir.join(format!("{}.jpg", id));
            if preview_path.exists() { return; }

            if embedded::is_raw_file(path) {
                if let Ok(jpeg) = embedded::extract_embedded_jpeg(path) {
                    let _ = fs::write(&preview_path, &jpeg);
                }
            } else {
                // JPEG/TIFF: just copy
                if let Ok(data) = fs::read(path) {
                    let _ = fs::write(&preview_path, &data);
                }
            }
        });
        log::info!("Pass 1 (previews): {} images in {:?}", images_to_process.len(), start.elapsed());

        // Pass 2: EXIF + thumbnails (also parallel)
        let bg_db = match Database::open(&db_path2) {
            Ok(db) => db,
            Err(e) => { log::error!("BG DB failed: {}", e); return; }
        };

        // EXIF extraction in parallel, collect results
        let exif_results: Vec<_> = images_to_process.par_iter().map(|(id, path)| {
            let exif_data = exif::extract_exif(path).ok();
            let rating = xmp::read_rating(path);
            (*id, exif_data, rating)
        }).collect();

        // Write EXIF to DB (must be sequential — SQLite is single-writer)
        for (id, exif_data, rating) in &exif_results {
            if let Some(ref ed) = exif_data {
                let _ = bg_db.update_exif(
                    *id,
                    ed.capture_time.as_deref(),
                    ed.camera_model.as_deref(),
                    ed.lens.as_deref(),
                    ed.focal_length,
                    ed.aperture,
                    ed.shutter_speed.as_deref(),
                    ed.iso,
                    ed.width,
                    ed.height,
                    ed.orientation,
                );
            }
            if let Some(r) = rating {
                let _ = bg_db.set_star_rating(*id, *r);
            }
        }

        // Thumbnails in parallel (CPU-intensive decode+resize)
        let thumb_results: Vec<_> = images_to_process.par_iter().map(|(id, _)| {
            let preview_path = preview_dir.join(format!("{}.jpg", id));
            let thumb = generate_small_thumbnail(&preview_path);
            (*id, thumb)
        }).collect();

        // Write thumbnails to DB sequentially
        for (id, thumb) in thumb_results {
            if let Some(bytes) = thumb {
                let _ = bg_db.set_thumbnail(id, &bytes);
            }
        }
        log::info!("Pass 2 (EXIF+thumbnails) complete");
    });

    Ok(ProjectInfo {
        folder_path,
        image_count,
        last_viewed_index: last_viewed,
    })
}

/// Generate a tiny thumbnail without decoding full resolution.
/// Reads JPEG, decodes, and resizes to 200px with fastest filter.
fn generate_small_thumbnail(jpeg_path: &Path) -> Option<Vec<u8>> {
    use image::codecs::jpeg::JpegEncoder;
    use image::ImageReader;
    use std::io::Cursor;

    let img = ImageReader::open(jpeg_path).ok()?.decode().ok()?;
    let thumb = img.thumbnail(200, 200); // thumbnail() is faster than resize()
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, 70);
    img.write_with_encoder(encoder).ok()?;

    // Actually use the thumbnail, not the full image
    let mut buf2 = Cursor::new(Vec::new());
    let encoder2 = JpegEncoder::new_with_quality(&mut buf2, 70);
    thumb.write_with_encoder(encoder2).ok()?;
    Some(buf2.into_inner())
}

#[tauri::command]
pub fn get_project_info(state: State<'_, Mutex<AppState>>) -> Result<Option<ProjectInfo>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    match &app_state.project_folder {
        Some(folder) => Ok(Some(ProjectInfo {
            folder_path: folder.to_string_lossy().to_string(),
            image_count: app_state.image_count(),
            last_viewed_index: app_state.current_index,
        })),
        None => Ok(None),
    }
}

fn read_last_viewed(project_json_path: &Path) -> Option<usize> {
    let content = fs::read_to_string(project_json_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("last_viewed_index")?.as_u64().map(|v| v as usize)
}
