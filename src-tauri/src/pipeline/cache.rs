use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

/// Thread-safe LRU cache for decoded preview images (JPEG bytes).
#[derive(Clone)]
pub struct ImageCache {
    inner: Arc<Mutex<LruCache<i64, Vec<u8>>>>,
}

impl ImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap(),
            ))),
        }
    }

    pub fn get(&self, image_id: i64) -> Option<Vec<u8>> {
        self.inner.lock().ok()?.get(&image_id).cloned()
    }

    pub fn put(&self, image_id: i64, jpeg_bytes: Vec<u8>) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.put(image_id, jpeg_bytes);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.inner.lock() {
            cache.clear();
        }
    }

    pub fn contains(&self, image_id: i64) -> bool {
        self.inner
            .lock()
            .ok()
            .map(|mut c| c.get(&image_id).is_some())
            .unwrap_or(false)
    }
}
