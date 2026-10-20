//! The import job: copies the chosen events from a scanned card into the
//! library, verifies and dedupes every file, renders previews and records the
//! rows. Blocking; the API runs it on its own thread.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use chrono::{Datelike, Duration};
use rayon::prelude::*;
use serde::Deserialize;

use super::copy::{
    copy_verified_expecting, hash_file, sha256_hex, suffixed_stem, sweep_partials, write_verified,
    Member, NameClaims,
};
use super::events::EventKind;
use super::naming::folder_path;
use super::scan::{CardScan, ItemKind, ScanItem};
use crate::db::photos::{
    attach_jpegs, commit_import_event, known_hashes, photo_for_file_hash, shoot_ids_of_photos,
    Attachment, NewFile, NewPhoto, NewShoot, PhotoBundle, ShootKind,
};
use crate::db::DbPool;
use crate::enrich::job::EnrichWorker;
use crate::events::{EventHub, ServerEvent};
use crate::media::{embedded, exif, phash, render};

/// Item processing is dominated by JPEG decode and disk waits on a card, so
/// more workers than cores still helps.
const WORKERS: usize = 16;

#[derive(Debug, Clone, Deserialize)]
pub struct EventSpec {
    pub item_idxs: Vec<usize>,
    pub kind: EventKind,
    /// Folder slug source for named events; ignored for everyday ones.
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportRequest {
    pub events: Vec<EventSpec>,
    /// Seconds to add to each camera's clock, keyed by `ScanItem::camera`.
    #[serde(default)]
    pub camera_offsets: HashMap<String, i64>,
}

/// Rejects requests that refer to items the scan does not have, that would
/// import one item twice, or that file an Everyday event spanning several
/// months into a single month folder (files never move after ingest).
pub fn validate(scan: &CardScan, req: &ImportRequest) -> Result<(), String> {
    if req.events.is_empty() {
        return Err("no events to import".into());
    }
    let mut seen = HashSet::new();
    for ev in &req.events {
        if ev.item_idxs.is_empty() {
            return Err("an event has no items".into());
        }
        for &idx in &ev.item_idxs {
            if idx >= scan.items.len() {
                return Err(format!(
                    "item {idx} is out of range (scan has {})",
                    scan.items.len()
                ));
            }
            if !seen.insert(idx) {
                return Err(format!("item {idx} is selected more than once"));
            }
        }
        if ev.kind == EventKind::Everyday {
            let months: HashSet<(i32, u32)> = ev
                .item_idxs
                .iter()
                .map(|&i| {
                    let it = &scan.items[i];
                    let secs = req.camera_offsets.get(&it.camera).copied().unwrap_or(0);
                    let t = it.capture_time + Duration::seconds(secs);
                    (t.year(), t.month())
                })
                .collect();
            if months.len() > 1 {
                return Err(
                    "an Everyday event cannot span more than one calendar month; \
                            split it by month or make it a named event"
                        .into(),
                );
            }
        }
    }
    Ok(())
}

pub struct JobCtx {
    pub db: DbPool,
    pub events: EventHub,
    pub enrich: EnrichWorker,
    pub library_root: PathBuf,
    /// `<data root>\cache`, holding `preview`, `thumb` and a scratch `tmp`.
    pub cache_root: PathBuf,
}

#[derive(Default)]
struct Summary {
    copied: usize,
    skipped: usize,
    shoot_ids: Vec<i64>,
    /// Shoots whose existing photos gained a camera JPEG. Not new shoots, so
    /// they stay out of `ImportDone`, but enrichment still revisits them.
    attached_shoot_ids: Vec<i64>,
}

/// Runs the job to completion and reports the outcome as an event. Shoots that
/// were committed before a failure are still handed to enrichment: their
/// photos are real and resumable.
pub fn run(ctx: &JobCtx, job_id: &str, scan: &CardScan, req: &ImportRequest) {
    let mut summary = Summary::default();
    let result = run_events(ctx, job_id, scan, req, &mut summary);
    for &shoot_id in summary.shoot_ids.iter().chain(&summary.attached_shoot_ids) {
        ctx.enrich.enqueue(shoot_id);
    }
    ctx.events.send(match result {
        Ok(()) => ServerEvent::ImportDone {
            job_id: job_id.to_string(),
            shoot_ids: summary.shoot_ids,
            copied: summary.copied,
            skipped_duplicates: summary.skipped,
        },
        Err(e) => ServerEvent::ImportFailed {
            job_id: job_id.to_string(),
            error: format!("{e:#}"),
        },
    });
}

/// Shared by the worker threads of one job.
struct Shared<'a> {
    ctx: &'a JobCtx,
    job_id: &'a str,
    total: usize,
    offsets: &'a HashMap<String, i64>,
    claims: NameClaims,
    /// Hashes accepted earlier in this job, so identical files on one card
    /// are copied once even though the DB does not know them yet.
    accepted: Mutex<HashSet<String>>,
    /// Held while counting AND sending so progress frames are monotonic.
    done: Mutex<usize>,
}

