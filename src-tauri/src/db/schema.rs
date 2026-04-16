use rusqlite::{params, Connection, OptionalExtension, Result};
use std::path::{Path, PathBuf};

pub struct Database {
    conn: Connection,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShootRow {
    pub id: i64,
    pub slug: String,
    pub date: String,
    pub source_path: String,
    pub dest_path: String,
    pub photo_count: i64,
    pub imported_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PhotoRow {
    pub id: i64,
    pub shoot_id: i64,
    pub filename: String,
    pub raw_path: String,
    pub preview_path: String,
    pub thumb_path: String,
    pub exif_date: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
    pub flag: String,
    pub destination: String,
    pub star_rating: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UndoEntry {
    pub id: i64,
    pub photo_id: i64,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupData {
    pub id: i64,
    pub shoot_id: i64,
    pub group_type: String,
    pub members: Vec<GroupMemberData>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMemberData {
    pub photo_id: i64,
    pub is_cover: bool,
}

#[derive(Debug)]
pub struct PhotoInsert {
    pub filename: String,
    pub raw_path: String,
    pub preview_path: String,
    pub thumb_path: String,
    pub content_hash: [u8; 32],
    pub phash: Option<[u8; 8]>,
    pub exif_date: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i32>,
}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    /// Open the global photosift DB at ~/.photosift/photosift.db
    pub fn open_global() -> Result<Self> {
        let path = global_db_path();
        Self::open(&path)
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS shoots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT NOT NULL,
                date TEXT NOT NULL,
                source_path TEXT NOT NULL,
                dest_path TEXT NOT NULL,
                photo_count INTEGER NOT NULL DEFAULT 0,
                imported_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS photos (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                filename TEXT NOT NULL,
                raw_path TEXT NOT NULL,
                preview_path TEXT NOT NULL,
                thumb_path TEXT NOT NULL,
                content_hash BLOB NOT NULL,
                phash BLOB,
                exif_date TEXT,
                camera TEXT,
                lens TEXT,
                focal_length REAL,
                aperture REAL,
                shutter_speed TEXT,
                iso INTEGER,
                flag TEXT NOT NULL DEFAULT 'unreviewed',
                destination TEXT NOT NULL DEFAULT 'unrouted',
                star_rating INTEGER NOT NULL DEFAULT 0,
                sharpness_score REAL,
                UNIQUE(content_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_photos_shoot ON photos(shoot_id);
            CREATE INDEX IF NOT EXISTS idx_photos_flag ON photos(shoot_id, flag);

            CREATE TABLE IF NOT EXISTS groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                group_type TEXT NOT NULL CHECK(group_type IN ('near_duplicate','related'))
            );
            CREATE INDEX IF NOT EXISTS idx_groups_shoot ON groups(shoot_id);

            CREATE TABLE IF NOT EXISTS group_members (
                group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                is_cover INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(group_id, photo_id)
            );
            CREATE INDEX IF NOT EXISTS idx_gm_photo ON group_members(photo_id);

            CREATE TABLE IF NOT EXISTS view_cursors (
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                view_name TEXT NOT NULL,
                last_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY(shoot_id, view_name)
            );

            CREATE TABLE IF NOT EXISTS undo_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                shoot_id INTEGER NOT NULL REFERENCES shoots(id) ON DELETE CASCADE,
                session_id TEXT NOT NULL,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                field TEXT NOT NULL,
                old_value TEXT,
                new_value TEXT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_undo_session ON undo_log(shoot_id, session_id);
            ",
        )
    }

    // ---- Shoots ----

    pub fn insert_shoot(
        &self,
        slug: &str,
        date: &str,
        source_path: &str,
        dest_path: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO shoots (slug, date, source_path, dest_path)
             VALUES (?1, ?2, ?3, ?4)",
            params![slug, date, source_path, dest_path],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_shoot_photo_count(&self, shoot_id: i64, count: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE shoots SET photo_count = ?2 WHERE id = ?1",
            params![shoot_id, count],
        )?;
        Ok(())
    }

    pub fn delete_shoot(&self, shoot_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM shoots WHERE id = ?1", params![shoot_id])?;
        Ok(())
    }

    pub fn list_shoots(&self) -> Result<Vec<ShootRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, slug, date, source_path, dest_path, photo_count, imported_at
             FROM shoots ORDER BY date DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ShootRow {
                id: row.get(0)?,
                slug: row.get(1)?,
                date: row.get(2)?,
                source_path: row.get(3)?,
                dest_path: row.get(4)?,
                photo_count: row.get(5)?,
                imported_at: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_shoot(&self, shoot_id: i64) -> Result<Option<ShootRow>> {
        self.conn
            .query_row(
                "SELECT id, slug, date, source_path, dest_path, photo_count, imported_at
                 FROM shoots WHERE id = ?1",
                params![shoot_id],
                |row| {
                    Ok(ShootRow {
                        id: row.get(0)?,
                        slug: row.get(1)?,
                        date: row.get(2)?,
                        source_path: row.get(3)?,
                        dest_path: row.get(4)?,
                        photo_count: row.get(5)?,
                        imported_at: row.get(6)?,
                    })
                },
            )
            .optional()
    }

    // ---- Photos ----

    pub fn photo_exists_by_hash(&self, content_hash: &[u8; 32]) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM photos WHERE content_hash = ?1",
                params![&content_hash[..]],
                |row| row.get::<_, i64>(0),
            )
            .optional()
    }

    /// Insert a batch of photos for a shoot in a single transaction.
    /// Returns the inserted photo ids in the same order as the input.
    pub fn insert_photos_batch(
        &mut self,
        shoot_id: i64,
        photos: &[PhotoInsert],
    ) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut ids = Vec::with_capacity(photos.len());
        {
            let mut stmt = tx.prepare(
                "INSERT INTO photos (
                    shoot_id, filename, raw_path, preview_path, thumb_path,
                    content_hash, phash, exif_date, camera, lens,
                    focal_length, aperture, shutter_speed, iso
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            )?;
            for p in photos {
                stmt.execute(params![
                    shoot_id,
                    p.filename,
                    p.raw_path,
                    p.preview_path,
                    p.thumb_path,
                    &p.content_hash[..],
                    p.phash.map(|h| h.to_vec()),
                    p.exif_date,
                    p.camera,
                    p.lens,
                    p.focal_length,
                    p.aperture,
                    p.shutter_speed,
                    p.iso,
                ])?;
                ids.push(tx.last_insert_rowid());
            }
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Update file paths for photos after initial insert (preview_path/thumb_path
    /// reference {photo_id}.jpg which isn't known until after insert). Kept separate
    /// from insert to avoid a second copy of the file data; ingest writes the files
    /// keyed by id after this update call.
    pub fn update_photo_paths(
        &self,
        photo_id: i64,
        preview_path: &str,
        thumb_path: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET preview_path = ?2, thumb_path = ?3 WHERE id = ?1",
            params![photo_id, preview_path, thumb_path],
        )?;
        Ok(())
    }

    pub fn get_photo_by_id(&self, photo_id: i64) -> Result<PhotoRow> {
        self.conn.query_row(
            "SELECT id, shoot_id, filename, raw_path, preview_path, thumb_path,
                    exif_date, camera, lens, focal_length, aperture, shutter_speed,
                    iso, flag, destination, star_rating
             FROM photos WHERE id = ?1",
            params![photo_id],
            row_to_photo,
        )
    }

    pub fn photos_for_shoot(&self, shoot_id: i64) -> Result<Vec<PhotoRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, shoot_id, filename, raw_path, preview_path, thumb_path,
                    exif_date, camera, lens, focal_length, aperture, shutter_speed,
                    iso, flag, destination, star_rating
             FROM photos
             WHERE shoot_id = ?1
             ORDER BY exif_date ASC NULLS LAST, id ASC",
        )?;
        let rows = stmt.query_map(params![shoot_id], row_to_photo)?;
        rows.collect()
    }

    pub fn set_star_rating(&self, photo_id: i64, rating: i32) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET star_rating = ?2 WHERE id = ?1",
            params![photo_id, rating],
        )?;
        Ok(())
    }

