//! Library folder naming. Pure, so the layout rules are testable without a disk.

use std::path::{Path, PathBuf};

use chrono::{Datelike, NaiveDate};

use super::events::EventKind;

/// ASCII words joined by `-`: accents are folded, apostrophes vanish (so
/// "Baby's" stays one word), and every other run of punctuation or whitespace
/// becomes a single separator. Empty results become "Untitled".
pub fn slugify(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in name.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '`') {
            continue;
        }
        let before = current.len();
        fold_char(c, &mut current);
        // A character with no ASCII form (punctuation, CJK, emoji) adds
        // nothing and so acts as a separator.
        if current.len() == before && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.is_empty() {
        "Untitled".to_string()
    } else {
        words.join("-")
    }
}

/// Appends the ASCII alphanumeric form of `c` to `out`, or nothing when `c`
/// is punctuation, whitespace, or has no sensible transliteration. A small
/// hand-written Latin-1/Latin Extended-A map; a full Unicode table is not
/// worth a dependency for folder names typed by one person.
fn fold_char(c: char, out: &mut String) {
    if c.is_ascii_alphanumeric() {
        out.push(c);
        return;
    }
    let s = match c {
        'À'..='Å' | 'Ā' | 'Ă' | 'Ą' => "A",
        'à'..='å' | 'ā' | 'ă' | 'ą' => "a",
        'Æ' => "AE",
        'æ' => "ae",
        'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => "C",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'Ð' | 'Ď' | 'Đ' => "D",
        'ð' | 'ď' | 'đ' => "d",
        'È'..='Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => "E",
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => "G",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'Ì'..='Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => "I",
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'Ł' | 'Ĺ' | 'Ļ' | 'Ľ' => "L",
        'ł' | 'ĺ' | 'ļ' | 'ľ' => "l",
        'Ñ' | 'Ń' | 'Ņ' | 'Ň' => "N",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'Ò'..='Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => "O",
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'Œ' => "OE",
        'œ' => "oe",
        'Ŕ' | 'Ř' => "R",
        'ŕ' | 'ř' => "r",
        'Ś' | 'Ş' | 'Š' => "S",
        'ś' | 'ş' | 'š' => "s",
        'ß' => "ss",
        'Ţ' | 'Ť' => "T",
        'ţ' | 'ť' => "t",
        'Ù'..='Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => "U",
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'Ý' | 'Ÿ' => "Y",
        'ý' | 'ÿ' => "y",
        'Ź' | 'Ż' | 'Ž' => "Z",
        'ź' | 'ż' | 'ž' => "z",
        'Þ' => "Th",
        'þ' => "th",
        _ => "",
    };
    out.push_str(s);
}

/// `{root}\{YYYY}\{YYYY-MM-DD}_{Slug}` for a named event, or
/// `{root}\{YYYY}\{YYYY-MM}_Everyday` for everyday photos (the name is ignored:
/// everyday folders are one per month, not per moment).
pub fn folder_path(root: &Path, kind: EventKind, date: NaiveDate, name: &str) -> PathBuf {
    let year = format!("{:04}", date.year());
    let leaf = match kind {
        EventKind::Named => format!("{}_{}", date.format("%Y-%m-%d"), slugify(name)),
        EventKind::Everyday => format!("{}_Everyday", date.format("%Y-%m")),
    };
    root.join(year).join(leaf)
}
