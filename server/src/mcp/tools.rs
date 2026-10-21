//! The tool table. Adding a tool is one entry in `all()` plus its function.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use base64::Engine;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

use crate::{
    api::AppState,
    db::{
        photos::{self, FaceRow, PhotoRow},
        review,
    },
    media::{crops, render},
    outcome::{self, ApplyError, Outcome},
};

const PHOTOS_MAX: usize = 8;
const FACE_PHOTOS_MAX: usize = 12;
const FACES_MAX: usize = 24;
const FRAMES_DEFAULT: u32 = 3;
const FRAMES_MAX: u32 = 6;
const PHOTO_EDGE: u32 = 1024;
const FACE_EDGE: u32 = 256;

pub enum Content {
    Text(String),
    Image { jpeg: Vec<u8> },
}

pub struct ToolResult {
    pub content: Vec<Content>,
    pub is_error: bool,
}

impl ToolResult {
    fn ok(content: Vec<Content>) -> Self {
        Self {
            content,
            is_error: false,
        }
    }

    fn text(s: impl Into<String>) -> Self {
        Self::ok(vec![Content::Text(s.into())])
    }

    /// A tool-level failure the model can read and react to.
    pub fn error(s: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text(s.into())],
            is_error: true,
        }
    }

    pub fn to_json(&self) -> Value {
        let b64 = base64::engine::general_purpose::STANDARD;
        let content: Vec<Value> = self
            .content
            .iter()
            .map(|c| match c {
                Content::Text(t) => json!({"type": "text", "text": t}),
                Content::Image { jpeg } => {
                    json!({"type": "image", "data": b64.encode(jpeg), "mimeType": "image/jpeg"})
                }
            })
            .collect();
        json!({"content": content, "isError": self.is_error})
    }
}

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub call: fn(&AppState, Value) -> ToolResult,
}

pub fn all() -> Vec<Tool> {
    vec![
        Tool {
            name: "list_shoots",
            description: "List all shoots, newest first, with id, name, dates, outcome counts and whether enrichment has finished.",
            schema: json!({"type": "object", "properties": {}}),
            call: list_shoots,
        },
        Tool {
            name: "get_shoot_overview",
            description: "Summary of one shoot: dates, cameras, outcome counts and its moments (index, time range, burst count, photo count).",
            schema: json!({
                "type": "object",
                "properties": {"shoot_id": {"type": "integer"}},
                "required": ["shoot_id"],
            }),
            call: get_shoot_overview,
        },
        Tool {
            name: "get_moment",
            description: "One moment of a shoot, burst by burst: a metadata line for every frame plus thumbnails of the best frames of each burst (best first, each preceded by a 'photo <id>' label).",
            schema: json!({
                "type": "object",
                "properties": {
                    "shoot_id": {"type": "integer"},
                    "moment_idx": {"type": "integer"},
                    "frames_per_burst": {"type": "integer", "minimum": 1, "maximum": FRAMES_MAX,
                        "default": FRAMES_DEFAULT},
                },
                "required": ["shoot_id", "moment_idx"],
            }),
            call: get_moment,
        },
        Tool {
            name: "get_photos",
            description: "Look at up to 8 photos at 1024 px on the long edge, each preceded by a 'photo <id>' label.",
            schema: json!({
                "type": "object",
                "properties": {"photo_ids": {"type": "array", "items": {"type": "integer"},
                    "minItems": 1, "maxItems": PHOTOS_MAX}},
                "required": ["photo_ids"],
            }),
            call: get_photos,
        },
        Tool {
            name: "get_faces",
            description: "Square 256 px crops of the detected faces in up to 12 photos (at most 24 faces, largest first), each preceded by a label with the face's sharpness.",
            schema: json!({
                "type": "object",
                "properties": {"photo_ids": {"type": "array", "items": {"type": "integer"},
                    "minItems": 1, "maxItems": FACE_PHOTOS_MAX}},
                "required": ["photo_ids"],
            }),
            call: get_faces,
        },
        Tool {
            name: "get_taste_profile",
            description: "The learned description of what the user keeps and rejects; empty until one has been written.",
            schema: json!({"type": "object", "properties": {}}),
            call: get_taste_profile,
        },
        Tool {
            name: "set_outcome",
            description: "Set one photo's outcome (unreviewed, album, library, archive, delete) and optionally its blooper flag; writes the Lightroom sidecar like the app does. Nothing is deleted until the user approves it in the app.",
            schema: json!({
                "type": "object",
                "properties": {
                    "photo_id": {"type": "integer"},
                    "outcome": {"type": "string",
                        "enum": ["unreviewed", "album", "library", "archive", "delete"]},
                    "blooper": {"type": "boolean"},
                },
                "required": ["photo_id", "outcome"],
            }),
            call: set_outcome,
        },
    ]
}

