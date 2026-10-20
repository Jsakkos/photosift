use std::collections::HashSet;

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{
    params, params_from_iter,
    types::{FromSqlError, Type},
    OptionalExtension, Row,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
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

pub fn insert_shoot(pool: &DbPool, shoot: &NewShoot) -> anyhow::Result<i64> {
    let created_at = chrono::Local::now()
        .naive_local()
        .format(DATETIME_FMT)
        .to_string();
    pool.with(|c| {
        c.execute(
            "INSERT INTO shoots (kind, name, folder, date_start, date_end, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
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
    })
}

pub fn insert_photo(pool: &DbPool, photo: &NewPhoto) -> anyhow::Result<i64> {
    pool.with(|c| {
        c.execute(
            "INSERT INTO photos (shoot_id, raw_path, jpeg_path, video_path, sha256, camera, \
             capture_time_orig, capture_time, time_source, orientation, width, height, phash) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
    })
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
    pool.with(|c| {
        c.execute(
            "INSERT INTO files (sha256, path, name, size, photo_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![sha256, path, name, size, photo_id],
        )?;
        Ok(())
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
