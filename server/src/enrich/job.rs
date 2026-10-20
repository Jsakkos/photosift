//! Enrichment worker. The import job calls `enqueue` for every shoot it touched.

/// Placeholder until the enrichment worker lands; the import job already calls
/// it so wiring will not change.
pub fn enqueue(_shoot_id: i64) {}