impl Shared<'_> {
    fn tick(&self, current: &str) {
        let mut done = self.done.lock().unwrap_or_else(|e| e.into_inner());
        *done += 1;
        self.send_progress(*done, current);
    }

    fn send_progress(&self, done: usize, current: &str) {
        self.ctx.events.send(ServerEvent::ImportProgress {
            job_id: self.job_id.to_string(),
            done,
            total: self.total,
            current: current.to_string(),
        });
    }

    /// True when `sha256` is new to both the library and this job; claims it.
    fn is_fresh(&self, sha256: &str) -> anyhow::Result<bool> {
        if !known_hashes(&self.ctx.db, &[sha256.to_string()])?.is_empty() {
            return Ok(false);
        }
        Ok(self
            .accepted
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sha256.to_string()))
    }
}

fn run_events(
    ctx: &JobCtx,
    job_id: &str,
    scan: &CardScan,
    req: &ImportRequest,
    summary: &mut Summary,
) -> anyhow::Result<()> {
    for dir in ["preview", "thumb", "tmp"] {
        fs::create_dir_all(ctx.cache_root.join(dir))
            .with_context(|| format!("creating cache/{dir}"))?;
    }
    // Scratch previews orphaned by an earlier crashed run; one job at a time
    // means nothing live can be in here.
    if let Ok(entries) = fs::read_dir(ctx.cache_root.join("tmp")) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(WORKERS)
        .build()
        .context("starting import workers")?;
    let shared = Shared {
        ctx,
        job_id,
        total: req.events.iter().map(|e| e.item_idxs.len()).sum(),
        offsets: &req.camera_offsets,
        claims: NameClaims::default(),
        accepted: Mutex::new(HashSet::new()),
        done: Mutex::new(0),
    };
    shared.send_progress(0, "");

    for ev in &req.events {
        let items: Vec<&ScanItem> = ev.item_idxs.iter().map(|&i| &scan.items[i]).collect();
        let times: Vec<_> = items
            .iter()
            .map(|it| it.capture_time + offset_for(&shared, it))
            .collect();
        let first = *times.iter().min().expect("validated non-empty");
        let last = *times.iter().max().expect("validated non-empty");
        let folder = folder_path(&ctx.library_root, ev.kind, first.date(), &ev.name);
        // A crash leaves .partial files that no later run would ever name again.
        sweep_partials(&folder);

        let results: Vec<anyhow::Result<Outcome>> = pool.install(|| {
            items
                .par_iter()
                .map(|it| process_item(&shared, it, &folder))
                .collect()
        });

        let mut processed = Vec::new();
        let mut attachments = Vec::new();
        let mut first_error = None;
        for (item, r) in items.iter().zip(results) {
            match r {
                Ok(Outcome::New(p)) => processed.push(*p),
                Ok(Outcome::Attached(a)) => attachments.push(a),
                Ok(Outcome::Skipped) => summary.skipped += 1,
                Err(e) => {
                    first_error.get_or_insert(e.context(format!("{}", item.primary.display())));
                }
            }
        }
        // Commit what succeeded even when something failed: those files are
        // verified on disk, and the rerun then skips them as duplicates.
        if !processed.is_empty() {
            let shoot = NewShoot {
                kind: match ev.kind {
                    EventKind::Named => ShootKind::Named,
                    EventKind::Everyday => ShootKind::Everyday,
                },
                name: (ev.kind == EventKind::Named).then(|| ev.name.clone()),
                folder: folder.to_string_lossy().into_owned(),
                date_start: Some(first.date()),
                date_end: Some(last.date()),
            };
            commit_event(ctx, &shoot, &items, &req.camera_offsets, processed, summary)?;
        }
        if !attachments.is_empty() {
            attach_jpegs(&ctx.db, &attachments).context("recording attached JPEGs")?;
            summary.copied += attachments.len();
            let photo_ids: Vec<i64> = attachments.iter().map(|a| a.photo_id).collect();
            match shoot_ids_of_photos(&ctx.db, &photo_ids) {
                Ok(ids) => summary.attached_shoot_ids.extend(ids),
                Err(e) => eprintln!("photosift: finding shoots of attached JPEGs: {e:#}"),
            }
        }
        if let Some(e) = first_error {
            return Err(e);
        }
    }
    Ok(())
}

