use std::collections::HashSet;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{
    params, params_from_iter,
    types::{FromSqlError, Type},
    Connection, OptionalExtension, Row,
};
use serde::Serialize;

use super::DbPool;
use crate::outcome::Outcome;

const DATETIME_FMT: &str = "%Y-%m-%dT%H:%M:%S";
const DATE_FMT: &str = "%Y-%m-%d";
// SQLite's default variable limit is 999 on older builds; stay well under it.
const HASH_CHUNK: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShootKind {
    Named,
    Everyday,
}

impl ShootKind {
    fn as_str(self) -> &'static str {
        match self {
            ShootKind::Named => "named",
            ShootKind::Everyday => "everyday",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeSource {
    Exif,
    Mtime,
}

impl TimeSource {
    fn as_str(self) -> &'static str {
        match self {
            TimeSource::Exif => "exif",
            TimeSource::Mtime => "mtime",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "exif" => Some(TimeSource::Exif),
            "mtime" => Some(TimeSource::Mtime),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewShoot {
    pub kind: ShootKind,
    pub name: Option<String>,
    pub folder: String,
    pub date_start: Option<NaiveDate>,
    pub date_end: Option<NaiveDate>,
}

/// Ingest-time fields only; enrichment columns (sharpness, moments, ...) are
/// written later by the enrichment worker. `width`/`height` are display
/// (post-orientation) dimensions.
#[derive(Debug, Clone, Default)]
pub struct NewPhoto {
    pub shoot_id: i64,
    pub raw_path: Option<String>,
    pub jpeg_path: Option<String>,
    pub video_path: Option<String>,
    pub sha256: String,
    pub camera: Option<String>,
    pub capture_time_orig: Option<NaiveDateTime>,
    pub capture_time: Option<NaiveDateTime>,
    pub time_source: Option<TimeSource>,
    pub orientation: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub phash: Option<u64>,
}

/// JSON numbers are f64 in JS; a 64-bit hash would lose its low bits.
fn phash_as_string<S: serde::Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(h) => s.serialize_some(&h.to_string()),
        None => s.serialize_none(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhotoRow {
    pub id: i64,
    pub shoot_id: i64,
    pub raw_path: Option<String>,
    pub jpeg_path: Option<String>,
    pub video_path: Option<String>,
    pub sha256: String,
    pub camera: Option<String>,
    pub capture_time_orig: Option<NaiveDateTime>,
    pub capture_time: Option<NaiveDateTime>,
    pub time_source: Option<TimeSource>,
    pub orientation: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    #[serde(serialize_with = "phash_as_string")]
    pub phash: Option<u64>,
    pub sharpness: Option<f64>,
    pub face_sharpness: Option<f64>,
    pub mean_luma: Option<f64>,
    pub clip_hi: Option<f64>,
    pub clip_lo: Option<f64>,
    pub moment_idx: Option<i64>,
    pub burst_idx: Option<i64>,
    pub signal_score: Option<f64>,
    pub outcome: Outcome,
    pub blooper: bool,
    pub enriched_at: Option<NaiveDateTime>,
    pub trashed_at: Option<NaiveDateTime>,
}

const PHOTO_COLUMNS: &str = "id, shoot_id, raw_path, jpeg_path, video_path, sha256, camera, \
    capture_time_orig, capture_time, time_source, orientation, width, height, phash, \
    sharpness, face_sharpness, mean_luma, clip_hi, clip_lo, moment_idx, burst_idx, \
    signal_score, outcome, blooper, enriched_at, trashed_at";

fn conversion_error(
    idx: usize,
    err: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err))
}

fn datetime_col(row: &Row, idx: usize) -> rusqlite::Result<Option<NaiveDateTime>> {
    row.get::<_, Option<String>>(idx)?
        .map(|s| {
            NaiveDateTime::parse_from_str(&s, DATETIME_FMT).map_err(|e| conversion_error(idx, e))
        })
        .transpose()
}

fn row_to_photo(row: &Row) -> rusqlite::Result<PhotoRow> {
    let time_source = row
        .get::<_, Option<String>>(9)?
        .map(|s| {
            TimeSource::parse(&s).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    Type::Text,
                    Box::new(FromSqlError::Other(
                        format!("unknown time_source {s:?}").into(),
                    )),
                )
            })
        })
        .transpose()?;
    let outcome_text: String = row.get(22)?;
    let outcome = outcome_text
        .parse::<Outcome>()
        .map_err(|e| conversion_error(22, e))?;
    Ok(PhotoRow {
        id: row.get(0)?,
        shoot_id: row.get(1)?,
        raw_path: row.get(2)?,
        jpeg_path: row.get(3)?,
        video_path: row.get(4)?,
        sha256: row.get(5)?,
        camera: row.get(6)?,
        capture_time_orig: datetime_col(row, 7)?,
        capture_time: datetime_col(row, 8)?,
        time_source,
        orientation: row.get(10)?,
        width: row.get(11)?,
        height: row.get(12)?,
        phash: row.get::<_, Option<i64>>(13)?.map(|v| v as u64),
        sharpness: row.get(14)?,
        face_sharpness: row.get(15)?,
        mean_luma: row.get(16)?,
        clip_hi: row.get(17)?,
        clip_lo: row.get(18)?,
        moment_idx: row.get(19)?,
        burst_idx: row.get(20)?,
        signal_score: row.get(21)?,
        outcome,
        blooper: row.get::<_, i64>(23)? != 0,
        enriched_at: datetime_col(row, 24)?,
        trashed_at: datetime_col(row, 25)?,
    })
}

