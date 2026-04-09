use crate::metadata::xmp;
use crate::state::AppState;
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingResult {
    pub image_id: i64,
    pub star_rating: i32,
}

#[tauri::command]
pub fn set_rating(
    image_id: i64,
    rating: i32,
    state: State<'_, Mutex<AppState>>,
) -> Result<RatingResult, String> {
    if !(0..=5).contains(&rating) {
        return Err("Rating must be 0-5".into());
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("No project open")?;

    db.set_star_rating(image_id, rating).map_err(|e| e.to_string())?;

    let img = db.get_image_by_id(image_id).map_err(|e| e.to_string())?;
    let filepath = img.filepath.clone();

    drop(app_state);

    xmp::write_rating(Path::new(&filepath), rating)?;

    Ok(RatingResult {
        image_id,
        star_rating: rating,
    })
}