fn offset_for(shared: &Shared, item: &ScanItem) -> Duration {
    Duration::seconds(shared.offsets.get(&item.camera).copied().unwrap_or(0))
}

/// What became of one scanned item.
enum Outcome {
    /// Content already in the library; nothing copied.
    Skipped,
    New(Box<Processed>),
    /// Only a camera JPEG was new: its RAW was imported earlier.
    Attached(Attachment),
}

/// An item that was copied and is waiting for its rows.
struct Processed {
    bundle: PhotoBundle,
    /// Rendered into the cache's scratch dir because the photo id the final
    /// name needs does not exist until the rows are inserted.
    previews: Option<ScratchPreviews>,
}

struct ScratchPreviews {
    preview: PathBuf,
    thumb: PathBuf,
}

fn commit_event(
    ctx: &JobCtx,
    shoot: &NewShoot,
    items: &[&ScanItem],
    offsets: &HashMap<String, i64>,
    processed: Vec<Processed>,
    summary: &mut Summary,
) -> anyhow::Result<()> {
    let mut cameras: Vec<&str> = items.iter().map(|it| it.camera.as_str()).collect();
    cameras.sort_unstable();
    cameras.dedup();
    let stored_offsets: Vec<(String, i64)> = cameras
        .into_iter()
        .filter_map(|c| offsets.get(c).map(|&s| (c.to_string(), s)))
        .collect();

    let (bundles, previews): (Vec<_>, Vec<_>) = processed
        .into_iter()
        .map(|p| (p.bundle, p.previews))
        .unzip();
    let committed = commit_import_event(&ctx.db, shoot, &bundles, &stored_offsets)
        .context("recording imported photos")?;

    summary.copied += bundles.len();
    if !summary.shoot_ids.contains(&committed.shoot_id) {
        summary.shoot_ids.push(committed.shoot_id);
    }
    for (id, scratch) in committed.photo_ids.iter().zip(previews) {
        let Some(scratch) = scratch else { continue };
        fs::rename(
            &scratch.preview,
            ctx.cache_root.join("preview").join(format!("{id}.jpg")),
        )
        .and_then(|()| {
            fs::rename(
                &scratch.thumb,
                ctx.cache_root.join("thumb").join(format!("{id}.jpg")),
            )
        })
        .with_context(|| format!("saving previews for photo {id}"))?;
    }
    Ok(())
}

fn process_item(shared: &Shared, item: &ScanItem, folder: &Path) -> anyhow::Result<Outcome> {
    let name = file_name_string(&item.primary);
    let outcome = match item.kind {
        ItemKind::Photo => process_photo(shared, item, &name, folder)?,
        ItemKind::Video => process_video(shared, item, &name, folder)?,
    };
    shared.tick(&name);
    Ok(outcome)
}

fn size_of(path: &Path) -> i64 {
    fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0)
}

