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
}
