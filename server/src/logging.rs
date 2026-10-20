//! Timestamped log lines. The release runs unattended with stderr redirected
//! to `<data root>\logs\photosift.log`, so every line carries local time and a
//! module tag.

use chrono::{Local, NaiveDateTime};

/// `YYYY-MM-DD HH:MM:SS [tag] message`
pub fn format_line(now: NaiveDateTime, tag: &str, message: &str) -> String {
    format!("{} [{tag}] {message}", now.format("%Y-%m-%d %H:%M:%S"))
}

#[doc(hidden)]
pub fn emit(tag: &str, message: std::fmt::Arguments<'_>) {
    eprintln!(
        "{}",
        format_line(Local::now().naive_local(), tag, &message.to_string())
    );
}

/// `plog!("tag", "format {}", args)`: one timestamped line on stderr.
#[macro_export]
macro_rules! plog {
    ($tag:expr, $($arg:tt)+) => {
        $crate::logging::emit($tag, format_args!($($arg)+))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_local_timestamp_and_tag() {
        let t = "2026-10-19T09:05:03".parse().unwrap();
        assert_eq!(
            format_line(t, "enrich", "face detection failed"),
            "2026-10-19 09:05:03 [enrich] face detection failed"
        );
    }
}
