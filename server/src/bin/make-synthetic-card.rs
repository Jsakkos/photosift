//! Writes a fake memory card of synthetic JPEGs for UI and end-to-end tests.
//!
//! `make-synthetic-card <out dir> <json | @file>` where the JSON is a list of
//! `{name, time: "YYYY-MM-DDTHH:MM:SS", camera, w, h, orientation}`. `camera`
//! becomes the EXIF Make, so the server reports exactly that string as the
//! camera. `w`, `h` and `orientation` default to 640, 480 and 1.

use std::{fs, path::Path};

use anyhow::{bail, Context};
use chrono::NaiveDateTime;
use photosift::testsupport::{synthetic_jpeg_with_exif, ExifSpec, Pattern};
use serde::Deserialize;

#[derive(Deserialize)]
struct CardFile {
    name: String,
    time: NaiveDateTime,
    camera: String,
    #[serde(default = "default_w")]
    w: u32,
    #[serde(default = "default_h")]
    h: u32,
    #[serde(default = "default_orientation")]
    orientation: u16,
}

fn default_w() -> u32 {
    640
}
fn default_h() -> u32 {
    480
}
fn default_orientation() -> u16 {
    1
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(out), Some(spec), None) = (args.next(), args.next(), args.next()) else {
        bail!("usage: make-synthetic-card <out dir> <json | @file>");
    };
    let json = match spec.strip_prefix('@') {
        Some(file) => fs::read_to_string(file).with_context(|| format!("reading {file}"))?,
        None => spec,
    };
    let files: Vec<CardFile> = serde_json::from_str(&json).context("parsing the file list")?;
    let out = Path::new(&out);
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    for (i, f) in files.iter().enumerate() {
        let name = if Path::new(&f.name).extension().is_some() {
            f.name.clone()
        } else {
            format!("{}.jpg", f.name)
        };
        // Alternating patterns keep neighbouring frames visually distinct.
        let pattern = if i % 2 == 0 {
            Pattern::Blocks
        } else {
            Pattern::Stripes
        };
        let taken = f.time.format("%Y:%m:%d %H:%M:%S").to_string();
        let jpeg = synthetic_jpeg_with_exif(
            f.w,
            f.h,
            pattern,
            &ExifSpec {
                orientation: Some(f.orientation),
                make: Some(&f.camera),
                taken: Some(&taken),
                ..Default::default()
            },
        );
        let path = out.join(&name);
        fs::write(&path, jpeg).with_context(|| format!("writing {}", path.display()))?;
    }
    println!("wrote {} files to {}", files.len(), out.display());
    Ok(())
}