fn fmt_datetime(t: Option<NaiveDateTime>) -> Option<String> {
    t.map(|t| t.format(DATETIME_FMT).to_string())
}

fn insert_shoot_on(c: &Connection, shoot: &NewShoot) -> rusqlite::Result<i64> {
    let created_at = chrono::Local::now()
        .naive_local()
        .format(DATETIME_FMT)
        .to_string();
    c.execute(
        "INSERT INTO shoots (kind, name, folder, date_start, date_end, created_at)          VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            shoot.kind.as_str(),
            shoot.name,
            shoot.folder,
            shoot.date_start.map(|d| d.format(DATE_FMT).to_string()),
            shoot.date_end.map(|d| d.format(DATE_FMT).to_string()),
            created_at,
        ],
    )?;
    Ok(c.last_insert_rowid())
}

fn insert_photo_on(c: &Connection, photo: &NewPhoto) -> rusqlite::Result<i64> {
    c.execute(
        "INSERT INTO photos (shoot_id, raw_path, jpeg_path, video_path, sha256, camera,          capture_time_orig, capture_time, time_source, orientation, width, height, phash)          VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            photo.shoot_id,
            photo.raw_path,
            photo.jpeg_path,
            photo.video_path,
            photo.sha256,
            photo.camera,
            fmt_datetime(photo.capture_time_orig),
            fmt_datetime(photo.capture_time),
            photo.time_source.map(TimeSource::as_str),
            photo.orientation,
            photo.width,
            photo.height,
            // Bit-for-bit u64 -> i64; SQLite has no unsigned type.
            photo.phash.map(|h| h as i64),
        ],
    )?;
    Ok(c.last_insert_rowid())
}

fn insert_file_on(
    c: &Connection,
    sha256: &str,
    path: &str,
    name: &str,
    size: i64,
    photo_id: Option<i64>,
) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO files (sha256, path, name, size, photo_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![sha256, path, name, size, photo_id],
    )?;
    Ok(())
}

pub fn insert_shoot(pool: &DbPool, shoot: &NewShoot) -> anyhow::Result<i64> {
    pool.with(|c| insert_shoot_on(c, shoot))
}

pub fn insert_photo(pool: &DbPool, photo: &NewPhoto) -> anyhow::Result<i64> {
    pool.with(|c| insert_photo_on(c, photo))
}