fn parse_args<T: DeserializeOwned>(args: Value) -> Result<T, ToolResult> {
    serde_json::from_value(args).map_err(|e| ToolResult::error(format!("invalid arguments: {e}")))
}

fn internal(e: impl std::fmt::Display) -> ToolResult {
    ToolResult::error(format!("internal error: {e}"))
}

/// The photo, or an error naming the id when it is unknown or in the Recycle Bin.
fn live_photo(state: &AppState, id: i64) -> Result<PhotoRow, ToolResult> {
    match photos::get_photo(&state.db, id).map_err(internal)? {
        None => Err(ToolResult::error(format!("photo {id} does not exist"))),
        Some(p) if p.trashed_at.is_some() => Err(ToolResult::error(format!(
            "photo {id} is in the Recycle Bin"
        ))),
        Some(p) => Ok(p),
    }
}

fn cache_path(state: &AppState, kind: &str, id: i64) -> std::path::PathBuf {
    state
        .data_root
        .join("cache")
        .join(kind)
        .join(format!("{id}.jpg"))
}

fn opt(v: Option<f64>) -> String {
    v.map_or_else(|| "-".into(), |v| format!("{v:.2}"))
}

fn when(t: Option<&str>) -> String {
    t.map_or_else(|| "-".into(), |t| t.replace('T', " "))
}

fn dates(start: Option<&str>, end: Option<&str>) -> String {
    match (start, end) {
        (Some(s), Some(e)) if s != e => format!("{s}..{e}"),
        (Some(d), _) | (None, Some(d)) => d.to_string(),
        (None, None) => "-".into(),
    }
}

fn shoot_label(s: &review::ShootSummary) -> String {
    s.name.clone().unwrap_or_else(|| format!("({})", s.kind))
}

fn counts_line(c: &review::Counts) -> String {
    format!(
        "total {}, unreviewed {}, album {}, library {}, archive {}, delete {}",
        c.total, c.unreviewed, c.album, c.library, c.archive, c.delete
    )
}

fn list_shoots(state: &AppState, _args: Value) -> ToolResult {
    let shoots = match review::list_shoots(&state.db) {
        Ok(s) => s,
        Err(e) => return internal(e),
    };
    if shoots.is_empty() {
        return ToolResult::text("No shoots yet.");
    }
    let mut out = String::from("id | name | dates | counts | enriched\n");
    for s in shoots {
        let _ = writeln!(
            out,
            "{} | {} | {} | {} | {}",
            s.id,
            shoot_label(&s),
            dates(s.date_start.as_deref(), s.date_end.as_deref()),
            counts_line(&s.counts),
            if s.enriched { "yes" } else { "no" },
        );
    }
    ToolResult::text(out)
}

#[derive(Deserialize)]
struct ShootArgs {
    shoot_id: i64,
}

fn load_review(state: &AppState, shoot_id: i64) -> Result<review::ShootReview, ToolResult> {
    review::shoot_review(&state.db, shoot_id)
        .map_err(internal)?
        .ok_or_else(|| ToolResult::error(format!("shoot {shoot_id} does not exist")))
}

