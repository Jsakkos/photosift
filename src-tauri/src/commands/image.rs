use crate::db::schema::PhotoRow;
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub id: i64,
    pub filepath: String,
    pub filename: String,
    pub capture_time: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub flag: String,
    pub destination: String,
    pub star_rating: i32,
}

impl From<PhotoRow> for ImageInfo {
    fn from(row: PhotoRow) -> Self {
        Self {
            id: row.id,
            filepath: row.raw_path,
            filename: row.filename,
            capture_time: row.exif_date,
            camera_model: row.camera,
            lens: row.lens,
            focal_length: row.focal_length,
            aperture: row.aperture,
            shutter_speed: row.shutter_speed,
            iso: row.iso,
            flag: row.flag,
            destination: row.destination,
            star_rating: row.star_rating,
        }
    }
}

#[tauri::command]
pub fn get_image_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<ImageInfo>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let shoot_id = app_state.current_shoot_id.ok_or("No shoot loaded")?;
    let rows = db.photos_for_shoot(shoot_id).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(ImageInfo::from).collect())
}

#[tauri::command]
pub fn get_image_metadata(
    image_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<ImageInfo, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("Database not open")?;
    let row = db.get_photo_by_id(image_id).map_err(|e| e.to_string())?;
    Ok(ImageInfo::from(row))
}
