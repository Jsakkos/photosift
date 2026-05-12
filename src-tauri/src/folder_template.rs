//! Configurable shoot-folder layout: the path template and bucket
//! folder names.
//!
//! Two independent knobs, both stored on the (single-row) `settings`
//! table as one JSON column and applied globally:
//!
//! * **`path_template`** — where a shoot folder is created at import
//!   time. Tokens: `{root}` (library root), `{year}`, `{year-month}`,
//!   `{slug}`. Default `"{root}/DSLR/{year}/{year-month}_{slug}"`.
//!   Only consulted on import — existing shoots keep whatever
//!   `dest_path` they were created with.
//!
//! * **`buckets`** — the subfolder names `sync_shoot_layout` moves
//!   files into. Defaults `RAW` / `rejects` / `selects` / `edit` /
//!   `export`. `rejects`/`selects`/`edit` nest under `raw`; `export`
//!   is a top-level sibling of `raw` (see issue #7). Applied on every
//!   layout sync, so renaming a bucket relocates files on the next
//!   sync (the old, now-empty directory is left behind).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Buckets {
    /// Import bucket — every RAW lands here on import.
    pub raw: String,
    /// `flag = reject`. Nested under `raw`.
    pub rejects: String,
    /// `flag = pick`, `destination = unrouted`. Nested under `raw`.
    pub selects: String,
    /// `flag = pick`, `destination = edit`. Nested under `raw`.
    pub edit: String,
    /// `flag = pick`, `destination = export`. Top-level sibling of `raw`.
    pub export: String,
}

impl Default for Buckets {
    fn default() -> Self {
        // Casing matches what shipped before this setting existed:
        // `RAW/`, `RAW/rejects/`, `RAW/selects/`, `RAW/edit/`, and the
        // top-level `Export/` from issue #7.
        Self {
            raw: "RAW".to_string(),
            rejects: "rejects".to_string(),
            selects: "selects".to_string(),
            edit: "edit".to_string(),
            export: "Export".to_string(),
        }
    }
}

impl Buckets {
    /// Subfolder (relative to the shoot root) a photo should live in,
    /// given its cull metadata. `rejects`/`selects`/`edit` nest under
    /// `raw`; `export` is top-level. Unreviewed (or any unexpected
    /// state) stays in the import bucket.
    pub fn subdir_for(&self, flag: &str, destination: &str) -> String {
        match (flag, destination) {
            ("reject", _) => format!("{}/{}", self.raw, self.rejects),
            ("pick", "edit") => format!("{}/{}", self.raw, self.edit),
            ("pick", "export") => self.export.clone(),
            ("pick", _) => format!("{}/{}", self.raw, self.selects),
            _ => self.raw.clone(),
        }
    }

