//! Card watcher behaviour, driven by a fake drive lister and scanner.

use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use photosift::{
    drives::{DriveInfo, Watcher},
    events::{EventHub, ServerEvent},
    ingest::scan::CardScan,
};
use tokio::sync::broadcast::{error::TryRecvError, Receiver};

fn drive(mount: &Path) -> DriveInfo {
    DriveInfo {
        mount_point: mount.to_string_lossy().into_owned(),
        label: None,
        drive_letter: None,
        is_removable: true,
        total_bytes: 0,
        available_bytes: 0,
    }
}

fn fake_scan(source: &Path) -> CardScan {
    CardScan {
        scan_id: format!("scan-{}", source.display()),
        source: source.to_path_buf(),
        items: vec![],
        cameras: vec![],
        already_imported: 0,
        events: vec![],
        warnings: vec![],
    }
}

struct Rig {
    watcher: Watcher,
    drives: Arc<Mutex<Vec<DriveInfo>>>,
    scans: Arc<AtomicUsize>,
    rx: Receiver<ServerEvent>,
}

impl Rig {
    fn new() -> Self {
        Self::with_scan(|p| Ok(fake_scan(p)))
    }

    fn with_scan(scan: impl Fn(&Path) -> anyhow::Result<CardScan> + Send + 'static) -> Self {
        let drives: Arc<Mutex<Vec<DriveInfo>>> = Default::default();
        let scans = Arc::new(AtomicUsize::new(0));
        let hub = EventHub::new();
        let rx = hub.subscribe();
        let (d, s) = (drives.clone(), scans.clone());
        let watcher = Watcher::new(Box::new(move || d.lock().unwrap().clone()), hub, move |p| {
            s.fetch_add(1, Ordering::SeqCst);
            scan(p)
        });
        Self {
            watcher,
            drives,
            scans,
            rx,
        }
    }

    fn set(&self, drives: &[&Path]) {
        *self.drives.lock().unwrap() = drives.iter().map(|p| drive(p)).collect();
    }

    fn events(&mut self) -> Vec<ServerEvent> {
        let mut out = vec![];
        loop {
            match self.rx.try_recv() {
                Ok(e) => out.push(e),
                Err(TryRecvError::Empty) => return out,
                Err(e) => panic!("{e}"),
            }
        }
    }
}

fn card_with_dcim() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("DCIM")).unwrap();
    d
}

#[test]
fn drive_without_dcim_is_ignored() {
    let plain = tempfile::tempdir().unwrap();
    let mut rig = Rig::new();
    rig.set(&[plain.path()]);
    for _ in 0..3 {
        rig.watcher.tick();
    }
    assert!(rig.events().is_empty());
    assert_eq!(rig.scans.load(Ordering::SeqCst), 0);
}

#[test]
fn new_card_is_announced_exactly_once_across_ticks() {
    let card = card_with_dcim();
    let mut rig = Rig::new();
    rig.set(&[card.path()]);
    for _ in 0..3 {
        rig.watcher.tick();
    }
    let events = rig.events();
    assert_eq!(events.len(), 1, "{events:?}");
    match &events[0] {
        ServerEvent::CardDetected { scan } => {
            assert_eq!(scan.source, card.path().to_string_lossy());
            assert_eq!(scan.scan_id, format!("scan-{}", card.path().display()));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(rig.scans.load(Ordering::SeqCst), 1);
}

#[test]
fn removal_then_reinsertion_announces_again() {
    let card = card_with_dcim();
    let mut rig = Rig::new();
    rig.set(&[card.path()]);
    rig.watcher.tick();
    rig.events();

    rig.set(&[]);
    rig.watcher.tick();
    match rig.events().as_slice() {
        [ServerEvent::CardRemoved { source }] => {
            assert_eq!(source, &card.path().to_string_lossy())
        }
        other => panic!("{other:?}"),
    }
    rig.watcher.tick();
    assert!(rig.events().is_empty(), "removal is announced once");

    rig.set(&[card.path()]);
    rig.watcher.tick();
    assert!(matches!(
        rig.events().as_slice(),
        [ServerEvent::CardDetected { .. }]
    ));
    assert_eq!(rig.scans.load(Ordering::SeqCst), 2);
}

#[test]
fn non_removable_drive_is_ignored() {
    let card = card_with_dcim();
    let mut rig = Rig::new();
    let mut d = drive(card.path());
    d.is_removable = false;
    *rig.drives.lock().unwrap() = vec![d];
    rig.watcher.tick();
    assert!(rig.events().is_empty());
    assert_eq!(rig.scans.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_scan_is_not_retried_until_the_card_is_reinserted() {
    let card = card_with_dcim();
    let fail = Arc::new(Mutex::new(true));
    let f = fail.clone();
    let mut rig = Rig::with_scan(move |p| {
        if *f.lock().unwrap() {
            anyhow::bail!("unreadable card")
        }
        Ok(fake_scan(p))
    });
    rig.set(&[card.path()]);
    for _ in 0..3 {
        rig.watcher.tick();
    }
    assert!(rig.events().is_empty());
    assert_eq!(rig.scans.load(Ordering::SeqCst), 1);

    // Pulling a card the UI never heard about is not announced either.
    rig.set(&[]);
    rig.watcher.tick();
    assert!(rig.events().is_empty());

    *fail.lock().unwrap() = false;
    rig.set(&[card.path()]);
    rig.watcher.tick();
    assert!(matches!(
        rig.events().as_slice(),
        [ServerEvent::CardDetected { .. }]
    ));
}

#[test]
fn two_cards_are_tracked_independently() {
    let (a, b) = (card_with_dcim(), card_with_dcim());
    let mut rig = Rig::new();
    rig.set(&[a.path()]);
    rig.watcher.tick();
    rig.set(&[a.path(), b.path()]);
    rig.watcher.tick();
    rig.set(&[b.path()]);
    rig.watcher.tick();
    let kinds: Vec<_> = rig
        .events()
        .iter()
        .map(|e| match e {
            ServerEvent::CardDetected { scan } => format!("+{}", scan.source),
            ServerEvent::CardRemoved { source } => format!("-{source}"),
            other => panic!("{other:?}"),
        })
        .collect();
    let (a, b) = (a.path().to_string_lossy(), b.path().to_string_lossy());
    assert_eq!(
        kinds,
        vec![format!("+{a}"), format!("+{b}"), format!("-{a}")]
    );
}
