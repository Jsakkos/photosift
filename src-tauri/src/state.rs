use crate::db::schema::Database;
use crate::pipeline::cache::ImageCache;
use crate::pipeline::prefetch::PrefetchManager;
use std::path::PathBuf;

pub struct AppState {
    pub db: Option<Database>,
    pub cache: ImageCache,
    pub prefetch: PrefetchManager,
    pub project_folder: Option<PathBuf>,
    pub preview_dir: Option<PathBuf>,
    pub current_index: usize,
    pub image_ids: Vec<i64>,
}

impl AppState {
    pub fn new() -> Self {
        let cache = ImageCache::new(100);
        let prefetch = PrefetchManager::new(cache.clone(), 5);
        Self {
            db: None,
            cache,
            prefetch,
            project_folder: None,
            preview_dir: None,
            current_index: 0,
            image_ids: Vec::new(),
        }
    }

    pub fn image_count(&self) -> usize {
        self.image_ids.len()
    }
}