fn get_shoot_overview(state: &AppState, args: Value) -> ToolResult {
    let a: ShootArgs = match parse_args(args) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let r = match load_review(state, a.shoot_id) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let s = &r.shoot;
    let mut cameras: BTreeMap<&str, usize> = BTreeMap::new();
    for p in r.photos.values() {
        *cameras
            .entry(p.camera.as_deref().unwrap_or("unknown"))
            .or_default() += 1;
    }
    let mut out = format!(
        "Shoot {}: {} ({})\nDates: {}\nEnriched: {}\nCounts: {}\nCameras: {}\nMoments: {}\n",
        s.id,
        shoot_label(s),
        s.kind,
        dates(s.date_start.as_deref(), s.date_end.as_deref()),
        if s.enriched { "yes" } else { "no" },
        counts_line(&r.counts),
        cameras
            .iter()
            .map(|(c, n)| format!("{c} x{n}"))
            .collect::<Vec<_>>()
            .join(", "),
        r.moments.len(),
    );
    for m in &r.moments {
        let photos: usize = m.bursts.iter().map(|b| b.photo_ids.len()).sum();
        let _ = writeln!(
            out,
            "moment {}: {} to {}, {} bursts, {} photos",
            m.idx,
            when(m.start.as_deref()),
            when(m.end.as_deref()),
            m.bursts.len(),
            photos,
        );
    }
    ToolResult::text(out)
}

#[derive(Deserialize)]
struct MomentArgs {
    shoot_id: i64,
    moment_idx: i64,
    frames_per_burst: Option<u32>,
}

fn get_moment(state: &AppState, args: Value) -> ToolResult {
    let a: MomentArgs = match parse_args(args) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let frames = a.frames_per_burst.unwrap_or(FRAMES_DEFAULT);
    if !(1..=FRAMES_MAX).contains(&frames) {
        return ToolResult::error(format!("frames_per_burst must be 1-{FRAMES_MAX}"));
    }
    let r = match load_review(state, a.shoot_id) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let Some(moment) = r.moments.iter().find(|m| m.idx == a.moment_idx) else {
        return ToolResult::error(format!(
            "shoot {} has no moment {}",
            a.shoot_id, a.moment_idx
        ));
    };
    let mut content = vec![Content::Text(format!(
        "Moment {} of shoot {}: {} to {}",
        moment.idx,
        a.shoot_id,
        when(moment.start.as_deref()),
        when(moment.end.as_deref()),
    ))];
    for burst in &moment.bursts {
        let mut head = format!(
            "Burst {} ({} frames, best first)\n",
            burst.idx,
            burst.photo_ids.len()
        );
        for id in &burst.photo_ids {
            let (Some(rp), Ok(Some(p))) = (r.photos.get(id), photos::get_photo(&state.db, *id))
            else {
                continue;
            };
            let _ = writeln!(
                head,
                "id={id} time={} faces={} face_sharpness={} sharpness={} clip_hi={} clip_lo={} signal_score={} outcome={}",
                when(rp.capture_time.as_deref()),
                rp.face_count,
                opt(p.face_sharpness),
                opt(p.sharpness),
                opt(p.clip_hi),
                opt(p.clip_lo),
                opt(rp.signal_score),
                rp.outcome,
            );
        }
        content.push(Content::Text(head));
        for id in burst.photo_ids.iter().take(frames as usize) {
            content.push(Content::Text(format!("photo {id}")));
            match std::fs::read(cache_path(state, "thumb", *id)) {
                Ok(jpeg) => content.push(Content::Image { jpeg }),
                Err(_) => content.push(Content::Text(format!("(no thumbnail for photo {id})"))),
            }
        }
    }
    ToolResult::ok(content)
}

#[derive(Deserialize)]
struct PhotoIdsArgs {
    photo_ids: Vec<i64>,
}

fn photo_ids(args: Value, max: usize) -> Result<Vec<i64>, ToolResult> {
    let a: PhotoIdsArgs = parse_args(args)?;
    if a.photo_ids.is_empty() || a.photo_ids.len() > max {
        return Err(ToolResult::error(format!(
            "photo_ids must hold 1-{max} ids, got {}",
            a.photo_ids.len()
        )));
    }
    Ok(a.photo_ids)
}

