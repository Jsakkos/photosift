//! The helper's prompts: `prompt.md` filled with the taste profile and recent
//! decisions (system), plus a one-line task naming the shoot and run (user).

use std::fmt::Write as _;
use std::sync::LazyLock;

use sha2::{Digest, Sha256};

use super::learning::{self, Example};
use crate::db::{helper, DbPool};

pub const TEMPLATE: &str = include_str!("prompt.md");

/// Overrides and accepts shown as examples.
const EXAMPLE_OVERRIDES: usize = 40;
const EXAMPLE_ACCEPTS: usize = 10;

/// The first 12 hex characters of the SHA-256 of `prompt.md` (line endings
/// normalised, so a CRLF checkout hashes the same), stored on every run.
pub static PROMPT_VERSION: LazyLock<String> = LazyLock::new(|| {
    let digest = Sha256::digest(TEMPLATE.replace("\r\n", "\n").as_bytes());
    digest.iter().take(6).fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
});

/// One example line: `proposed <o> ("<reason>") → chose <o>[; why: <criterion>[ — <note>]]`.
pub fn example_line(e: &Example) -> String {
    let mut line = format!(
        "proposed {} (\"{}\") → chose {}",
        e.proposed.as_str(),
        e.proposed_reason,
        e.chosen.as_str()
    );
    if let Some(c) = &e.criterion {
        let _ = write!(line, "; why: {c}");
        if let Some(n) = &e.note {
            let _ = write!(line, " — {n}");
        }
    }
    line
}

/// `(system, user)`. `new_photo_ids`: None reviews the whole shoot; Some
/// means an earlier run already proposed for the rest of it.
pub fn build(
    pool: &DbPool,
    shoot_id: i64,
    run_id: i64,
    new_photo_ids: Option<&[i64]>,
    frames_per_burst: u32,
) -> anyhow::Result<(String, String)> {
    let taste = helper::get_taste(pool)?;
    let taste = if taste.trim().is_empty() {
        "No notes yet.".to_string()
    } else {
        taste
    };
    let examples = learning::recent_examples(pool, EXAMPLE_OVERRIDES, EXAMPLE_ACCEPTS)?;
    let examples = if examples.is_empty() {
        "No decisions yet.".to_string()
    } else {
        examples
            .iter()
            .map(example_line)
            .collect::<Vec<_>>()
            .join("\n")
    };
    let system = TEMPLATE
        .replace("{{TASTE}}", taste.trim())
        .replace("{{EXAMPLES}}", &examples);

    // One line: it travels as a command-line argument.
    let mut user = format!(
        "Review shoot {shoot_id} in run_id {run_id}: pass run_id {run_id} to propose, \
         propose_your_call and finish_run, and call get_moment with frames_per_burst \
         {frames_per_burst}."
    );
    if let Some(new_photo_ids) = new_photo_ids {
        let ids: Vec<String> = new_photo_ids.iter().map(i64::to_string).collect();
        let _ = write!(
            user,
            " Only these photos are new since the last run: {}. Propose for each of them; \
             the shoot's other photos already have proposals, which stand unless a new photo \
             changes which frame of a burst is best.",
            ids.join(", ")
        );
    }
    Ok((system, user))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Outcome;

    #[test]
    fn example_lines() {
        let mut e = Example {
            proposed: Outcome::Archive,
            proposed_reason: "soft focus".into(),
            chosen: Outcome::Album,
            criterion: None,
            note: None,
        };
        assert_eq!(
            example_line(&e),
            "proposed archive (\"soft focus\") → chose album"
        );
        e.criterion = Some("expression".into());
        assert_eq!(
            example_line(&e),
            "proposed archive (\"soft focus\") → chose album; why: expression"
        );
        e.note = Some("her smile".into());
        assert_eq!(
            example_line(&e),
            "proposed archive (\"soft focus\") → chose album; why: expression — her smile"
        );
    }

    #[test]
    fn prompt_version_is_12_hex() {
        assert_eq!(PROMPT_VERSION.len(), 12);
        assert!(PROMPT_VERSION.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn template_has_both_placeholders() {
        assert!(TEMPLATE.contains("{{TASTE}}"));
        assert!(TEMPLATE.contains("{{EXAMPLES}}"));
    }
}