    pub fn get_star_rating(&self, photo_id: i64) -> Result<i32> {
        self.conn.query_row(
            "SELECT star_rating FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )
    }

    // ---- Groups ----

    pub fn create_group(&self, shoot_id: i64, group_type: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO groups (shoot_id, group_type) VALUES (?1, ?2)",
            params![shoot_id, group_type],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_group_member(&self, group_id: i64, photo_id: i64, is_cover: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO group_members (group_id, photo_id, is_cover)
             VALUES (?1, ?2, ?3)",
            params![group_id, photo_id, is_cover as i32],
        )?;
        Ok(())
    }

    pub fn get_groups_for_shoot(&self, shoot_id: i64) -> Result<Vec<GroupData>> {
        let mut stmt = self.conn.prepare(
            "SELECT g.id, g.shoot_id, g.group_type, gm.photo_id, gm.is_cover
             FROM groups g
             JOIN group_members gm ON gm.group_id = g.id
             WHERE g.shoot_id = ?1
             ORDER BY g.id, gm.photo_id",
        )?;

        let mut groups: Vec<GroupData> = Vec::new();
        let mut current_id: Option<i64> = None;

        let rows = stmt.query_map(params![shoot_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })?;

        for row in rows {
            let (gid, sid, gtype, pid, is_cover) = row?;
            if current_id != Some(gid) {
                groups.push(GroupData {
                    id: gid,
                    shoot_id: sid,
                    group_type: gtype,
                    members: Vec::new(),
                });
                current_id = Some(gid);
            }
            if let Some(g) = groups.last_mut() {
                g.members.push(GroupMemberData {
                    photo_id: pid,
                    is_cover,
                });
            }
        }

        Ok(groups)
    }

    pub fn set_group_cover(&self, group_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE group_members SET is_cover = 0 WHERE group_id = ?1",
            params![group_id],
        )?;
        self.conn.execute(
            "UPDATE group_members SET is_cover = 1 WHERE group_id = ?1 AND photo_id = ?2",
            params![group_id, photo_id],
        )?;
        Ok(())
    }

