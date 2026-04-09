use lru::LruCache;
use std::num::NonZeroUsize;

pub struct ImageCache {
    pub previews: LruCache<i64, Vec<u8>>,
}

impl ImageCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            previews: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }
}
