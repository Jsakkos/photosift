//! Server-pushed events. One broadcast hub feeds every WebSocket client.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
    /// A card with a `DCIM` folder was inserted but could not be scanned. The
    /// watcher does not retry until the card is reinserted; the user can still
    /// import it by hand from the Shoots page.
    CardScanFailed {
        source: String,
        error: String,
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
    /// The helper wrote proposals or your-call pairs for the shoot; one per
    /// tool call, so a page refetches the review once per batch.
    ProposalsChanged {
        shoot_id: i64,
    },
    /// A helper run changed status (`queued`, `running`, `done`, `failed`,
    /// `interrupted`).
    HelperStatus {
        shoot_id: i64,
        run_id: i64,
        status: String,
        error: Option<String>,
    },
    /// One chat message from the helper (`role` is `helper`), one per
    /// assistant text block. The user's own messages are not announced.
    ChatMessage {
        shoot_id: i64,
        role: String,
        text: String,
    },
    /// The helper rewrote the taste profile (through the MCP tool).
    TasteChanged,
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

/// Jobs whose state is remembered, running or finished; older ones are forgotten.
const JOB_LOG_LEN: usize = 8;

/// Where an import job stands, for a page that missed some of its events (the
/// socket dropped, or the tab opened mid-job).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    Running {
        done: usize,
        total: usize,
    },
    Done {
        shoot_ids: Vec<i64>,
        copied: usize,
        skipped_duplicates: usize,
    },
    Failed {
        error: String,
    },
}

/// The last few import jobs' latest state, fed by the same events the
/// WebSocket carries so the two can never disagree.
#[derive(Clone, Default)]
pub struct JobLog {
    inner: Arc<Mutex<VecDeque<(String, JobState)>>>,
}

impl JobLog {
    /// Registers a job as running before its first event, so a lookup right
    /// after the start request already finds it.
    pub fn begin(&self, job_id: &str) {
        self.set(job_id, JobState::Running { done: 0, total: 0 });
    }

    pub fn get(&self, job_id: &str) -> Option<JobState> {
        self.lock()
            .iter()
            .find(|(id, _)| id == job_id)
            .map(|(_, s)| s.clone())
    }

    /// The job still running, if any.
    pub fn running(&self) -> Option<String> {
        self.lock()
            .iter()
            .find(|(_, s)| matches!(s, JobState::Running { .. }))
            .map(|(id, _)| id.clone())
    }

    fn observe(&self, event: &ServerEvent) {
        match event {
            ServerEvent::ImportProgress {
                job_id,
                done,
                total,
                ..
            } => self.set(
                job_id,
                JobState::Running {
                    done: *done,
                    total: *total,
                },
            ),
            ServerEvent::ImportDone {
                job_id,
                shoot_ids,
                copied,
                skipped_duplicates,
            } => self.set(
                job_id,
                JobState::Done {
                    shoot_ids: shoot_ids.clone(),
                    copied: *copied,
                    skipped_duplicates: *skipped_duplicates,
                },
            ),
            ServerEvent::ImportFailed { job_id, error } => self.set(
                job_id,
                JobState::Failed {
                    error: error.clone(),
                },
            ),
            _ => {}
        }
    }

    fn set(&self, job_id: &str, state: JobState) {
        let mut jobs = self.lock();
        if let Some(entry) = jobs.iter_mut().find(|(id, _)| id == job_id) {
            entry.1 = state;
            return;
        }
        jobs.push_back((job_id.to_string(), state));
        while jobs.len() > JOB_LOG_LEN {
            jobs.pop_front();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<(String, JobState)>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Clone)]
pub struct EventHub {
    tx: broadcast::Sender<ServerEvent>,
    jobs: JobLog,
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
            jobs: JobLog::default(),
        }
    }

    /// Import job states, updated by every import event sent through the hub.
    pub fn jobs(&self) -> &JobLog {
        &self.jobs
    }

    /// Never fails: with no WebSocket connected the event is simply dropped.
    pub fn send(&self, event: ServerEvent) {
        // Recorded first, so a client that reacts to the event and asks for
        // the job's state sees at least what the event said.
        self.jobs.observe(&event);
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(job: &str, done: usize) -> ServerEvent {
        ServerEvent::ImportProgress {
            job_id: job.into(),
            done,
            total: 10,
            current: "x.jpg".into(),
        }
    }

    #[test]
    fn card_scan_failed_serialises_with_its_wire_name() {
        let json = serde_json::to_value(ServerEvent::CardScanFailed {
            source: "F:\\".into(),
            error: "unreadable".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "card_scan_failed", "source": "F:\\", "error": "unreadable"})
        );
    }

    #[test]
    fn helper_events_serialise_with_their_wire_names() {
        let json = serde_json::to_value(ServerEvent::HelperStatus {
            shoot_id: 2,
            run_id: 5,
            status: "done".into(),
            error: None,
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "helper_status", "shoot_id": 2, "run_id": 5,
                               "status": "done", "error": null})
        );
        let json = serde_json::to_value(ServerEvent::ChatMessage {
            shoot_id: 2,
            role: "helper".into(),
            text: "Hi".into(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "chat_message", "shoot_id": 2, "role": "helper",
                               "text": "Hi"})
        );
        let json = serde_json::to_value(ServerEvent::ProposalsChanged { shoot_id: 2 }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "proposals_changed", "shoot_id": 2})
        );
    }

    #[test]
    fn job_log_follows_a_job_from_running_to_done() {
        let hub = EventHub::new();
        hub.jobs().begin("j1");
        assert_eq!(
            hub.jobs().get("j1"),
            Some(JobState::Running { done: 0, total: 0 })
        );
        hub.send(progress("j1", 4));
        assert_eq!(
            hub.jobs().get("j1"),
            Some(JobState::Running { done: 4, total: 10 })
        );
        assert_eq!(hub.jobs().running().as_deref(), Some("j1"));

        hub.send(ServerEvent::ImportDone {
            job_id: "j1".into(),
            shoot_ids: vec![3],
            copied: 9,
            skipped_duplicates: 1,
        });
        assert_eq!(
            hub.jobs().get("j1"),
            Some(JobState::Done {
                shoot_ids: vec![3],
                copied: 9,
                skipped_duplicates: 1
            })
        );
        assert_eq!(hub.jobs().running(), None);
    }

    #[test]
    fn job_log_records_failure_and_forgets_old_jobs() {
        let hub = EventHub::new();
        hub.send(ServerEvent::ImportFailed {
            job_id: "bad".into(),
            error: "disk full".into(),
        });
        assert_eq!(
            hub.jobs().get("bad"),
            Some(JobState::Failed {
                error: "disk full".into()
            })
        );
        for n in 0..JOB_LOG_LEN {
            hub.jobs().begin(&format!("job{n}"));
        }
        assert_eq!(hub.jobs().get("bad"), None);
        assert!(hub.jobs().get("job0").is_some());
        assert_eq!(hub.jobs().get("never"), None);
    }
}
