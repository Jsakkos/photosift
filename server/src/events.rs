//! Server-pushed events. One broadcast hub feeds every WebSocket client.

use chrono::NaiveDateTime;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::ingest::{
    events::EventKind,
    scan::{CameraRange, CardScan, ItemKind},
};

/// A slow WebSocket client lags and skips frames rather than stalling the
/// import job; this is how many frames it may fall behind before that happens.
const HUB_CAPACITY: usize = 4096;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    CardDetected {
        scan: CardScanSummary,
    },
    CardRemoved {
        source: String,
    },
    ImportProgress {
        job_id: String,
        done: usize,
        total: usize,
        /// File name being processed, so the UI can show what is happening.
        current: String,
    },
    ImportDone {
        job_id: String,
        shoot_ids: Vec<i64>,
        copied: usize,
        skipped_duplicates: usize,
    },
    ImportFailed {
        job_id: String,
        error: String,
    },
    EnrichProgress {
        shoot_id: i64,
        done: usize,
        total: usize,
    },
    EnrichDone {
        shoot_id: i64,
    },
}

/// What a card-detected toast needs; the full `CardScan` is fetched on demand.
#[derive(Debug, Clone, Serialize)]
pub struct CardScanSummary {
    pub scan_id: String,
    pub source: String,
    pub photo_count: usize,
    pub video_count: usize,
    pub already_imported: usize,
    pub cameras: Vec<CameraRange>,
    pub events: Vec<EventSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventSummary {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub count: usize,
    pub kind: EventKind,
}

impl CardScanSummary {
    pub fn from_scan(scan: &CardScan) -> Self {
        let videos = scan
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Video)
            .count();
        Self {
            scan_id: scan.scan_id.clone(),
            source: scan.source.to_string_lossy().into_owned(),
            photo_count: scan.items.len() - videos,
            video_count: videos,
            already_imported: scan.already_imported,
            cameras: scan.cameras.clone(),
            events: scan
                .events
                .iter()
                .map(|e| EventSummary {
                    start: e.start,
                    end: e.end,
                    count: e.item_idxs.len(),
                    kind: e.kind,
                })
                .collect(),
        }
    }
}

#[derive(Clone)]
pub struct EventHub {
    tx: broadcast::Sender<ServerEvent>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> Self {
        Self {
            tx: broadcast::channel(HUB_CAPACITY).0,
        }
    }

    /// Never fails: with no WebSocket connected the event is simply dropped.
    pub fn send(&self, event: ServerEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}
