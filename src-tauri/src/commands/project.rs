use crate::db::schema::Database;
use crate::metadata::exif;
use crate::metadata::xmp;
use crate::pipeline::decoder;
use crate::pipeline::embedded;
use crate::state::AppState;
use sha2::{Digest, Sha256};
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

    let db_path = photosift_dir.join("cache.sqlite");
    let db = Database::open(&db_path).map_err(|e| e.to_string())?;

    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| embedded::is_supported_image(path))
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

        let file_hash = hash_file_header(path).unwrap_or_default();
        let file_size = fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);

        let id = db
            .insert_image(&filepath, &filename, &file_hash, file_size, idx as i32)
            .map_err(|e| e.to_string())?;

        if let Ok(exif_data) = exif::extract_exif(path) {
            let _ = db.update_exif(
                id,
                exif_data.capture_time.as_deref(),
                exif_data.camera_model.as_deref(),
                exif_data.lens.as_deref(),
                exif_data.focal_length,
                exif_data.aperture,
                exif_data.shutter_speed.as_deref(),
                exif_data.iso,
                exif_data.width,
                exif_data.height,
                exif_data.orientation,
            );
        }

        if let Some(rating) = xmp::read_rating(path) {
            let _ = db.set_star_rating(id, rating);
        }

        if let Ok(thumb_bytes) = decoder::generate_thumbnail(path) {
            let _ = db.set_thumbnail(id, &thumb_bytes);
        }
    }

    let image_ids: Vec<i64> = db
        .get_all_images()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|img| img.id)
        .collect();

    let image_count = image_ids.len();
    let last_viewed = read_last_viewed(&photosift_dir.join("project.json")).unwrap_or(0);

    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    app_state.db = Some(db);
    app_state.project_folder = Some(folder);
    app_state.image_ids = image_ids;
    app_state.current_index = last_viewed;

    // Initialize prefetch manager with image paths
    if let Some(ref db) = app_state.db {
        let prefetch_images: Vec<(i64, std::path::PathBuf)> = db
            .get_all_images()
            .unwrap_or_default()
            .into_iter()
            .map(|img| (img.id, std::path::PathBuf::from(&img.filepath)))
            .collect();
        app_state.prefetch.set_images(prefetch_images);
    }

    Ok(ProjectInfo {
        folder_path,
        image_count,
        last_viewed_index: last_viewed,
    })
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

fn hash_file_header(path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; 65536];
    let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
    buffer.truncate(bytes_read);
    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_last_viewed(project_json_path: &Path) -> Option<usize> {
    let content = fs::read_to_string(project_json_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("last_viewed_index")?.as_u64().map(|v| v as usize)
}

pub fn save_last_viewed(project_folder: &Path, index: usize) {
    let project_json = project_folder.join(".photosift").join("project.json");
    let json = serde_json::json!({ "last_viewed_index": index });
    let _ = fs::write(&project_json, serde_json::to_string_pretty(&json).unwrap_or_default());
}
