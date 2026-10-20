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
        Self::with_scan(|p| Ok(Arc::new(fake_scan(p))))
    }

    fn with_scan(scan: impl Fn(&Path) -> anyhow::Result<Arc<CardScan>> + Send + 'static) -> Self {
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
        Ok(Arc::new(fake_scan(p)))
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

#[test]
fn panicking_scan_is_contained_and_not_retried() {
    let (bad, good) = (card_with_dcim(), card_with_dcim());
    let bad_path = bad.path().to_path_buf();
    let mut rig = Rig::with_scan(move |p| {
        if p == bad_path {
            panic!("corrupt file in decoder");
        }
        Ok(Arc::new(fake_scan(p)))
    });
    rig.set(&[bad.path()]);
    for _ in 0..3 {
        rig.watcher.tick();
    }
    assert!(rig.events().is_empty());
    assert_eq!(rig.scans.load(Ordering::SeqCst), 1);

    rig.set(&[bad.path(), good.path()]);
    rig.watcher.tick();
    match rig.events().as_slice() {
        [ServerEvent::CardDetected { scan }] => {
            assert_eq!(scan.source, good.path().to_string_lossy())
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(rig.scans.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cards_endpoint_lists_announced_cards() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let data_root = tempfile::tempdir().unwrap();
    let state = photosift::build_state(data_root.path(), port).unwrap();
    let app = photosift::api::router(state.clone(), None);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let drives: Arc<Mutex<Vec<DriveInfo>>> = Default::default();
    let d = drives.clone();
    let mut watcher = Watcher::new(
        Box::new(move || d.lock().unwrap().clone()),
        state.events.clone(),
        |p| Ok(Arc::new(fake_scan(p))),
    )
    .with_registry(state.cards.clone());

    let url = format!("http://127.0.0.1:{port}/api/cards");
    let list = || async {
        reqwest::get(&url)
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap()
    };
    assert_eq!(list().await, serde_json::json!([]));

    let card = card_with_dcim();
    *drives.lock().unwrap() = vec![drive(card.path())];
    // The tick blocks on the scanner, as it does on its own thread in main.
    watcher = tokio::task::spawn_blocking(move || {
        watcher.tick();
        watcher
    })
    .await
    .unwrap();
    let got = list().await;
    assert_eq!(got.as_array().unwrap().len(), 1, "{got}");
    assert_eq!(got[0]["source"], card.path().to_string_lossy().as_ref());

    drives.lock().unwrap().clear();
    tokio::task::spawn_blocking(move || watcher.tick())
        .await
        .unwrap();
    assert_eq!(list().await, serde_json::json!([]));
}
