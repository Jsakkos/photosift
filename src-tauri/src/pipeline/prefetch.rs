use crate::pipeline::cache::ImageCache;
use crate::pipeline::decoder::{decode_to_jpeg, DecodeTier};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PrefetchManager {
    cache: ImageCache,
    window_size: usize,
    image_paths: Arc<Mutex<Vec<(i64, PathBuf)>>>,
}

impl PrefetchManager {
    pub fn new(cache: ImageCache, window_size: usize) -> Self {
        Self {
            cache,
            window_size,
            image_paths: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_images(&self, images: Vec<(i64, PathBuf)>) {
        if let Ok(mut paths) = self.image_paths.lock() {
            *paths = images;
        }
    }

    pub fn prefetch_around(&self, current_index: usize, direction: i32) {
        let cache = self.cache.clone();
        let paths = self.image_paths.clone();
        let window = self.window_size;

        thread::spawn(move || {
            let paths = match paths.lock() {
                Ok(p) => p.clone(),
                Err(_) => return,
            };

            if paths.is_empty() || current_index >= paths.len() {
                return;
            }

            let (primary, secondary): (Vec<i64>, Vec<i64>) = if direction >= 0 {
                (
                    (1..=window as i64).collect(),
                    (1..=window as i64).map(|i| -i).collect(),
                )
            } else {
                (
                    (1..=window as i64).map(|i| -i).collect(),
                    (1..=window as i64).collect(),
                )
            };

            for offset in primary.iter().chain(secondary.iter()) {
                let idx = current_index as i64 + offset;
                if idx < 0 || (idx as usize) >= paths.len() {
                    continue;
                }
                let (image_id, ref filepath) = paths[idx as usize];
                if cache.contains(image_id) {
                    continue;
                }

                match decode_to_jpeg(filepath, DecodeTier::Preview, 90) {
                    Ok(jpeg_bytes) => cache.put(image_id, jpeg_bytes),
                    Err(e) => log::warn!("Prefetch failed for {:?}: {}", filepath, e),
                }
            }
        });
    }
}
