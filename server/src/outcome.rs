use std::str::FromStr;

use serde::{Deserialize, Serialize};

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