fn get_photos(state: &AppState, args: Value) -> ToolResult {
    let ids = match photo_ids(args, PHOTOS_MAX) {
        Ok(i) => i,
        Err(e) => return e,
    };
    // Validate everything first so a bad id never costs the decodes.
    for id in &ids {
        if let Err(e) = live_photo(state, *id) {
            return e;
        }
    }
    let mut content = Vec::new();
    for id in ids {
        content.push(Content::Text(format!("photo {id}")));
        match crops::downsized(&cache_path(state, "preview", id), PHOTO_EDGE) {
            Ok(jpeg) => content.push(Content::Image { jpeg }),
            Err(e) => return ToolResult::error(format!("photo {id} has no usable preview: {e}")),
        }
    }
    ToolResult::ok(content)
}

fn get_faces(state: &AppState, args: Value) -> ToolResult {
    let ids = match photo_ids(args, FACE_PHOTOS_MAX) {
        Ok(i) => i,
        Err(e) => return e,
    };
    let mut faces: Vec<(f32, i64, usize, FaceRow)> = Vec::new();
    for id in &ids {
        if let Err(e) = live_photo(state, *id) {
            return e;
        }
        match photos::list_faces(&state.db, *id) {
            Ok(list) => faces.extend(
                list.into_iter()
                    .enumerate()
                    .map(|(i, f)| (f.bbox.2 * f.bbox.3, *id, i + 1, f)),
            ),
            Err(e) => return internal(e),
        }
    }
    if faces.is_empty() {
        return ToolResult::text("No faces were detected in those photos.");
    }
    faces.sort_by(|a, b| b.0.total_cmp(&a.0));
    faces.truncate(FACES_MAX);

    // One decode per photo, keeping only the small crops.
    let mut crops_by_key: BTreeMap<(i64, usize), Vec<u8>> = BTreeMap::new();
    let mut by_photo: BTreeMap<i64, Vec<&(f32, i64, usize, FaceRow)>> = BTreeMap::new();
    for f in &faces {
        by_photo.entry(f.1).or_default().push(f);
    }
    for (id, list) in by_photo {
        let decoded = std::fs::read(cache_path(state, "preview", id))
            .map_err(|e| e.to_string())
            .and_then(|b| render::decode_jpeg(&b).map_err(|e| e.to_string()));
        let preview = match decoded {
            Ok(p) => p,
            Err(e) => return ToolResult::error(format!("photo {id} has no usable preview: {e}")),
        };
        for (_, _, n, face) in list {
            match crops::face_crop(&preview, face, FACE_EDGE) {
                Ok(jpeg) => {
                    crops_by_key.insert((id, *n), jpeg);
                }
                Err(e) => return internal(e),
            }
        }
    }
    let mut content = Vec::new();
    for (_, id, n, face) in faces {
        let Some(jpeg) = crops_by_key.remove(&(id, n)) else {
            continue;
        };
        content.push(Content::Text(format!(
            "photo {id} face {n} (sharpness {})",
            face.sharpness
                .map_or_else(|| "n/a".into(), |s| format!("{s:.1}")),
        )));
        content.push(Content::Image { jpeg });
    }
    ToolResult::ok(content)
}

fn get_taste_profile(_state: &AppState, _args: Value) -> ToolResult {
    ToolResult::text("")
}

#[derive(Deserialize)]
struct SetOutcomeArgs {
    photo_id: i64,
    outcome: String,
    blooper: Option<bool>,
}

fn set_outcome(state: &AppState, args: Value) -> ToolResult {
    let a: SetOutcomeArgs = match parse_args(args) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let outcome: Outcome = match a.outcome.parse() {
        Ok(o) => o,
        Err(e) => return ToolResult::error(format!("{e}")),
    };
    match outcome::set_and_mirror(&state.db, a.photo_id, outcome, a.blooper) {
        Ok((photo, xmp_error)) => {
            let mut msg = format!(
                "photo {}: outcome {}, blooper {}",
                photo.id,
                photo.outcome.as_str(),
                photo.blooper
            );
            if let Some(e) = xmp_error {
                let _ = write!(msg, " (sidecar not written: {e})");
            }
            ToolResult::text(msg)
        }
        Err(ApplyError::NotFound(id)) => ToolResult::error(format!("photo {id} does not exist")),
        Err(e @ ApplyError::Trashed(_)) => ToolResult::error(e.to_string()),
        Err(ApplyError::Other(e)) => internal(format!("{e:#}")),
    }
}
