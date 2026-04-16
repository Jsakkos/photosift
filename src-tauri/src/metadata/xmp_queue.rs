use crate::metadata::xmp;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEBOUNCE_MS: u64 = 100;
const POLL_INTERVAL_MS: u64 = 50;

struct Entry {
    path: PathBuf,
    rating: i32,
    queued_at: Instant,
}

#[derive(Clone)]
pub struct XmpWriteQueue {
    pending: Arc<Mutex<HashMap<i64, Entry>>>,
}

impl XmpWriteQueue {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn enqueue(&self, image_id: i64, path: &Path, rating: i32) {
        let mut pending = self.pending.lock().unwrap();
        pending.insert(
            image_id,
            Entry {
                path: path.to_path_buf(),
                rating,
                queued_at: Instant::now(),
            },
        );
    }

    /// Spawn the background flush thread. Runs forever; entries older than
    /// DEBOUNCE_MS are flushed each tick.
    pub fn spawn_flusher(&self) {
        let pending = self.pending.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            let ready: Vec<(i64, PathBuf, i32)> = {
                let mut guard = pending.lock().unwrap();
                let now = Instant::now();
                let mut out = Vec::new();
                guard.retain(|id, entry| {
                    if now.duration_since(entry.queued_at) >= Duration::from_millis(DEBOUNCE_MS) {
                        out.push((*id, entry.path.clone(), entry.rating));
                        false
                    } else {
                        true
                    }
                });
                out
            };
            for (_id, path, rating) in ready {
                if let Err(e) = xmp::write_rating(&path, rating) {
                    log::error!("XMP write failed for {:?}: {}", path, e);
                }
            }
        });
    }

    /// Synchronously flush all pending entries. Call on shutdown.
    pub fn drain(&self) {
        let entries: Vec<(PathBuf, i32)> = {
            let mut guard = self.pending.lock().unwrap();
            guard
                .drain()
                .map(|(_, e)| (e.path, e.rating))
                .collect()
        };
        for (path, rating) in entries {
            if let Err(e) = xmp::write_rating(&path, rating) {
                log::error!("XMP drain write failed for {:?}: {}", path, e);
            }
        }
    }
}