    fn all(&self) -> [(&str, &str); 5] {
        [
            ("import (RAW)", &self.raw),
            ("rejects", &self.rejects),
            ("selects", &self.selects),
            ("edit", &self.edit),
            ("export", &self.export),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTemplate {
    pub path_template: String,
    pub buckets: Buckets,
}

impl Default for FolderTemplate {
    fn default() -> Self {
        Self {
            path_template: "{root}/DSLR/{year}/{year-month}_{slug}".to_string(),
            buckets: Buckets::default(),
        }
    }
}

/// Characters that are illegal in a path segment on at least one of
/// Windows / macOS / Linux. `/` is allowed in the template (it's the
/// separator between segments) but not inside a bucket name.
const ILLEGAL_PATH_CHARS: &[char] = &['<', '>', ':', '"', '\\', '|', '?', '*'];

impl FolderTemplate {
    /// Resolve the shoot-folder path for an import. `year_month` is the
    /// `"YYYY-MM"` string; `year` is derived from its first four chars.
    /// Unknown tokens are left verbatim (validation should have caught
    /// them, but we don't want to silently drop path components). The
    /// template's `/` separators are accepted on every platform — `Path`
    /// treats both `/` and `\` as separators on Windows.
    pub fn resolve_shoot_dir(&self, root: &std::path::Path, year_month: &str, slug: &str) -> PathBuf {
        let year = year_month.get(..4).unwrap_or(year_month);
        let resolved = self
            .path_template
            .replace("{root}", &root.to_string_lossy())
            .replace("{year-month}", year_month)
            .replace("{year}", year)
            .replace("{slug}", slug);
        PathBuf::from(resolved)
    }

    /// Returns a list of human-readable problems, empty if valid.
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();

        if !self.path_template.contains("{slug}") {
            errs.push(
                "Path template must include {slug} so shoots don't collide on disk.".to_string(),
            );
        }
        if self.path_template.trim().is_empty() {
            errs.push("Path template can't be empty.".to_string());
        }

        // Reject obviously bad characters in the non-token parts of the
        // template. Strip the known tokens first so a `:` inside e.g. a
        // Windows `{root}` (`C:\...`) — which arrives via `{root}` at
        // resolve time, not literally in the template — doesn't trip us.
        let stripped = self
            .path_template
            .replace("{root}", "")
            .replace("{year-month}", "")
            .replace("{year}", "")
            .replace("{slug}", "");
        if stripped.contains(|c| ILLEGAL_PATH_CHARS.contains(&c)) {
            errs.push(format!(
                "Path template contains an illegal character. Avoid: {}",
                ILLEGAL_PATH_CHARS.iter().collect::<String>()
            ));
        }
        // Any braces left after stripping the known tokens = a typo'd or
        // unsupported token.
        if stripped.contains('{') || stripped.contains('}') {
            errs.push(
                "Path template has an unrecognized {token}. Supported: {root} {year} {year-month} {slug}."
                    .to_string(),
            );
        }

        // Bucket names: non-empty, no path separators or illegal chars,
        // and pairwise distinct (a duplicate would route two cull states
        // to the same folder).
        let mut seen: Vec<&str> = Vec::new();
        for (label, name) in self.buckets.all() {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                errs.push(format!("The {label} bucket name can't be empty."));
                continue;
            }
            if name.contains('/')
                || name.contains('\\')
                || name.contains(|c| ILLEGAL_PATH_CHARS.contains(&c))
            {
                errs.push(format!(
                    "The {label} bucket name has an illegal character (no / \\ {} ).",
                    ILLEGAL_PATH_CHARS.iter().collect::<String>()
                ));
            }
            if seen.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                errs.push(format!(
                    "The {label} bucket name \"{name}\" duplicates another bucket — each must be unique."
                ));
            }
            seen.push(name);
        }

        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn default_resolves_to_legacy_dslr_layout() {
        let t = FolderTemplate::default();
        let dir = t.resolve_shoot_dir(Path::new("/lib"), "2026-05", "greece-trip");
        assert_eq!(dir, PathBuf::from("/lib/DSLR/2026/2026-05_greece-trip"));
    }

    #[test]
    fn resolve_handles_custom_template() {
        let t = FolderTemplate {
            path_template: "{root}/{year}/{slug}".to_string(),
            buckets: Buckets::default(),
        };
        let dir = t.resolve_shoot_dir(Path::new("/photos"), "2024-11", "wedding");
        assert_eq!(dir, PathBuf::from("/photos/2024/wedding"));
    }

    #[test]
    fn default_bucket_subdirs() {
        let b = Buckets::default();
        assert_eq!(b.subdir_for("unreviewed", "unrouted"), "RAW");
        assert_eq!(b.subdir_for("reject", "unrouted"), "RAW/rejects");
        assert_eq!(b.subdir_for("pick", "unrouted"), "RAW/selects");
        assert_eq!(b.subdir_for("pick", "edit"), "RAW/edit");
        assert_eq!(b.subdir_for("pick", "export"), "Export");
    }

    #[test]
    fn custom_bucket_subdirs() {
        let b = Buckets {
            raw: "Originals".to_string(),
            rejects: "trash".to_string(),
            selects: "keepers".to_string(),
            edit: "to-edit".to_string(),
            export: "Publish".to_string(),
        };
        assert_eq!(b.subdir_for("reject", "x"), "Originals/trash");
        assert_eq!(b.subdir_for("pick", "edit"), "Originals/to-edit");
        assert_eq!(b.subdir_for("pick", "export"), "Publish");
        assert_eq!(b.subdir_for("pick", "unrouted"), "Originals/keepers");
        assert_eq!(b.subdir_for("unreviewed", "x"), "Originals");
    }

    #[test]
    fn default_template_validates_clean() {
        assert!(FolderTemplate::default().validate().is_empty());
    }

    #[test]
    fn validate_flags_missing_slug() {
        let t = FolderTemplate {
            path_template: "{root}/{year}".to_string(),
            ..Default::default()
        };
        assert!(t.validate().iter().any(|e| e.contains("{slug}")));
    }

    #[test]
    fn validate_flags_unknown_token() {
        let t = FolderTemplate {
            path_template: "{root}/{slug}/{camera}".to_string(),
            ..Default::default()
        };
        assert!(t.validate().iter().any(|e| e.contains("unrecognized")));
    }

    #[test]
    fn validate_flags_empty_and_duplicate_buckets() {
        let t = FolderTemplate {
            path_template: "{root}/{slug}".to_string(),
            buckets: Buckets {
                raw: "RAW".to_string(),
                rejects: "".to_string(),
                selects: "RAW".to_string(),
                edit: "edit".to_string(),
                export: "export".to_string(),
            },
        };
        let errs = t.validate();
        assert!(errs.iter().any(|e| e.contains("can't be empty")));
        assert!(errs.iter().any(|e| e.contains("duplicates")));
    }

    #[test]
    fn validate_flags_illegal_bucket_char() {
        let t = FolderTemplate {
            path_template: "{root}/{slug}".to_string(),
            buckets: Buckets {
                rejects: "bad/name".to_string(),
                ..Buckets::default()
            },
        };
        assert!(t.validate().iter().any(|e| e.contains("illegal character")));
    }

    #[test]
    fn json_round_trips() {
        let t = FolderTemplate::default();
        let json = serde_json::to_string(&t).unwrap();
        let back: FolderTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
        // camelCase on the wire.
        assert!(json.contains("\"pathTemplate\""));
        assert!(json.contains("\"buckets\""));
    }
}