    // ---- Flag / Destination ----

    pub fn set_flag(&self, photo_id: i64, flag: &str) -> Result<String> {
        let old: String = self.conn.query_row(
            "SELECT flag FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE photos SET flag = ?2 WHERE id = ?1",
            params![photo_id, flag],
        )?;
        Ok(old)
    }

    pub fn set_destination(&self, photo_id: i64, dest: &str) -> Result<String> {
        let old: String = self.conn.query_row(
            "SELECT destination FROM photos WHERE id = ?1",
            params![photo_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "UPDATE photos SET destination = ?2 WHERE id = ?1",
            params![photo_id, dest],
        )?;
        Ok(old)
    }

    pub fn bulk_set_flag(&self, photo_ids: &[i64], flag: &str) -> Result<Vec<(i64, String)>> {
        let mut old_values = Vec::with_capacity(photo_ids.len());
        for &id in photo_ids {
            let old = self.set_flag(id, flag)?;
            old_values.push((id, old));
        }
        Ok(old_values)
    }

    // ---- Undo Log ----

    pub fn append_undo(
        &self,
        shoot_id: i64,
        session_id: &str,
        photo_id: i64,
        field: &str,
        old_value: &str,
        new_value: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO undo_log (shoot_id, session_id, photo_id, field, old_value, new_value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![shoot_id, session_id, photo_id, field, old_value, new_value],
        )?;
        Ok(())
    }

    pub fn pop_undo(&self, shoot_id: i64, session_id: &str) -> Result<Option<UndoEntry>> {
        let entry = self.conn.query_row(
            "SELECT id, photo_id, field, old_value, new_value
             FROM undo_log
             WHERE shoot_id = ?1 AND session_id = ?2
             ORDER BY id DESC LIMIT 1",
            params![shoot_id, session_id],
            |row| {
                Ok(UndoEntry {
                    id: row.get(0)?,
                    photo_id: row.get(1)?,
                    field: row.get(2)?,
                    old_value: row.get(3)?,
                    new_value: row.get(4)?,
                })
            },
        ).optional()?;

        if let Some(ref e) = entry {
            self.conn.execute(
                "DELETE FROM undo_log WHERE id = ?1",
                params![e.id],
            )?;
            match e.field.as_str() {
                "flag" => { self.conn.execute("UPDATE photos SET flag = ?2 WHERE id = ?1", params![e.photo_id, e.old_value])?; }
                "destination" => { self.conn.execute("UPDATE photos SET destination = ?2 WHERE id = ?1", params![e.photo_id, e.old_value])?; }
                "star_rating" => { self.conn.execute("UPDATE photos SET star_rating = ?2 WHERE id = ?1", params![e.photo_id, e.old_value])?; }
                _ => {}
            }
        }

        Ok(entry)
    }

    // ---- View Cursors ----

    pub fn set_view_cursor(&self, shoot_id: i64, view_name: &str, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO view_cursors (shoot_id, view_name, last_photo_id, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(shoot_id, view_name) DO UPDATE SET
                last_photo_id = excluded.last_photo_id,
                updated_at = excluded.updated_at",
            params![shoot_id, view_name, photo_id],
        )?;
        Ok(())
    }

    pub fn get_view_cursor(&self, shoot_id: i64, view_name: &str) -> Result<Option<i64>> {
        self.conn.query_row(
            "SELECT last_photo_id FROM view_cursors
             WHERE shoot_id = ?1 AND view_name = ?2",
            params![shoot_id, view_name],
            |row| row.get(0),
        ).optional()
    }
}

fn row_to_photo(row: &rusqlite::Row) -> Result<PhotoRow> {
    Ok(PhotoRow {
        id: row.get(0)?,
        shoot_id: row.get(1)?,
        filename: row.get(2)?,
        raw_path: row.get(3)?,
        preview_path: row.get(4)?,
        thumb_path: row.get(5)?,
        exif_date: row.get(6)?,
        camera: row.get(7)?,
        lens: row.get(8)?,
        focal_length: row.get(9)?,
        aperture: row.get(10)?,
        shutter_speed: row.get(11)?,
        iso: row.get(12)?,
        flag: row.get(13)?,
        destination: row.get(14)?,
        star_rating: row.get(15)?,
    })
}

/// ~/.photosift/photosift.db
pub fn global_db_path() -> PathBuf {
    photosift_home().join("photosift.db")
}

/// ~/.photosift/
pub fn photosift_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".photosift")
}

