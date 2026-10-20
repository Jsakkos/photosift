//! Removable-card watcher: announces cards with a `DCIM` folder as they are
//! inserted and removed. The drive list and the scan are injected so tests run
//! without hardware.

use std::{
    collections::{BTreeMap, HashMap},
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use sysinfo::Disks;

use crate::{
    events::{CardScanSummary, EventHub, ServerEvent},
    ingest::scan::CardScan,
};

#[derive(Debug, Clone, Serialize)]
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
    let mut out: Vec<DriveInfo> = disks
        .list()
        .iter()
        .filter(|d| d.is_removable())
        .map(|d| {
            let mount_point = d.mount_point().to_string_lossy().into_owned();
            DriveInfo {
                drive_letter: parse_drive_letter(&mount_point),
                label: nonempty(d.name().to_string_lossy().as_ref()),
                mount_point,
                is_removable: true,
                total_bytes: d.total_space(),
                available_bytes: d.available_space(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out
}

fn parse_drive_letter(mount_point: &str) -> Option<String> {
    let b = mount_point.as_bytes();
    (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic())
        .then(|| (b[0] as char).to_ascii_uppercase().to_string())
}

fn nonempty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

type Lister = Box<dyn Fn() -> Vec<DriveInfo> + Send>;
type Scanner = Box<dyn Fn(&Path) -> anyhow::Result<Arc<CardScan>> + Send>;

/// Cards currently announced, by mount point. The broadcast only reaches
/// connected clients, so a card inserted before the browser tab opens is
/// recovered from here via `GET /api/cards`.
#[derive(Clone, Default)]
pub struct CardRegistry {
    inner: Arc<Mutex<BTreeMap<String, CardScanSummary>>>,
}

impl CardRegistry {
    pub fn list(&self) -> Vec<CardScanSummary> {
        self.lock().values().cloned().collect()
    }

    fn insert(&self, mount: &str, summary: CardScanSummary) {
        self.lock().insert(mount.to_string(), summary);
    }

    fn remove(&self, mount: &str) {
        self.lock().remove(mount);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, CardScanSummary>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// What the watcher knows about a mounted drive with a `DCIM` folder.
enum Tracked {
    /// `CardDetected` went out with this `source`; `CardRemoved` must echo it.
    Announced(String),
    /// The scan failed. Kept so a broken card is not rescanned every tick; it
    /// is forgotten (and so retried) only once the drive disappears.
    ScanFailed,
}

pub struct Watcher {
    lister: Lister,
    hub: EventHub,
    scan: Scanner,
    cards: CardRegistry,
    /// Keyed by mount point. Starts empty, so a card already in the reader
    /// when the server starts is "new" on the first tick and gets announced.
    tracked: HashMap<String, Tracked>,
}

impl Watcher {
    pub fn new(
        lister: Lister,
        hub: EventHub,
        scan: impl Fn(&Path) -> anyhow::Result<Arc<CardScan>> + Send + 'static,
    ) -> Self {
        Self {
            lister,
            hub,
            scan: Box::new(scan),
            cards: CardRegistry::default(),
            tracked: HashMap::new(),
        }
    }

    fn scan_failed(&mut self, mount_point: String, error: String) {
        self.hub.send(ServerEvent::CardScanFailed {
            source: mount_point.clone(),
            error,
        });
        self.tracked.insert(mount_point, Tracked::ScanFailed);
    }

    /// Publishes announced cards into `cards` (the registry the API serves).
    pub fn with_registry(mut self, cards: CardRegistry) -> Self {
        self.cards = cards;
        self
    }

    /// One poll. Blocking (the scan reads EXIF off the card): call from a
    /// dedicated thread, not the async runtime.
    pub fn tick(&mut self) {
        let present: Vec<DriveInfo> = (self.lister)()
            .into_iter()
            .filter(|d| d.is_removable)
            .collect();

        let gone: Vec<String> = self
            .tracked
            .keys()
            .filter(|m| !present.iter().any(|d| &d.mount_point == *m))
            .cloned()
            .collect();
        for mount in gone {
            if let Some(Tracked::Announced(source)) = self.tracked.remove(&mount) {
                self.cards.remove(&mount);
                self.hub.send(ServerEvent::CardRemoved { source });
            }
        }

        for d in present {
            if self.tracked.contains_key(&d.mount_point) {
                continue;
            }
            // Not tracked when DCIM is absent, so it is cheaply re-checked
            // each tick (a card can finish mounting after the drive appears).
            if !Path::new(&d.mount_point).join("DCIM").is_dir() {
                continue;
            }
            // A decoder panic on one corrupt file must not end card detection.
            let result = catch_unwind(AssertUnwindSafe(|| (self.scan)(Path::new(&d.mount_point))));
            match result {
                Ok(Ok(scan)) => {
                    let summary = CardScanSummary::from_scan(&scan);
                    let source = summary.source.clone();
                    // Registry first, so a client reacting to the event and
                    // calling /api/cards always sees the card.
                    self.cards.insert(&d.mount_point, summary.clone());
                    self.hub.send(ServerEvent::CardDetected { scan: summary });
                    self.tracked
                        .insert(d.mount_point, Tracked::Announced(source));
                }
                Ok(Err(e)) => {
                    let error = format!("{e:#}");
                    eprintln!("card scan of {} failed: {error}", d.mount_point);
                    self.scan_failed(d.mount_point, error);
                }
                Err(_) => {
                    eprintln!("card scan of {} panicked", d.mount_point);
                    self.scan_failed(d.mount_point, "the scan crashed on a corrupt file".into());
                }
            }
        }
    }
}