fn file_name_string(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn process_photo(
    shared: &Shared,
    item: &ScanItem,
    name: &str,
    folder: &Path,
) -> anyhow::Result<Outcome> {
    // The one read of the source: hashed, copied, decoded and EXIF-read from memory.
    let bytes = fs::read(&item.primary).with_context(|| format!("reading {name}"))?;
    let sha256 = sha256_hex(&bytes);
    if !shared.is_fresh(&sha256)? {
        // The RAW is already here, but its camera JPEG may be new (RAWs
        // imported first, RAW+JPEG card later) and must not be lost.
        return process_orphan_sibling(shared, item, folder, &sha256);
    }
    copy_photo(shared, item, name, folder, &bytes, &sha256)
}

/// The primary was a duplicate; handles its sibling JPEG on its own.
fn process_orphan_sibling(
    shared: &Shared,
    item: &ScanItem,
    folder: &Path,
    primary_sha: &str,
) -> anyhow::Result<Outcome> {
    let Some(path) = &item.sibling_jpeg else {
        return Ok(Outcome::Skipped);
    };
    let name = file_name_string(path);
    let jpeg = fs::read(path).with_context(|| format!("reading {name}"))?;
    let hash = sha256_hex(&jpeg);
    if !shared.is_fresh(&hash)? {
        return Ok(Outcome::Skipped);
    }

    // Prefer attaching to the photo that owns the RAW, beside the RAW and
    // under its stem; fall back to a JPEG-only photo in this event's folder.
    let owner = photo_for_file_hash(&shared.ctx.db, primary_sha)?
        .filter(|o| o.photo_jpeg_path.is_none())
        .and_then(|o| {
            let raw = PathBuf::from(&o.path);
            let dir = raw.parent().filter(|d| d.is_dir())?.to_path_buf();
            let stem = raw.file_stem()?.to_string_lossy().into_owned();
            Some((o.photo_id, dir, stem))
        });
    if let Some((photo_id, dir, raw_stem)) = owner {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let aligned = format!("{raw_stem}.{ext}");
        let n = shared
            .claims
            .claim_sibling(&dir, Member::of(&aligned, &hash));
        let copied = write_verified(&jpeg, &name, &dir, Some(&suffixed_stem(&aligned, n)))
            .with_context(|| format!("copying {name}"))?;
        return Ok(Outcome::Attached(Attachment {
            photo_id,
            file: NewFile {
                sha256: copied.sha256,
                path: copied.dest.to_string_lossy().into_owned(),
                name,
                size: jpeg.len() as i64,
            },
        }));
    }

    let jpeg_item = ScanItem {
        primary: path.clone(),
        sibling_jpeg: None,
        ..item.clone()
    };
    copy_photo(shared, &jpeg_item, &name, folder, &jpeg, &hash)
}

/// Copies a photo whose primary hash `sha256` was just accepted as new.
fn copy_photo(
    shared: &Shared,
    item: &ScanItem,
    name: &str,
    folder: &Path,
    bytes: &[u8],
    sha256: &str,
) -> anyhow::Result<Outcome> {
    // A sibling whose content is already in the library is not copied again;
    // files.sha256 is unique, so registering it twice would fail the event.
    let sibling = match &item.sibling_jpeg {
        Some(path) => {
            let jpeg = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            let hash = sha256_hex(&jpeg);
            let fresh = shared.is_fresh(&hash)?;
            fresh.then(|| (file_name_string(path), jpeg, hash))
        }
        None => None,
    };

    fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    let mut members = vec![Member::of(name, sha256)];
    if let Some((file_name, _, hash)) = &sibling {
        members.push(Member::of(file_name, hash));
    }
    let n = shared.claims.claim(folder, &members);
    // Always pass the claimed stem, even for n == 0: left to itself the writer
    // would re-pick a name for that one file without the pair's context and
    // could split the pair (e.g. a stray .xmp pushing only the JPEG to _1).
    let stem_for = |file_name: &str| suffixed_stem(file_name, n);

    let main = write_verified(bytes, name, folder, Some(&stem_for(name)))
        .with_context(|| format!("copying {name}"))?;
    let mut files = vec![NewFile {
        sha256: main.sha256.clone(),
        path: main.dest.to_string_lossy().into_owned(),
        name: name.to_string(),
        size: bytes.len() as i64,
    }];
    let mut jpeg_path = None;
    if let Some((file_name, jpeg, _)) = &sibling {
        let copied = write_verified(jpeg, file_name, folder, Some(&stem_for(file_name)))
            .with_context(|| format!("copying {file_name}"))?;
        files.push(NewFile {
            sha256: copied.sha256,
            path: copied.dest.to_string_lossy().into_owned(),
            name: file_name.clone(),
            size: jpeg.len() as i64,
        });
        jpeg_path = Some(copied.dest.to_string_lossy().into_owned());
    }

    let ext = item
        .primary
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_jpeg_item = matches!(ext.to_ascii_lowercase().as_str(), "jpg" | "jpeg");
    let main_path = main.dest.to_string_lossy().into_owned();
    let (raw_path, jpeg_path) = if is_jpeg_item {
        (None, Some(main_path))
    } else {
        (Some(main_path), jpeg_path)
    };

    let rendered = render_previews(shared, item, bytes, &ext);
    let (previews, width, height, phash_value, orientation) = match rendered {
        Ok(r) => (
            Some(r.previews),
            Some(r.width),
            Some(r.height),
            Some(r.phash),
            r.orientation,
        ),
        Err(e) => {
            // The file is safely copied; losing one preview must not abort a
            // 500-photo import. The row simply has no dimensions yet.
            eprintln!("photosift: no preview for {name}: {e:#}");
            (None, None, None, None, None)
        }
    };

    let offset = offset_for(shared, item);
    Ok(Outcome::New(Box::new(Processed {
        bundle: PhotoBundle {
            photo: NewPhoto {
                shoot_id: 0,
                raw_path,
                jpeg_path,
                video_path: None,
                sha256: main.sha256,
                camera: Some(item.camera.clone()),
                capture_time_orig: Some(item.capture_time),
                capture_time: Some(item.capture_time + offset),
                time_source: Some(item.time_source),
                orientation,
                width,
                height,
                phash: phash_value,
            },
            files,
        },
        previews,
    })))
}

struct RenderedItem {
    previews: ScratchPreviews,
    width: i64,
    height: i64,
    phash: u64,
    orientation: Option<i64>,
}

fn render_previews(
    shared: &Shared,
    item: &ScanItem,
    bytes: &[u8],
    ext: &str,
) -> anyhow::Result<RenderedItem> {
    let jpeg = embedded::largest_jpeg(bytes, ext)?;
    let info = exif::read(bytes, &item.primary);
    let tmp = shared.ctx.cache_root.join("tmp");
    let stamp = format!("{}-{}", shared.job_id, item.idx);
    let previews = ScratchPreviews {
        preview: tmp.join(format!("{stamp}-preview.jpg")),
        thumb: tmp.join(format!("{stamp}-thumb.jpg")),
    };
    let rendered =
        render::make_previews(jpeg, info.orientation, &previews.preview, &previews.thumb)
            .inspect_err(|_| {
                let _ = fs::remove_file(&previews.preview);
                let _ = fs::remove_file(&previews.thumb);
            })?;
    Ok(RenderedItem {
        phash: phash::compute(&rendered.thumb_rgb, rendered.thumb_w, rendered.thumb_h) as u64,
        width: i64::from(rendered.display_w),
        height: i64::from(rendered.display_h),
        orientation: Some(i64::from(info.orientation)),
        previews,
    })
}

fn process_video(
    shared: &Shared,
    item: &ScanItem,
    name: &str,
    folder: &Path,
) -> anyhow::Result<Outcome> {
    // Streamed twice (hash, then copy) rather than held in memory: a clip can
    // be gigabytes and 16 workers would otherwise hold 16 of them.
    let sha256 = hash_file(&item.primary).with_context(|| format!("hashing {name}"))?;
    if !shared.is_fresh(&sha256)? {
        return Ok(Outcome::Skipped);
    }
    fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    let n = shared.claims.claim(folder, &[Member::of(name, &sha256)]);
    let stem = suffixed_stem(name, n);
    let copied = copy_verified_expecting(&item.primary, folder, Some(&stem), Some(&sha256))
        .with_context(|| format!("copying {name}"))?;
    let dest = copied.dest.to_string_lossy().into_owned();
    Ok(Outcome::New(Box::new(Processed {
        bundle: PhotoBundle {
            photo: NewPhoto {
                shoot_id: 0,
                raw_path: None,
                jpeg_path: None,
                video_path: Some(dest.clone()),
                sha256: copied.sha256.clone(),
                camera: Some(item.camera.clone()),
                capture_time_orig: Some(item.capture_time),
                capture_time: Some(item.capture_time + offset_for(shared, item)),
                time_source: Some(item.time_source),
                orientation: None,
                width: None,
                height: None,
                phash: None,
            },
            files: vec![NewFile {
                sha256: copied.sha256,
                path: dest,
                name: name.to_string(),
                size: size_of(&item.primary),
            }],
        },
        previews: None,
    })))
}
