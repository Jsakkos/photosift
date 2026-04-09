use crate::db::schema::Database;
use crate::pipeline::cache::ImageCache;

pub struct AppState {
    pub db: Option<Database>,
    pub cache: ImageCache,
    pub project_folder: Option<String>,
    pub current_index: usize,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: None,
            cache: ImageCache::new(20),
            project_folder: None,
            current_index: 0,
        }
    }
}
