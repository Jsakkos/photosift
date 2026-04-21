#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub shoot_id: i64,
    pub phase: ImportPhase,
    pub current: usize,
    pub total: usize,
    pub current_filename: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Walking,
    Processing,
    Clustering,
    Finalizing,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportComplete {
    pub shoot_id: i64,
    pub photo_count: usize,
    pub dedup_skipped: usize,
}

/// Fired after a single photo has a DB row, a preview JPEG on disk, and
/// a thumb JPEG on disk — i.e. everything the UI needs to render it.
/// Stream target: shoot-list cards show "importing 42/200" live; a
/// future progressive cull view can append to `images` on each event.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPhotoReady {
    pub shoot_id: i64,
    pub photo_id: i64,
    pub filename: String,
    pub imported: usize,
    pub total: usize,
}
