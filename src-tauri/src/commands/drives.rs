use crate::drives::{self, DriveInfo};

#[tauri::command]
pub fn list_removable_drives() -> Result<Vec<DriveInfo>, String> {
    Ok(drives::list_removable_drives())
}
