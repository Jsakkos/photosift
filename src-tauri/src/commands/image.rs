use crate::db::schema::ImageRow;
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
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: Option<i32>,
    pub star_rating: i32,
}

impl From<ImageRow> for ImageInfo {
    fn from(row: ImageRow) -> Self {
        Self {
            id: row.id,
            filepath: row.filepath,
            filename: row.filename,
            capture_time: row.capture_time,
            camera_model: row.camera_model,
            lens: row.lens,
            focal_length: row.focal_length,
            aperture: row.aperture,
            shutter_speed: row.shutter_speed,
            iso: row.iso,
            width: row.width,
            height: row.height,
            orientation: row.orientation,
            star_rating: row.star_rating,
        }
    }
}

#[tauri::command]
pub fn get_image_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<ImageInfo>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("No project open")?;
    let rows = db.get_all_images().map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(ImageInfo::from).collect())
}

#[tauri::command]
pub fn get_image_metadata(
    image_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<ImageInfo, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("No project open")?;
    let row = db.get_image_by_id(image_id).map_err(|e| e.to_string())?;
    Ok(ImageInfo::from(row))
}
