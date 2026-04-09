use rusqlite::{params, Connection, Result};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageRow {
    pub id: i64,
    pub filepath: String,
    pub filename: String,
    pub file_hash: Option<String>,
    pub file_size: Option<i64>,
    pub capture_time: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub orientation: Option<i32>,
    pub star_rating: i32,
    pub sort_order: i32,
}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS images (
                id INTEGER PRIMARY KEY,
                filepath TEXT UNIQUE NOT NULL,
                filename TEXT NOT NULL,
                file_hash TEXT,
                file_size INTEGER,
                capture_time TEXT,
                camera_model TEXT,
                lens TEXT,
                focal_length REAL,
                aperture REAL,
                shutter_speed TEXT,
                iso INTEGER,
                width INTEGER,
                height INTEGER,
                orientation INTEGER,
                star_rating INTEGER DEFAULT 0,
                sort_order INTEGER,
                thumbnail_blob BLOB,
                embedded_preview_path TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_images_capture_time ON images(capture_time);
            CREATE INDEX IF NOT EXISTS idx_images_sort_order ON images(sort_order);",
        )
    }

    pub fn insert_image(
        &self,
        filepath: &str,
        filename: &str,
        file_hash: &str,
        file_size: i64,
        sort_order: i32,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT OR IGNORE INTO images (filepath, filename, file_hash, file_size, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![filepath, filename, file_hash, file_size, sort_order],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_exif(
        &self,
        image_id: i64,
        capture_time: Option<&str>,
        camera_model: Option<&str>,
        lens: Option<&str>,
        focal_length: Option<f64>,
        aperture: Option<f64>,
        shutter_speed: Option<&str>,
        iso: Option<i32>,
        width: Option<i32>,
        height: Option<i32>,
        orientation: Option<i32>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET
                capture_time = ?2, camera_model = ?3, lens = ?4,
                focal_length = ?5, aperture = ?6, shutter_speed = ?7,
                iso = ?8, width = ?9, height = ?10, orientation = ?11,
                updated_at = datetime('now')
             WHERE id = ?1",
            params![
                image_id, capture_time, camera_model, lens,
                focal_length, aperture, shutter_speed,
                iso, width, height, orientation
            ],
        )?;
        Ok(())
    }

    pub fn set_thumbnail(&self, image_id: i64, jpeg_bytes: &[u8]) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET thumbnail_blob = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![image_id, jpeg_bytes],
        )?;
        Ok(())
    }

    pub fn get_thumbnail(&self, image_id: i64) -> Result<Option<Vec<u8>>> {
        self.conn.query_row(
            "SELECT thumbnail_blob FROM images WHERE id = ?1",
            params![image_id],
            |row| row.get(0),
        )
    }

    pub fn set_embedded_preview_path(&self, image_id: i64, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET embedded_preview_path = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![image_id, path],
        )?;
        Ok(())
    }

    pub fn set_star_rating(&self, image_id: i64, rating: i32) -> Result<()> {
        self.conn.execute(
            "UPDATE images SET star_rating = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![image_id, rating],
        )?;
        Ok(())
    }

    pub fn get_star_rating(&self, image_id: i64) -> Result<i32> {
        self.conn.query_row(
            "SELECT star_rating FROM images WHERE id = ?1",
            params![image_id],
            |row| row.get(0),
        )
    }

    pub fn get_all_images(&self) -> Result<Vec<ImageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, filepath, filename, file_hash, file_size,
                    capture_time, camera_model, lens, focal_length, aperture,
                    shutter_speed, iso, width, height, orientation,
                    star_rating, sort_order
             FROM images ORDER BY sort_order ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ImageRow {
                id: row.get(0)?,
                filepath: row.get(1)?,
                filename: row.get(2)?,
                file_hash: row.get(3)?,
                file_size: row.get(4)?,
                capture_time: row.get(5)?,
                camera_model: row.get(6)?,
                lens: row.get(7)?,
                focal_length: row.get(8)?,
                aperture: row.get(9)?,
                shutter_speed: row.get(10)?,
                iso: row.get(11)?,
                width: row.get(12)?,
                height: row.get(13)?,
                orientation: row.get(14)?,
                star_rating: row.get(15)?,
                sort_order: row.get(16)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_image_by_id(&self, image_id: i64) -> Result<ImageRow> {
        self.conn.query_row(
            "SELECT id, filepath, filename, file_hash, file_size,
                    capture_time, camera_model, lens, focal_length, aperture,
                    shutter_speed, iso, width, height, orientation,
                    star_rating, sort_order
             FROM images WHERE id = ?1",
            params![image_id],
            |row| {
                Ok(ImageRow {
                    id: row.get(0)?,
                    filepath: row.get(1)?,
                    filename: row.get(2)?,
                    file_hash: row.get(3)?,
                    file_size: row.get(4)?,
                    capture_time: row.get(5)?,
                    camera_model: row.get(6)?,
                    lens: row.get(7)?,
                    focal_length: row.get(8)?,
                    aperture: row.get(9)?,
                    shutter_speed: row.get(10)?,
                    iso: row.get(11)?,
                    width: row.get(12)?,
                    height: row.get(13)?,
                    orientation: row.get(14)?,
                    star_rating: row.get(15)?,
                    sort_order: row.get(16)?,
                })
            },
        )
    }

    pub fn get_image_count(&self) -> Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM images", [], |row| row.get(0))
    }

    pub fn image_exists(&self, filepath: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM images WHERE filepath = ?1",
            params![filepath],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_db() -> (Database, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.sqlite");
        let db = Database::open(&db_path).unwrap();
        (db, dir)
    }

    #[test]
    fn test_insert_and_retrieve_image() {
        let (db, _dir) = test_db();
        let id = db.insert_image("/photos/test.nef", "test.nef", "abc123", 50_000_000, 0).unwrap();
        assert!(id > 0);

        let img = db.get_image_by_id(id).unwrap();
        assert_eq!(img.filename, "test.nef");
        assert_eq!(img.filepath, "/photos/test.nef");
        assert_eq!(img.star_rating, 0);
    }

    #[test]
    fn test_set_and_get_star_rating() {
        let (db, _dir) = test_db();
        let id = db.insert_image("/photos/test.nef", "test.nef", "abc", 100, 0).unwrap();

        db.set_star_rating(id, 3).unwrap();
        assert_eq!(db.get_star_rating(id).unwrap(), 3);

        db.set_star_rating(id, 5).unwrap();
        assert_eq!(db.get_star_rating(id).unwrap(), 5);

        db.set_star_rating(id, 0).unwrap();
        assert_eq!(db.get_star_rating(id).unwrap(), 0);
    }

    #[test]
    fn test_update_exif() {
        let (db, _dir) = test_db();
        let id = db.insert_image("/photos/test.nef", "test.nef", "abc", 100, 0).unwrap();

        db.update_exif(
            id,
            Some("2026-04-08T10:30:00"),
            Some("NIKON D750"),
            Some("AF-S NIKKOR 85mm f/1.8G"),
            Some(85.0),
            Some(1.8),
            Some("1/200"),
            Some(400),
            Some(6016),
            Some(4016),
            Some(1),
        ).unwrap();

        let img = db.get_image_by_id(id).unwrap();
        assert_eq!(img.camera_model.as_deref(), Some("NIKON D750"));
        assert_eq!(img.iso, Some(400));
        assert_eq!(img.focal_length, Some(85.0));
    }

    #[test]
    fn test_thumbnail_roundtrip() {
        let (db, _dir) = test_db();
        let id = db.insert_image("/photos/test.nef", "test.nef", "abc", 100, 0).unwrap();

        let fake_jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        db.set_thumbnail(id, &fake_jpeg).unwrap();

        let retrieved = db.get_thumbnail(id).unwrap().unwrap();
        assert_eq!(retrieved, fake_jpeg);
    }

    #[test]
    fn test_get_all_images_sorted() {
        let (db, _dir) = test_db();
        db.insert_image("/b.nef", "b.nef", "b", 100, 1).unwrap();
        db.insert_image("/a.nef", "a.nef", "a", 100, 0).unwrap();
        db.insert_image("/c.nef", "c.nef", "c", 100, 2).unwrap();

        let images = db.get_all_images().unwrap();
        assert_eq!(images.len(), 3);
        assert_eq!(images[0].filename, "a.nef");
        assert_eq!(images[1].filename, "b.nef");
        assert_eq!(images[2].filename, "c.nef");
    }

    #[test]
    fn test_image_exists() {
        let (db, _dir) = test_db();
        db.insert_image("/test.nef", "test.nef", "abc", 100, 0).unwrap();

        assert!(db.image_exists("/test.nef").unwrap());
        assert!(!db.image_exists("/nonexistent.nef").unwrap());
    }

    #[test]
    fn test_duplicate_filepath_ignored() {
        let (db, _dir) = test_db();
        db.insert_image("/test.nef", "test.nef", "abc", 100, 0).unwrap();
        db.insert_image("/test.nef", "test.nef", "abc", 100, 0).unwrap();

        assert_eq!(db.get_image_count().unwrap(), 1);
    }
}
