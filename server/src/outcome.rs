use std::{path::PathBuf, str::FromStr, sync::Mutex};

use serde::{Deserialize, Serialize};

use crate::{
    db::{
        photos::{self, PhotoRow},
        DbPool,
    },
    xmp,
};

/// Serialises "write the outcome, then mirror it to the sidecar". Without it
/// two requests could update the database A then B but write the sidecar B then
/// A, leaving them disagreeing. Taken before `xmp`'s own lock, never inside it.
pub static OUTCOME_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error("no photo {0}")]
    NotFound(i64),
    #[error("photo {0} is in the Recycle Bin")]
    Trashed(i64),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// The file whose `<stem>.xmp` carries the outcome: the RAW, else the JPEG.
/// Videos get no sidecar.
pub fn sidecar_target(photo: &PhotoRow) -> Option<PathBuf> {
    photo
        .raw_path
        .as_deref()
        .or(photo.jpeg_path.as_deref())
        .map(PathBuf::from)
}

/// Persists the outcome and rewrites the sidecar as one critical section.
/// Returns the stored row and, if the sidecar could not be written, why (the
/// database change stands). A photo already in the Recycle Bin is refused so no
/// sidecar reappears beside a file that left.
pub fn set_and_mirror(
    db: &DbPool,
    id: i64,
    outcome: Outcome,
    blooper: Option<bool>,
) -> Result<(PhotoRow, Option<String>), ApplyError> {
    let _guard = OUTCOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    match photos::get_photo(db, id)? {
        None => return Err(ApplyError::NotFound(id)),
        Some(p) if p.trashed_at.is_some() => return Err(ApplyError::Trashed(id)),
        Some(_) => {}
    }
    let photo = photos::set_outcome(db, id, outcome, blooper)?;
    let xmp_error = sidecar_target(&photo).and_then(|image| {
        xmp::apply(&image, &xmp_fields(photo.outcome, photo.blooper))
            .err()
            .map(|e| e.to_string())
    });
    Ok((photo, xmp_error))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Unreviewed,
    Album,
    Library,
    Archive,
    Delete,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown outcome {0:?}")]
pub struct ParseOutcomeError(String);

impl Outcome {
    /// The string stored in `photos.outcome`; must match the schema CHECK list.
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Unreviewed => "unreviewed",
            Outcome::Album => "album",
            Outcome::Library => "library",
            Outcome::Archive => "archive",
            Outcome::Delete => "delete",
        }
    }
}

impl FromStr for Outcome {
    type Err = ParseOutcomeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unreviewed" => Ok(Outcome::Unreviewed),
            "album" => Ok(Outcome::Album),
            "library" => Ok(Outcome::Library),
            "archive" => Ok(Outcome::Archive),
            "delete" => Ok(Outcome::Delete),
            other => Err(ParseOutcomeError(other.to_string())),
        }
    }
}

/// Hierarchical keyword (`lr:hierarchicalSubject`) for the blooper tag; its
/// leaf (`Blooper`) is mirrored in `dc:subject`.
pub const BLOOPER_KEYWORD: &str = "PhotoSift|Blooper";

/// The label PhotoSift applies to Delete outcomes.
pub const DELETE_LABEL: &str = "Red";

/// What an outcome asks of an XMP sidecar. `None`/empty means "don't touch",
/// except `label: None`, which also releases a label PhotoSift set earlier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XmpFields {
    pub rating: Option<i32>,
    pub label: Option<&'static str>,
    pub add_keywords: Vec<&'static str>,
    pub remove_keywords: Vec<&'static str>,
}

/// The XMP mapping: album ★4, library ★3, archive ★2, delete ★1 + Red.
/// Unreviewed writes no rating or label (and never adds the blooper keyword);
/// the blooper tag is otherwise mirrored in both directions.
pub fn xmp_fields(o: Outcome, blooper: bool) -> XmpFields {
    let (rating, label) = match o {
        Outcome::Unreviewed => (None, None),
        Outcome::Album => (Some(4), None),
        Outcome::Library => (Some(3), None),
        Outcome::Archive => (Some(2), None),
        Outcome::Delete => (Some(1), Some(DELETE_LABEL)),
    };
    let add = blooper && o != Outcome::Unreviewed;
    XmpFields {
        rating,
        label,
        add_keywords: if add { vec![BLOOPER_KEYWORD] } else { vec![] },
        remove_keywords: if blooper {
            vec![]
        } else {
            vec![BLOOPER_KEYWORD]
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_round_trip() {
        for o in [
            Outcome::Unreviewed,
            Outcome::Album,
            Outcome::Library,
            Outcome::Archive,
            Outcome::Delete,
        ] {
            assert_eq!(o.as_str().parse::<Outcome>().unwrap(), o);
        }
        assert!("pick".parse::<Outcome>().is_err());
    }

    #[test]
    fn serde_is_lowercase() {
        assert_eq!(serde_json::to_string(&Outcome::Album).unwrap(), "\"album\"");
    }

    #[test]
    fn mapping_follows_the_constraints() {
        let f = xmp_fields(Outcome::Delete, true);
        assert_eq!((f.rating, f.label), (Some(1), Some("Red")));
        assert_eq!(f.add_keywords, ["PhotoSift|Blooper"]);
        assert!(f.remove_keywords.is_empty());
        let f = xmp_fields(Outcome::Album, false);
        assert_eq!((f.rating, f.label), (Some(4), None));
        assert_eq!(f.remove_keywords, ["PhotoSift|Blooper"]);
        let f = xmp_fields(Outcome::Unreviewed, true);
        assert_eq!(f, XmpFields::default());
    }
}
