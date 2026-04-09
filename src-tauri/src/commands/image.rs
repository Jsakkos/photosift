#[tauri::command]
pub fn get_image_list() -> Result<Vec<()>, String> {
    Ok(vec![])
}

#[tauri::command]
pub fn get_image_metadata() -> Result<(), String> {
    Ok(())
}