/// Registers a copied file for global dedupe. `photo_id` is None for files not
/// tied to a photo row.
pub fn insert_file(
    pool: &DbPool,
    sha256: &str,
    path: &str,
    name: &str,
    size: i64,
    photo_id: Option<i64>,
) -> anyhow::Result<()> {
    pool.with(|c| insert_file_on(c, sha256, path, name, size, photo_id))
}

/// A copied file to register in `files`. `name`/`size` describe the SOURCE
/// file (what a later card scan will see), `path` the library copy.
#[derive(Debug, Clone)]
pub struct NewFile {
    pub sha256: String,
    pub path: String,
    pub name: String,
    pub size: i64,
}

/// One imported frame: its photo row plus every file copied for it.
#[derive(Debug, Clone)]
pub struct PhotoBundle {
    pub photo: NewPhoto,
    pub files: Vec<NewFile>,
}

pub struct CommittedEvent {
    pub shoot_id: i64,
    /// Photo ids in the order of the `bundles` passed in.
    pub photo_ids: Vec<i64>,
}

/// Records one imported event atomically: finds or creates the shoot, inserts
/// every photo and file row, and stores the camera offsets. A shoot already
/// at `shoot.folder` (a second import into the same Everyday month, or a
/// resumed import) is reused and its date range widened. `bundle.photo.shoot_id`
/// is ignored and replaced.
pub fn commit_import_event(
    pool: &DbPool,
    shoot: &NewShoot,
    bundles: &[PhotoBundle],
    offsets: &[(String, i64)],
) -> anyhow::Result<CommittedEvent> {
    pool.with(|c| {
        // Dropped without commit on any error, which rolls the whole event back.
        let tx = c.unchecked_transaction()?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM shoots WHERE folder = ?1",
                [&shoot.folder],
                |r| r.get(0),
            )
            .optional()?;
        let shoot_id = match existing {
            Some(id) => {
                tx.execute(
                    "UPDATE shoots SET                        date_start = MIN(COALESCE(date_start, ?1), ?1),                        date_end = MAX(COALESCE(date_end, ?2), ?2)                      WHERE id = ?3 AND ?1 IS NOT NULL AND ?2 IS NOT NULL",
                    params![
                        shoot.date_start.map(|d| d.format(DATE_FMT).to_string()),
                        shoot.date_end.map(|d| d.format(DATE_FMT).to_string()),
                        id,
                    ],
                )?;
                id
            }
            None => insert_shoot_on(&tx, shoot)?,
        };
        let mut photo_ids = Vec::with_capacity(bundles.len());
        for bundle in bundles {
            let mut photo = bundle.photo.clone();
            photo.shoot_id = shoot_id;
            let photo_id = insert_photo_on(&tx, &photo)?;
            for f in &bundle.files {
                insert_file_on(&tx, &f.sha256, &f.path, &f.name, f.size, Some(photo_id))?;
            }
            photo_ids.push(photo_id);
        }
        for (camera, secs) in offsets {
            tx.execute(
                "INSERT INTO camera_offsets (shoot_id, camera, offset_seconds)                  VALUES (?1, ?2, ?3)                  ON CONFLICT (shoot_id, camera) DO UPDATE SET offset_seconds = ?3",
                params![shoot_id, camera, secs],
            )?;
        }
        tx.commit()?;
        Ok(CommittedEvent {
            shoot_id,
            photo_ids,
        })
    })
}

/// The photo a registered file belongs to.
#[derive(Debug, Clone)]
pub struct ExistingFile {
    pub photo_id: i64,
    /// Library path of the registered file itself.
    pub path: String,
    pub photo_jpeg_path: Option<String>,
}

/// Looks up the photo that owns the file with this content hash, if any.
pub fn photo_for_file_hash(pool: &DbPool, sha256: &str) -> anyhow::Result<Option<ExistingFile>> {
    pool.with(|c| {
        c.query_row(
            "SELECT f.photo_id, f.path, p.jpeg_path FROM files f              JOIN photos p ON p.id = f.photo_id WHERE f.sha256 = ?1",
            [sha256],
            |r| {
                Ok(ExistingFile {
                    photo_id: r.get(0)?,
                    path: r.get(1)?,
                    photo_jpeg_path: r.get(2)?,
                })
            },
        )
        .optional()
    })
}

