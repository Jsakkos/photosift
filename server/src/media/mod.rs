//! Pixels and metadata from camera files: embedded-JPEG extraction, EXIF,
//! oriented preview/thumbnail rendering, and the perceptual hash.

pub mod crops;
pub mod embedded;
pub mod exif;
pub mod phash;
pub mod render;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a recognised {0} container")]
    InvalidContainer(&'static str),
    #[error("no embedded JPEG of usable size found")]
    NoJpeg,
    #[error("jpeg decode failed: {0}")]
    Decode(String),
    #[error("image resize failed: {0}")]
    Resize(String),
    #[error("jpeg encode failed: {0}")]
    Encode(String),
}
