//! Debug-only commands for the AI quality evaluation tool.
//!
//! Compiled and registered ONLY under `#[cfg(debug_assertions)]` — release
//! builds cannot call these and the directory layout is untouched.
//!
//! Data shape: one JSON file per benchmark "set" at
//! `~/.photosift-dev/benchmarks/<slug>.json`. A set is a photographer-
//! picked collection of photos (possibly spanning shoots) plus
//! per-photo and per-face judgments captured while reviewing each frame.
//!
//! Atomic writes (`tmp → rename`) make crash mid-save safe.

#![cfg(debug_assertions)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

// ---- on-disk shapes -----------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSetMeta {
    pub name: String,
    pub slug: String,
    pub created_at: String,
    #[serde(default)]
    pub notes: String,
    /// Bumped if the JSON shape ever changes. Loaders that don't recognize
    /// the version error rather than misinterpret older records.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SharpnessSnapshot {
    pub global_score: Option<f64>,
    pub max_eye_sharpness: Option<f64>,
    pub mean_eye_sharpness: Option<f64>,
    /// The 1–10 bucket from `AiSharpnessBadge` at the time of judgment;
    /// captured so the summary can correlate it against the verdict
    /// without re-fetching percentiles after the fact.
    pub ai_sharpness_badge_1to10: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkFaceJudgment {
    /// Position in the `get_faces_for_photo` response. FaceRow has no
    /// primary key, so position is the only stable identifier across
    /// runs *within the same analysis*. If the photo is re-analyzed,
    /// existing judgments are still pinned by the snapshot bbox below.
    pub face_index: i32,
    /// Snapshot bbox at judgment time (normalized 0–1). Lets a stale
    /// judgment surface "the face here moved" diagnostics if the
    /// detector is re-run.
    #[serde(default)]
    pub bbox_snapshot: Option<[f64; 4]>,
    #[serde(default)]
    pub detection_correct: Option<bool>,
    #[serde(default)]
    pub left_eye_correct: Option<bool>,
    #[serde(default)]
    pub right_eye_correct: Option<bool>,
    #[serde(default)]
    pub smile_correct: Option<bool>,
    #[serde(default)]
    pub species_correct: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkPhotoRecord {
    pub photo_id: i64,
    pub shoot_id: i64,
    pub camera_model: Option<String>,
    #[serde(default)]
    pub judged_at: Option<String>,
    #[serde(default)]
    pub faces: Vec<BenchmarkFaceJudgment>,
    /// Faces the user noticed that YuNet missed entirely.
    #[serde(default)]
    pub missed_face_count: i32,
    /// `subject_sharp` | `subject_blurry` | `all_sharp` | `all_blurry`
    /// | `intended_bokeh`. None until the user tags this photo.
    #[serde(default)]
    pub subject_sharpness_verdict: Option<String>,
    #[serde(default)]
    pub sharpness_signals_snapshot: Option<SharpnessSnapshot>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSet {
    pub set: BenchmarkSetMeta,
    pub photos: Vec<BenchmarkPhotoRecord>,
}

/// Summary entry for the set list view — just enough to render a card
/// without loading every photo record.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSetListing {
    pub slug: String,
    pub name: String,
    pub created_at: String,
    pub photo_count: usize,
    /// How many of `photo_count` have any non-empty judgment field. Lets
    /// the UI badge "12/30 judged" without parsing every record.
    pub judged_count: usize,
}

// ---- filesystem helpers -------------------------------------------------

/// `<photosift_home>/benchmarks/`. Created lazily on first save.
pub fn benchmarks_dir() -> PathBuf {
    crate::db::schema::photosift_home().join("benchmarks")
}

/// Lowercase, ASCII-alphanumeric + hyphen only. Multiple separators
/// collapse to a single hyphen, leading/trailing hyphens trimmed,
/// empty → `untitled`.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_hyphen = true; // suppress leading hyphens
    for ch in name.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else {
            None
        };
        match mapped {
            Some(c) => {
                out.push(c);
                prev_hyphen = false;
            }
            None => {
                if !prev_hyphen {
                    out.push('-');
                    prev_hyphen = true;
                }
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}

/// Write `value` to `path` atomically: serialize to `<path>.tmp`, fsync,
/// then rename over `path`. A crash mid-write leaves either the old
/// file or the `.tmp` sibling — never a half-written `<path>`.
fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        let bytes = serde_json::to_vec_pretty(value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn set_path(slug: &str) -> PathBuf {
    benchmarks_dir().join(format!("{}.json", slug))
}

fn count_judged(set: &BenchmarkSet) -> usize {
    set.photos
        .iter()
        .filter(|p| {
            p.judged_at.is_some()
                || p.subject_sharpness_verdict.is_some()
                || p.missed_face_count > 0
                || p.faces.iter().any(|f| {
                    f.detection_correct.is_some()
                        || f.left_eye_correct.is_some()
                        || f.right_eye_correct.is_some()
                        || f.smile_correct.is_some()
                        || f.species_correct.is_some()
                })
        })
        .count()
}

// ---- tauri commands -----------------------------------------------------

#[tauri::command]
pub fn benchmark_list_sets() -> Result<Vec<BenchmarkSetListing>, String> {
    let dir = benchmarks_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut listings = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<BenchmarkSet>(&text) {
                Ok(set) => {
                    listings.push(BenchmarkSetListing {
                        slug: set.set.slug.clone(),
                        name: set.set.name.clone(),
                        created_at: set.set.created_at.clone(),
                        photo_count: set.photos.len(),
                        judged_count: count_judged(&set),
                    });
                }
                Err(e) => {
                    log::warn!(
                        "benchmark_list_sets: skipping {} (parse error: {})",
                        path.display(),
                        e
                    );
                }
            },
            Err(e) => log::warn!("benchmark_list_sets: cannot read {}: {}", path.display(), e),
        }
    }
    // Newest first by created_at string (ISO 8601 sorts lexically).
    listings.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(listings)
}

#[tauri::command]
pub fn benchmark_load_set(slug: String) -> Result<BenchmarkSet, String> {
    let path = set_path(&slug);
    let text = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let set: BenchmarkSet = serde_json::from_str(&text)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    if set.set.schema_version != 1 {
        return Err(format!(
            "benchmark set {} uses schema_version {} (this build understands 1)",
            slug, set.set.schema_version
        ));
    }
    Ok(set)
}

#[tauri::command]
pub fn benchmark_save_set(set: BenchmarkSet) -> Result<BenchmarkSetListing, String> {
    if set.set.slug.is_empty() {
        return Err("set.slug is empty".into());
    }
    let path = set_path(&set.set.slug);
    atomic_write_json(&path, &set).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(BenchmarkSetListing {
        slug: set.set.slug.clone(),
        name: set.set.name.clone(),
        created_at: set.set.created_at.clone(),
        photo_count: set.photos.len(),
        judged_count: count_judged(&set),
    })
}

#[tauri::command]
pub fn benchmark_delete_set(slug: String) -> Result<(), String> {
    let path = set_path(&slug);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
    }
    // Best-effort cleanup of a sibling markdown export, if present.
    let md = benchmarks_dir().join(format!("{}.md", slug));
    if md.exists() {
        let _ = fs::remove_file(md);
    }
    Ok(())
}