/// A camera JPEG copied for a photo whose RAW was imported earlier.
#[derive(Debug, Clone)]
pub struct Attachment {
    pub photo_id: i64,
    pub file: NewFile,
}

/// Registers late-arriving sibling JPEGs and sets `jpeg_path` where the photo
/// has none yet (an existing JPEG is never replaced).
pub fn attach_jpegs(pool: &DbPool, attachments: &[Attachment]) -> anyhow::Result<()> {
    pool.with(|c| {
        let tx = c.unchecked_transaction()?;
        for a in attachments {
            tx.execute(
                "UPDATE photos SET jpeg_path = ?1 WHERE id = ?2 AND jpeg_path IS NULL",
                params![a.file.path, a.photo_id],
            )?;
            insert_file_on(
                &tx,
                &a.file.sha256,
                &a.file.path,
                &a.file.name,
                a.file.size,
                Some(a.photo_id),
            )?;
        }
        tx.commit()
    })
}

/// Which of `hashes` are already registered in `files`.
pub fn known_hashes(pool: &DbPool, hashes: &[String]) -> anyhow::Result<HashSet<String>> {
    pool.with(|c| {
        let mut known = HashSet::new();
        for chunk in hashes.chunks(HASH_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let mut stmt = c.prepare(&format!(
                "SELECT sha256 FROM files WHERE sha256 IN ({placeholders})"
            ))?;
            let rows = stmt.query_map(params_from_iter(chunk), |r| r.get::<_, String>(0))?;
            for row in rows {
                known.insert(row?);
            }
        }
        Ok(known)
    })
}

pub fn get_photo(pool: &DbPool, id: i64) -> anyhow::Result<Option<PhotoRow>> {
    pool.with(|c| {
        c.query_row(
            &format!("SELECT {PHOTO_COLUMNS} FROM photos WHERE id = ?1"),
            [id],
            row_to_photo,
        )
        .optional()
    })
}

/// Sets the outcome; `blooper: None` leaves the existing tag untouched so
/// outcome changes never silently clear it. Errors if the photo does not exist.
pub fn set_outcome(
    pool: &DbPool,
    photo_id: i64,
    outcome: Outcome,
    blooper: Option<bool>,
) -> anyhow::Result<PhotoRow> {
    pool.with(|c| {
        let changed = c.execute(
            "UPDATE photos SET outcome = ?1, blooper = COALESCE(?2, blooper) WHERE id = ?3",
            params![outcome.as_str(), blooper.map(i64::from), photo_id],
        )?;
        if changed == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }
        c.query_row(
            &format!("SELECT {PHOTO_COLUMNS} FROM photos WHERE id = ?1"),
            [photo_id],
            row_to_photo,
        )
    })
}

/// Distinct shoots owning the given photos.
pub fn shoot_ids_of_photos(pool: &DbPool, photo_ids: &[i64]) -> anyhow::Result<Vec<i64>> {
    pool.with(|c| {
        let mut out = HashSet::new();
        for chunk in photo_ids.chunks(HASH_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let mut stmt = c.prepare(&format!(
                "SELECT DISTINCT shoot_id FROM photos WHERE id IN ({placeholders})"
            ))?;
            for row in stmt.query_map(params_from_iter(chunk), |r| r.get::<_, i64>(0))? {
                out.insert(row?);
            }
        }
        let mut ids: Vec<i64> = out.into_iter().collect();
        ids.sort_unstable();
        Ok(ids)
    })
}

/// Shoots with at least one photo the enrichment worker has not seen yet.
pub fn shoots_needing_enrichment(pool: &DbPool) -> anyhow::Result<Vec<i64>> {
    pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT DISTINCT shoot_id FROM photos WHERE enriched_at IS NULL ORDER BY shoot_id",
        )?;
        let ids = stmt.query_map([], |r| r.get(0))?.collect();
        ids
    })
}

