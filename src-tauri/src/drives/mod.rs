use serde::Serialize;
use sysinfo::Disks;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveInfo {
    pub mount_point: String,
    pub label: Option<String>,
    pub drive_letter: Option<String>,
    pub is_removable: bool,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

pub fn list_removable_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut out = Vec::new();
    for disk in disks.list() {
        if !disk.is_removable() {
            continue;
        }
        let mount_point = disk.mount_point().to_string_lossy().into_owned();
        let drive_letter = parse_drive_letter(&mount_point);
        let label = nonempty(disk.name().to_string_lossy().to_string());
        out.push(DriveInfo {
            mount_point,
            label,
            drive_letter,
            is_removable: true,
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
        });
    }
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out
}

fn parse_drive_letter(mount_point: &str) -> Option<String> {
    let bytes = mount_point.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        Some((bytes[0] as char).to_ascii_uppercase().to_string())
    } else {
        None
    }
}

fn nonempty(s: String) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
