use crate::ai::{worker::WorkerHandle, AiProviderStatus};
use crate::db::schema::{self, Database};
use crate::metadata::xmp_queue::XmpWriteQueue;
use crate::pipeline::cache::ImageCache;
use crate::pipeline::prefetch::PrefetchManager;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;

pub struct AppState {
    pub db: Option<Database>,
    pub cache: ImageCache,
    pub prefetch: PrefetchManager,
    pub xmp_queue: XmpWriteQueue,
    pub current_shoot_id: Option<i64>,
    pub preview_dir: Option<PathBuf>,
    pub thumb_dir: Option<PathBuf>,
    pub image_ids: Vec<i64>,
    pub session_id: String,
    pub import_cancel: Arc<AtomicBool>,
    pub ai_worker: Option<WorkerHandle>,
    pub ai_status: AiProviderStatus,
    pub ai_cancel: Arc<AtomicBool>,
    pub ai_analyzed: Arc<AtomicUsize>,
    pub ai_failed: Arc<AtomicUsize>,
    pub ai_total: Arc<AtomicUsize>,
}

impl AppState {
    pub fn new() -> Self {
        let cache = ImageCache::new(100);
        let prefetch = PrefetchManager::new(cache.clone(), 5);
        let xmp_queue = XmpWriteQueue::new();
        xmp_queue.spawn_flusher();

        let db = match Database::open_global() {
            Ok(db) => {
                log::info!("Opened global DB at {:?}", schema::global_db_path());
                Some(db)
            }
            Err(e) => {
                log::error!("Failed to open global DB: {}", e);
                None
            }
        };

        Self {
            db,
            cache,
            prefetch,
            xmp_queue,
            current_shoot_id: None,
            preview_dir: None,
            thumb_dir: None,
            image_ids: Vec::new(),
            session_id: uuid::Uuid::new_v4().to_string(),
            import_cancel: Arc::new(AtomicBool::new(false)),
            ai_worker: None,
            ai_status: AiProviderStatus::Disabled,
            ai_cancel: Arc::new(AtomicBool::new(false)),
            ai_analyzed: Arc::new(AtomicUsize::new(0)),
            ai_failed: Arc::new(AtomicUsize::new(0)),
            ai_total: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Switch to a different shoot. Updates cache dirs and image_ids.
    pub fn load_shoot(&mut self, shoot_id: i64) -> Result<(), String> {
        let db = self.db.as_ref().ok_or("Database not open")?;
        let _shoot = db
            .get_shoot(shoot_id)
            .map_err(|e| e.to_string())?
            .ok_or("Shoot not found")?;

        let photos = db
            .photos_for_shoot(shoot_id)
            .map_err(|e| e.to_string())?;

        let cache_dir = schema::shoot_cache_dir(shoot_id);
        let preview_dir = cache_dir.join("previews");
        let thumb_dir = cache_dir.join("thumbs");

        let image_ids: Vec<i64> = photos.iter().map(|p| p.id).collect();
        let prefetch_images: Vec<(i64, PathBuf)> = photos
            .iter()
            .map(|p| (p.id, PathBuf::from(&p.preview_path)))
            .collect();

        self.current_shoot_id = Some(shoot_id);
        self.preview_dir = Some(preview_dir.clone());
        self.thumb_dir = Some(thumb_dir);
        self.image_ids = image_ids;
        self.prefetch.set_images(prefetch_images);
        self.prefetch.set_preview_dir(preview_dir);
        self.cache.clear();

        Ok(())
    }
}
