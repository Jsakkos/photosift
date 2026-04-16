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
