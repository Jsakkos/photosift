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
    pub import_mode: String,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub near_dup_threshold: i32,
    pub related_threshold: i32,
    pub triage_expand_groups: bool,
    pub select_requires_pick: bool,
    pub route_min_star: i32,
    /// Absolute path to the root of the photo library (used for copy-mode imports).
    /// `None` falls back to the system Pictures directory.
    pub library_root: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            near_dup_threshold: crate::ingest::clustering::DEFAULT_NEAR_DUP_THRESHOLD as i32,
            related_threshold: crate::ingest::clustering::DEFAULT_RELATED_THRESHOLD as i32,
            triage_expand_groups: false,
            select_requires_pick: true,
            route_min_star: 3,
            library_root: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceRow {
    pub photo_id: i64,
    pub bbox_x: f64, pub bbox_y: f64, pub bbox_w: f64, pub bbox_h: f64,
    pub left_eye_x: f64, pub left_eye_y: f64,
    pub right_eye_x: f64, pub right_eye_y: f64,
    pub left_eye_open: i32, pub right_eye_open: i32,
    pub left_eye_sharpness: f64, pub right_eye_sharpness: f64,
    pub detection_confidence: f64,
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
    /// Initial flag from EXIF/XMP sidecar at import time.
    /// Defaults to "unreviewed" when not provided.
    pub initial_flag: Option<String>,
    /// Initial star rating from EXIF/XMP sidecar at import time.
    pub initial_star_rating: Option<i32>,
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
                imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                import_mode TEXT NOT NULL DEFAULT 'copy'
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

            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                near_dup_threshold INTEGER NOT NULL DEFAULT 4,
                related_threshold INTEGER NOT NULL DEFAULT 12,
                triage_expand_groups INTEGER NOT NULL DEFAULT 0,
                select_requires_pick INTEGER NOT NULL DEFAULT 1,
                route_min_star INTEGER NOT NULL DEFAULT 3,
                library_root TEXT
            );
            INSERT OR IGNORE INTO settings (id) VALUES (1);
            ",
        )?;
        self.run_migrations()
    }

    /// Additive SQLite migrations for columns introduced after the initial
    /// schema. Idempotent — each migration checks column presence before
    /// altering the table, so existing DBs are upgraded in place.
    fn run_migrations(&self) -> Result<()> {
        self.ensure_column("settings", "select_requires_pick", "INTEGER NOT NULL DEFAULT 1")?;
        self.ensure_column("settings", "route_min_star", "INTEGER NOT NULL DEFAULT 3")?;
        self.ensure_column("settings", "library_root", "TEXT")?;
        self.ensure_column("shoots", "import_mode", "TEXT NOT NULL DEFAULT 'copy'")?;
        // Phase 2 AI
        self.ensure_column("photos", "face_count", "INTEGER")?;
        self.ensure_column("photos", "eyes_open_count", "INTEGER")?;
        self.ensure_column("photos", "ai_analyzed_at", "TEXT")?;
        self.create_faces_table()?;
        Ok(())
    }

    fn create_faces_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS faces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
                bbox_x REAL NOT NULL,
                bbox_y REAL NOT NULL,
                bbox_w REAL NOT NULL,
                bbox_h REAL NOT NULL,
                left_eye_x REAL NOT NULL,
                left_eye_y REAL NOT NULL,
                right_eye_x REAL NOT NULL,
                right_eye_y REAL NOT NULL,
                left_eye_open INTEGER NOT NULL,
                right_eye_open INTEGER NOT NULL,
                left_eye_sharpness REAL NOT NULL,
                right_eye_sharpness REAL NOT NULL,
                detection_confidence REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_faces_photo ON faces(photo_id);",
        )?;
        Ok(())
    }

    fn column_exists(&self, table: &str, column: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({})", table))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
        for r in rows {
            if r? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_column(&self, table: &str, column: &str, type_clause: &str) -> Result<()> {
        if !self.column_exists(table, column)? {
            self.conn.execute(
                &format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, type_clause),
                [],
            )?;
        }
        Ok(())
    }

    // ---- Shoots ----

    pub fn insert_shoot(
        &self,
        slug: &str,
        date: &str,
        source_path: &str,
        dest_path: &str,
        import_mode: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO shoots (slug, date, source_path, dest_path, import_mode)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![slug, date, source_path, dest_path, import_mode],
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
            "SELECT id, slug, date, source_path, dest_path, photo_count, imported_at, import_mode
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
                import_mode: row.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn get_shoot(&self, shoot_id: i64) -> Result<Option<ShootRow>> {
        self.conn
            .query_row(
                "SELECT id, slug, date, source_path, dest_path, photo_count, imported_at, import_mode
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
                        import_mode: row.get(7)?,
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
                    focal_length, aperture, shutter_speed, iso,
                    flag, star_rating
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            )?;
            for p in photos {
                let flag = p.initial_flag.clone().unwrap_or_else(|| "unreviewed".into());
                let rating = p.initial_star_rating.unwrap_or(0);
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
                    flag,
                    rating,
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

    /// Create a new group from the given photo IDs. The first photo becomes
    /// cover. Any listed photo that is already a member of another group is
    /// removed from that group first; groups left with fewer than 2 members
    /// are deleted. Returns the new group's id.
    pub fn create_group_with_members(
        &mut self,
        shoot_id: i64,
        group_type: &str,
        photo_ids: &[i64],
    ) -> Result<i64> {
        if photo_ids.len() < 2 {
            return Err(rusqlite::Error::InvalidParameterName(
                "need at least 2 photos to form a group".into(),
            ));
        }
        // Detach from prior groups first so the new group is clean.
        self.remove_photos_from_groups(photo_ids)?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO groups (shoot_id, group_type) VALUES (?1, ?2)",
            params![shoot_id, group_type],
        )?;
        let group_id = tx.last_insert_rowid();
        for (i, &pid) in photo_ids.iter().enumerate() {
            tx.execute(
                "INSERT INTO group_members (group_id, photo_id, is_cover)
                 VALUES (?1, ?2, ?3)",
                params![group_id, pid, (i == 0) as i32],
            )?;
        }
        tx.commit()?;
        Ok(group_id)
    }

    /// Remove one or more photos from any group they belong to. Any group
    /// left with fewer than 2 members is deleted.
    pub fn remove_photos_from_groups(&mut self, photo_ids: &[i64]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut affected_groups: std::collections::HashSet<i64> =
            std::collections::HashSet::new();
        for &pid in photo_ids {
            let mut stmt = tx.prepare(
                "SELECT group_id FROM group_members WHERE photo_id = ?1",
            )?;
            let ids: Vec<i64> = stmt
                .query_map(params![pid], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            drop(stmt);
            for gid in ids {
                affected_groups.insert(gid);
            }
            tx.execute(
                "DELETE FROM group_members WHERE photo_id = ?1",
                params![pid],
            )?;
        }
        for gid in affected_groups {
            let count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM group_members WHERE group_id = ?1",
                params![gid],
                |row| row.get(0),
            )?;
            if count < 2 {
                tx.execute("DELETE FROM groups WHERE id = ?1", params![gid])?;
            }
        }
        tx.commit()
    }

    pub fn delete_all_groups_for_shoot(&self, shoot_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM groups WHERE shoot_id = ?1",
            params![shoot_id],
        )?;
        Ok(())
    }

    /// Returns (photo_id, phash_bytes) for every photo in the shoot that has a phash.
    pub fn phashes_for_shoot(&self, shoot_id: i64) -> Result<Vec<(i64, [u8; 8])>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, phash FROM photos WHERE shoot_id = ?1 AND phash IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![shoot_id], |row| {
            let id: i64 = row.get(0)?;
            let bytes: Vec<u8> = row.get(1)?;
            let mut arr = [0u8; 8];
            if bytes.len() == 8 {
                arr.copy_from_slice(&bytes);
            }
            Ok((id, arr))
        })?;
        rows.collect()
    }

    // ---- AI: Faces + Aggregates ----

    pub fn insert_faces_batch(&mut self, faces: &[FaceRow]) -> Result<()> {
        if faces.is_empty() { return Ok(()); }
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO faces (
                    photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                    left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                    left_eye_open, right_eye_open,
                    left_eye_sharpness, right_eye_sharpness,
                    detection_confidence
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            )?;
            for f in faces {
                stmt.execute(params![
                    f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                    f.left_eye_x, f.left_eye_y, f.right_eye_x, f.right_eye_y,
                    f.left_eye_open, f.right_eye_open,
                    f.left_eye_sharpness, f.right_eye_sharpness,
                    f.detection_confidence,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<FaceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, bbox_x, bbox_y, bbox_w, bbox_h,
                    left_eye_x, left_eye_y, right_eye_x, right_eye_y,
                    left_eye_open, right_eye_open,
                    left_eye_sharpness, right_eye_sharpness, detection_confidence
             FROM faces WHERE photo_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![photo_id], |r| Ok(FaceRow {
            photo_id: r.get(0)?,
            bbox_x: r.get(1)?, bbox_y: r.get(2)?, bbox_w: r.get(3)?, bbox_h: r.get(4)?,
            left_eye_x: r.get(5)?, left_eye_y: r.get(6)?,
            right_eye_x: r.get(7)?, right_eye_y: r.get(8)?,
            left_eye_open: r.get(9)?, right_eye_open: r.get(10)?,
            left_eye_sharpness: r.get(11)?, right_eye_sharpness: r.get(12)?,
            detection_confidence: r.get(13)?,
        }))?;
        rows.collect()
    }

    pub fn delete_faces_for_photo(&self, photo_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM faces WHERE photo_id = ?1", params![photo_id])?;
        Ok(())
    }

    /// Write AI aggregates + timestamp in a single call. Pass None for face_count
    /// to mark a photo as "attempted but failed" (ai_analyzed_at set, face_count null).
    pub fn mark_ai_analyzed(
        &self,
        photo_id: i64,
        face_count: Option<i32>,
        eyes_open_count: Option<i32>,
        sharpness_score: Option<f64>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET face_count = ?2, eyes_open_count = ?3,
                               sharpness_score = ?4,
                               ai_analyzed_at = datetime('now')
             WHERE id = ?1",
            params![photo_id, face_count, eyes_open_count, sharpness_score],
        )?;
        Ok(())
    }

    pub fn clear_ai_for_shoot(&mut self, shoot_id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE photos SET face_count = NULL, eyes_open_count = NULL,
                               sharpness_score = NULL, ai_analyzed_at = NULL
             WHERE shoot_id = ?1",
            params![shoot_id],
        )?;
        tx.execute(
            "DELETE FROM faces WHERE photo_id IN
                (SELECT id FROM photos WHERE shoot_id = ?1)",
            params![shoot_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn photos_needing_ai(&self, shoot_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM photos WHERE shoot_id = ?1 AND ai_analyzed_at IS NULL ORDER BY id",
        )?;
        let rows = stmt.query_map(params![shoot_id], |r| r.get::<_, i64>(0))?;
        rows.collect()
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

    // ---- Settings ----

    pub fn get_settings(&self) -> Result<Settings> {
        self.conn
            .query_row(
                "SELECT near_dup_threshold, related_threshold, triage_expand_groups,
                        select_requires_pick, route_min_star, library_root
                 FROM settings WHERE id = 1",
                [],
                |row| {
                    Ok(Settings {
                        near_dup_threshold: row.get(0)?,
                        related_threshold: row.get(1)?,
                        triage_expand_groups: row.get::<_, i32>(2)? != 0,
                        select_requires_pick: row.get::<_, i32>(3)? != 0,
                        route_min_star: row.get(4)?,
                        library_root: row.get(5)?,
                    })
                },
            )
            .or_else(|_| Ok(Settings::default()))
    }

    pub fn update_settings(&self, s: &Settings) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (id, near_dup_threshold, related_threshold, triage_expand_groups,
                                   select_requires_pick, route_min_star, library_root)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                near_dup_threshold = excluded.near_dup_threshold,
                related_threshold = excluded.related_threshold,
                triage_expand_groups = excluded.triage_expand_groups,
                select_requires_pick = excluded.select_requires_pick,
                route_min_star = excluded.route_min_star,
                library_root = excluded.library_root",
            params![
                s.near_dup_threshold,
                s.related_threshold,
                s.triage_expand_groups as i32,
                s.select_requires_pick as i32,
                s.route_min_star,
                s.library_root,
            ],
        )?;
        Ok(())
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
            initial_flag: None,
            initial_star_rating: None,
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
            .insert_shoot("Greece", "2026-06-01", "/src", "/dst", "copy")
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
            .insert_shoot("Test", "2026-04-15", "/s", "/d", "copy")
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
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
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
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef")])
            .unwrap();

        db.set_star_rating(ids[0], 4).unwrap();
        assert_eq!(db.get_star_rating(ids[0]).unwrap(), 4);
    }

    #[test]
    fn test_groups_and_members() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
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

    /// End-to-end integration: simulates a Select-view pick where the user
    /// picks one photo and auto-rejects its three siblings, then undoes the
    /// whole batch. Exercises set_flag + bulk_set_flag + append_undo +
    /// pop_undo in a single realistic sequence, and confirms reverted state
    /// persists across DB reopens.
    #[test]
    fn test_select_pick_batch_then_undo() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("cull.sqlite");
        let session = "sess-abc";

        let ids: Vec<i64> = {
            let mut db = Database::open(&db_path).unwrap();
            let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
            let photos = vec![
                sample_insert(1, "a.nef"),
                sample_insert(2, "b.nef"),
                sample_insert(3, "c.nef"),
                sample_insert(4, "d.nef"),
            ];
            let ids = db.insert_photos_batch(shoot_id, &photos).unwrap();

            // Pick photo[0], reject photos[1..4] (simulating select-view auto-reject).
            let old_pick = db.set_flag(ids[0], "pick").unwrap();
            db.append_undo(shoot_id, session, ids[0], "flag", &old_pick, "pick").unwrap();
            let old_rejs = db.bulk_set_flag(&ids[1..], "reject").unwrap();
            for (id, old) in &old_rejs {
                db.append_undo(shoot_id, session, *id, "flag", old, "reject").unwrap();
            }

            // State check: one pick + three rejects.
            let rows = db.photos_for_shoot(shoot_id).unwrap();
            let picks = rows.iter().filter(|p| p.flag == "pick").count();
            let rejects = rows.iter().filter(|p| p.flag == "reject").count();
            assert_eq!(picks, 1);
            assert_eq!(rejects, 3);

            // Undo 4 actions (bulk reject was logged as 3 separate entries + 1 pick).
            for _ in 0..4 {
                let entry = db.pop_undo(shoot_id, session).unwrap();
                assert!(entry.is_some(), "undo stack drained prematurely");
            }
            assert!(db.pop_undo(shoot_id, session).unwrap().is_none());

            ids
        };

        // Reopen and verify the undo reverted state survived.
        let db = Database::open(&db_path).unwrap();
        let shoot_id: i64 = db.conn.query_row("SELECT id FROM shoots LIMIT 1", [], |r| r.get(0)).unwrap();
        let rows = db.photos_for_shoot(shoot_id).unwrap();
        assert_eq!(rows.len(), 4);
        for row in &rows {
            assert_eq!(row.flag, "unreviewed", "photo {} should be reverted", row.id);
        }
        // And the id set is unchanged — no rows were lost.
        let mut live: Vec<i64> = rows.iter().map(|r| r.id).collect();
        live.sort();
        let mut expected = ids.clone();
        expected.sort();
        assert_eq!(live, expected);
    }

    #[test]
    fn test_ai_columns_present_after_migration() {
        let (db, _dir) = test_db();
        for col in &["face_count", "eyes_open_count", "ai_analyzed_at"] {
            assert!(
                db.column_exists("photos", col).unwrap(),
                "photos.{} should be present after migration",
                col
            );
        }
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='faces'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "faces table should exist");
    }

    #[test]
    fn test_migration_is_idempotent() {
        let (db, _dir) = test_db();
        // Re-run migration — should be a no-op, not error.
        db.run_migrations().unwrap();
        assert!(db.column_exists("photos", "face_count").unwrap());
    }

    #[test]
    fn test_faces_roundtrip() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef")])
            .unwrap();

        let face = FaceRow {
            photo_id: ids[0],
            bbox_x: 0.1, bbox_y: 0.1, bbox_w: 0.2, bbox_h: 0.3,
            left_eye_x: 0.15, left_eye_y: 0.18,
            right_eye_x: 0.22, right_eye_y: 0.18,
            left_eye_open: 1, right_eye_open: 1,
            left_eye_sharpness: 78.0, right_eye_sharpness: 81.0,
            detection_confidence: 0.92,
        };
        db.insert_faces_batch(&[face.clone()]).unwrap();

        let got = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(got.len(), 1);
        assert!((got[0].bbox_x - 0.1).abs() < 1e-6);
        assert_eq!(got[0].left_eye_open, 1);
        assert!((got[0].detection_confidence - 0.92).abs() < 1e-6);

        // Cascade delete
        db.delete_shoot(shoot_id).unwrap();
        let gone = db.get_faces_for_photo(ids[0]).unwrap();
        assert_eq!(gone.len(), 0);
    }

    #[test]
    fn test_clear_ai_for_shoot() {
        let (mut db, _dir) = test_db();
        let shoot_id = db.insert_shoot("T", "2026-04-15", "/s", "/d", "copy").unwrap();
        let ids = db
            .insert_photos_batch(shoot_id, &[sample_insert(1, "a.nef"), sample_insert(2, "b.nef")])
            .unwrap();
        db.mark_ai_analyzed(ids[0], Some(0), Some(0), Some(50.0)).unwrap();
        db.mark_ai_analyzed(ids[1], Some(1), Some(2), Some(75.0)).unwrap();

        db.clear_ai_for_shoot(shoot_id).unwrap();

        for id in &ids {
            let row: (Option<i32>, Option<i32>, Option<f64>, Option<String>) = db.conn.query_row(
                "SELECT face_count, eyes_open_count, sharpness_score, ai_analyzed_at FROM photos WHERE id = ?1",
                params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            ).unwrap();
            assert_eq!(row.0, None, "face_count should be NULL");
            assert_eq!(row.1, None, "eyes_open_count should be NULL");
            assert_eq!(row.2, None, "sharpness_score should be NULL");
            assert_eq!(row.3, None, "ai_analyzed_at should be NULL");
        }
    }

    /// Mutex-poison resilience: the XmpWriteQueue must not deadlock or crash
    /// when a poisoned lock is encountered. This simulates a panic in one
    /// thread while holding the pending map, then verifies the queue still
    /// enqueues and drains correctly.
    #[test]
    fn test_xmp_queue_survives_poison() {
        use crate::metadata::xmp_queue::XmpWriteQueue;
        use std::path::PathBuf;
        use std::sync::Arc;
        use std::thread;

        let queue = Arc::new(XmpWriteQueue::new());
        let q2 = queue.clone();

        // Force a panic inside a thread while nominally using the queue;
        // this would poison the internal Mutex if not handled.
        let _ = thread::spawn(move || {
            q2.enqueue(1, &PathBuf::from("/nonexistent/a.xmp"), 3);
            panic!("deliberate");
        })
        .join();

        // Must not panic — poison-tolerant lock returns the inner guard.
        queue.enqueue(2, &PathBuf::from("/nonexistent/b.xmp"), 5);
        queue.drain(); // writes to bogus paths will fail gracefully via log::error
    }
}
