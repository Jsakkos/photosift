use crate::pipeline::cache::ImageCache;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PrefetchManager {
    cache: ImageCache,
    window_size: usize,
    image_paths: Arc<Mutex<Vec<(i64, PathBuf)>>>,
    preview_dir: Arc<Mutex<Option<PathBuf>>>,
}

impl PrefetchManager {
    pub fn new(cache: ImageCache, window_size: usize) -> Self {
        Self {
            cache,
            window_size,
            image_paths: Arc::new(Mutex::new(Vec::new())),
            preview_dir: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_images(&self, images: Vec<(i64, PathBuf)>) {
        if let Ok(mut paths) = self.image_paths.lock() {
            *paths = images;
        }
    }

    pub fn set_preview_dir(&self, dir: PathBuf) {
        if let Ok(mut pd) = self.preview_dir.lock() {
            *pd = Some(dir);
        }
    }

    pub fn prefetch_around(&self, current_index: usize, direction: i32) {
        let cache = self.cache.clone();
        let paths = self.image_paths.clone();
        let preview_dir = self.preview_dir.clone();
        let window = self.window_size;

        thread::spawn(move || {
            let paths = match paths.lock() {
                Ok(p) => p.clone(),
                Err(_) => return,
            };
            let pdir = preview_dir.lock().ok().and_then(|p| p.clone());

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
                let (image_id, _) = &paths[idx as usize];
                if cache.contains(*image_id) {
                    continue;
                }

                // Read from preview file cache (fast: ~1ms for 876KB)
                if let Some(ref pdir) = pdir {
                    let preview_path = pdir.join(format!("{}.jpg", image_id));
                    if let Ok(data) = std::fs::read(&preview_path) {
                        cache.put(*image_id, data);
                        continue;
                    }
                }
                // Skip if preview not ready yet — background thread will create it
            }
        });
    }
}