/// ~/.photosift/cache/{shoot_id}/
pub fn shoot_cache_dir(shoot_id: i64) -> PathBuf {
    photosift_home()
        .join("cache")
        .join(shoot_id.to_string())
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

    fn sample_insert(hash_byte: u8, filename: &str) -> PhotoInsert {
        PhotoInsert {
            filename: filename.into(),
            raw_path: format!("/fake/{filename}"),
            preview_path: "/fake/p.jpg".into(),
            thumb_path: "/fake/t.jpg".into(),
            content_hash: [hash_byte; 32],
            phash: Some([0u8; 8]),
            exif_date: Some("2026-04-15T10:00:00".into()),
            camera: Some("NIKON D750".into()),
            lens: Some("50mm".into()),
            focal_length: Some(50.0),
            aperture: Some(1.8),
            shutter_speed: Some("1/250".into()),
            iso: Some(400),
        }
    }

    #[test]
    fn test_schema_creates_all_tables() {
        let (db, _dir) = test_db();
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN
                 ('shoots','photos','groups','group_members','view_cursors','undo_log')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn test_shoot_roundtrip() {
        let (db, _dir) = test_db();
        let id = db
            .insert_shoot("Greece", "2026-06-01", "/src", "/dst")
            .unwrap();
        let s = db.get_shoot(id).unwrap().unwrap();
        assert_eq!(s.slug, "Greece");
        assert_eq!(s.photo_count, 0);

        db.update_shoot_photo_count(id, 42).unwrap();
        let s2 = db.get_shoot(id).unwrap().unwrap();
        assert_eq!(s2.photo_count, 42);

        let listed = db.list_shoots().unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn test_insert_photos_batch_and_read() {
        let (mut db, _dir) = test_db();
        let shoot_id = db
            .insert_shoot("Test", "2026-04-15", "/s", "/d")
            .unwrap();

        let photos = vec![sample_insert(1, "a.nef"), sample_insert(2, "b.nef")];
        let ids = db.insert_photos_batch(shoot_id, &photos).unwrap();
        assert_eq!(ids.len(), 2);

        let rows = db.photos_for_shoot(shoot_id).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].flag, "unreviewed");
        assert_eq!(rows[0].destination, "unrouted");
        assert_eq!(rows[0].star_rating, 0);
    }

    #[test]
    fn test_dedup_by_hash() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d").unwrap();
        db.insert_photos_batch(shoot_id, &[sample_insert(7, "x.nef")])
            .unwrap();

        let existing = db.photo_exists_by_hash(&[7u8; 32]).unwrap();
        assert!(existing.is_some());
        let missing = db.photo_exists_by_hash(&[99u8; 32]).unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_star_rating() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef")])
            .unwrap();

        db.set_star_rating(ids[0], 4).unwrap();
        assert_eq!(db.get_star_rating(ids[0]).unwrap(), 4);
    }

    #[test]
    fn test_groups_and_members() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d").unwrap();
        let ids = db
            .insert_photos_batch(
                shoot_id,
                &[sample_insert(1, "a.nef"), sample_insert(2, "b.nef")],
            )
            .unwrap();

        let group_id = db.create_group(shoot_id, "near_duplicate").unwrap();
        db.add_group_member(group_id, ids[0], true).unwrap();
        db.add_group_member(group_id, ids[1], false).unwrap();

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM group_members WHERE group_id = ?1",
                params![group_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
}