/// Write a sibling `<slug>.md` next to the JSON. The frontend renders
/// the readable report; this command just persists the text it built
/// (so all markdown formatting lives in one place: the TS summary view).
#[tauri::command]
pub fn benchmark_export_markdown(slug: String, markdown: String) -> Result<String, String> {
    let dir = benchmarks_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let path = dir.join(format!("{}.md", slug));
    fs::write(&path, markdown).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

// ---- tests --------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(slug: &str) -> BenchmarkSetMeta {
        BenchmarkSetMeta {
            name: slug.into(),
            slug: slug.into(),
            created_at: "2026-05-13T18:22:01Z".into(),
            notes: String::new(),
            schema_version: 1,
        }
    }

    fn empty_set(slug: &str) -> BenchmarkSet {
        BenchmarkSet {
            set: meta(slug),
            photos: Vec::new(),
        }
    }

    #[test]
    fn slugify_strips_punctuation() {
        assert_eq!(slugify("D750 — smoke test #1"), "d750-smoke-test-1");
        assert_eq!(slugify("  hello  world  "), "hello-world");
        assert_eq!(slugify("___"), "untitled");
        assert_eq!(slugify(""), "untitled");
        assert_eq!(slugify("Already-OK"), "already-ok");
        assert_eq!(slugify("trailing!!!"), "trailing");
    }

    #[test]
    fn atomic_write_round_trip() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("set.json");
        let original = BenchmarkSet {
            set: meta("round-trip"),
            photos: vec![BenchmarkPhotoRecord {
                photo_id: 7,
                shoot_id: 3,
                camera_model: Some("NIKON D750".into()),
                judged_at: Some("2026-05-13T18:22:01Z".into()),
                faces: vec![BenchmarkFaceJudgment {
                    face_index: 0,
                    bbox_snapshot: Some([0.1, 0.2, 0.3, 0.4]),
                    detection_correct: Some(true),
                    left_eye_correct: Some(true),
                    right_eye_correct: None,
                    smile_correct: Some(false),
                    species_correct: Some(true),
                }],
                missed_face_count: 1,
                subject_sharpness_verdict: Some("intended_bokeh".into()),
                sharpness_signals_snapshot: Some(SharpnessSnapshot {
                    global_score: Some(47.3),
                    max_eye_sharpness: Some(81.4),
                    mean_eye_sharpness: Some(76.2),
                    ai_sharpness_badge_1to10: Some(5),
                }),
                notes: "f/1.8 portrait".into(),
            }],
        };
        atomic_write_json(&path, &original).unwrap();
        // No leftover `.tmp` from a happy path.
        assert!(!path.with_extension("json.tmp").exists());
        let parsed: BenchmarkSet =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn count_judged_treats_empty_as_unjudged() {
        let mut set = empty_set("c");
        set.photos.push(BenchmarkPhotoRecord {
            photo_id: 1,
            shoot_id: 1,
            camera_model: None,
            judged_at: None,
            faces: vec![],
            missed_face_count: 0,
            subject_sharpness_verdict: None,
            sharpness_signals_snapshot: None,
            notes: String::new(),
        });
        set.photos.push(BenchmarkPhotoRecord {
            photo_id: 2,
            shoot_id: 1,
            camera_model: None,
            judged_at: Some("now".into()),
            faces: vec![],
            missed_face_count: 0,
            subject_sharpness_verdict: None,
            sharpness_signals_snapshot: None,
            notes: String::new(),
        });
        set.photos.push(BenchmarkPhotoRecord {
            photo_id: 3,
            shoot_id: 1,
            camera_model: None,
            judged_at: None,
            faces: vec![BenchmarkFaceJudgment {
                face_index: 0,
                bbox_snapshot: None,
                detection_correct: Some(true),
                left_eye_correct: None,
                right_eye_correct: None,
                smile_correct: None,
                species_correct: None,
            }],
            missed_face_count: 0,
            subject_sharpness_verdict: None,
            sharpness_signals_snapshot: None,
            notes: String::new(),
        });
        assert_eq!(count_judged(&set), 2);
    }

    #[test]
    fn schema_version_default_applied_when_missing() {
        // Older or hand-edited JSON might omit schemaVersion entirely.
        // `serde(default)` should slot in `1` rather than failing.
        let json = r#"{
            "set": {
              "name": "x",
              "slug": "x",
              "createdAt": "2026-05-13T18:22:01Z",
              "notes": ""
            },
            "photos": []
        }"#;
        let parsed: BenchmarkSet = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.set.schema_version, 1);
    }
}