pub fn unenriched_photo_ids(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Vec<i64>> {
    pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id FROM photos WHERE shoot_id = ?1 AND enriched_at IS NULL ORDER BY id",
        )?;
        let ids = stmt.query_map([shoot_id], |r| r.get(0))?.collect();
        ids
    })
}

/// One detected face, boxes and landmarks normalized 0-1 in display orientation.
#[derive(Debug, Clone, PartialEq)]
pub struct NewFace {
    pub bbox: (f32, f32, f32, f32),
    pub landmarks: [(f32, f32); 5],
    pub score: f32,
    pub embedding: Option<Vec<f32>>,
    pub sharpness: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FaceRow {
    pub id: i64,
    pub photo_id: i64,
    pub bbox: (f32, f32, f32, f32),
    pub score: Option<f32>,
    pub landmarks: Option<[(f32, f32); 5]>,
    pub embedding: Option<Vec<f32>>,
    pub sharpness: Option<f64>,
}

/// Quality columns written by the enrichment worker; all `None` when the
/// photo could not be analysed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct QualityColumns {
    pub sharpness: Option<f64>,
    pub face_sharpness: Option<f64>,
    pub mean_luma: Option<f64>,
    pub clip_hi: Option<f64>,
    pub clip_lo: Option<f64>,
}

/// Replaces the photo's faces and quality columns and marks it enriched, in
/// one transaction, so a crash never leaves faces without the enriched flag
/// (or the reverse). Re-running for a photo is idempotent.
pub fn save_enrichment(
    pool: &DbPool,
    photo_id: i64,
    quality: &QualityColumns,
    faces: &[NewFace],
) -> anyhow::Result<()> {
    pool.with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("DELETE FROM faces WHERE photo_id = ?1", [photo_id])?;
        for f in faces {
            let landmarks = serde_json::to_string(&f.landmarks.map(|(x, y)| [x, y]))
                .expect("landmarks serialize");
            let blob: Option<Vec<u8>> = f
                .embedding
                .as_ref()
                .map(|e| e.iter().flat_map(|v| v.to_le_bytes()).collect());
            tx.execute(
                "INSERT INTO faces(photo_id, x, y, w, h, score, landmarks, embedding, sharpness) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    photo_id,
                    f.bbox.0,
                    f.bbox.1,
                    f.bbox.2,
                    f.bbox.3,
                    f.score,
                    landmarks,
                    blob,
                    f.sharpness
                ],
            )?;
        }
        tx.execute(
            "UPDATE photos SET sharpness = ?1, face_sharpness = ?2, mean_luma = ?3, \
             clip_hi = ?4, clip_lo = ?5, enriched_at = ?6 WHERE id = ?7",
            params![
                quality.sharpness,
                quality.face_sharpness,
                quality.mean_luma,
                quality.clip_hi,
                quality.clip_lo,
                fmt_datetime(Some(chrono::Local::now().naive_local())),
                photo_id
            ],
        )?;
        tx.commit()
    })
}

pub fn list_faces(pool: &DbPool, photo_id: i64) -> anyhow::Result<Vec<FaceRow>> {
    pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id, photo_id, x, y, w, h, score, landmarks, embedding, sharpness \
             FROM faces WHERE photo_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([photo_id], |r| {
            let landmarks: Option<String> = r.get(7)?;
            let landmarks = landmarks
                .and_then(|s| serde_json::from_str::<[[f32; 2]; 5]>(&s).ok())
                .map(|l| l.map(|[x, y]| (x, y)));
            let embedding: Option<Vec<u8>> = r.get(8)?;
            let embedding = embedding.map(|b| {
                b.chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect()
            });
            Ok(FaceRow {
                id: r.get(0)?,
                photo_id: r.get(1)?,
                bbox: (r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?),
                score: r.get(6)?,
                landmarks,
                embedding,
                sharpness: r.get(9)?,
            })
        })?;
        rows.collect()
    })
}
