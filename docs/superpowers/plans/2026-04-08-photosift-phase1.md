# PhotoSift Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows photo culling tool that opens a folder of NEF/JPEG files, displays them with zero-lag navigation, and writes star ratings to XMP sidecars.

**Architecture:** Tauri 2.x app with a Rust backend handling RAW decode, image caching, and XMP I/O via custom protocol handler. React frontend with Zustand state, virtualized filmstrip, and keyboard-driven navigation. Three-tier preview strategy (embedded JPEG → decoded preview → full res) with LRU cache and directional prefetch.

**Tech Stack:** Rust (rawler, image, kamadak-exif, quick-xml, rusqlite, rayon, lru), Tauri 2.x, React 18, TypeScript, Zustand, Tailwind CSS, react-window, Vite.

---

## File Map

### Rust Backend (`src-tauri/`)

| File | Responsibility |
|---|---|
| `src/main.rs` | Tauri entry point, plugin registration |
| `src/lib.rs` | Module tree, Tauri command registration, custom protocol setup |
| `src/commands/mod.rs` | Re-export command modules |
| `src/commands/project.rs` | `open_project`, `get_project_info` commands |
| `src/commands/image.rs` | `get_image_metadata`, `get_image_list` commands |
| `src/commands/rating.rs` | `set_rating`, `get_rating` commands |
| `src/pipeline/mod.rs` | Re-export pipeline modules |
| `src/pipeline/decoder.rs` | RAW decode via rawler, JPEG decode via image crate |
| `src/pipeline/embedded.rs` | Embedded JPEG extraction from NEF files (TIFF IFD parsing) |
| `src/pipeline/cache.rs` | LRU cache for decoded images, thumbnail cache |
| `src/pipeline/prefetch.rs` | Sliding window prefetch manager with direction awareness |
| `src/pipeline/protocol.rs` | Tauri custom protocol handler (serves images + thumbnails) |
| `src/metadata/mod.rs` | Re-export metadata modules |
| `src/metadata/exif.rs` | EXIF extraction via kamadak-exif |
| `src/metadata/xmp.rs` | XMP sidecar read/write/merge via quick-xml |
| `src/db/mod.rs` | Re-export db module |
| `src/db/schema.rs` | SQLite schema creation, image CRUD, thumbnail storage |
| `src/state.rs` | AppState struct (shared across commands via Tauri managed state) |

### React Frontend (`src/`)

| File | Responsibility |
|---|---|
| `src/main.tsx` | React entry point |
| `src/App.tsx` | Root layout, view routing, keyboard listener |
| `src/components/LoupeView.tsx` | Main image display with two-img swap strategy |
| `src/components/Filmstrip.tsx` | Virtualized horizontal thumbnail strip |
| `src/components/Toolbar.tsx` | Project name, image counter, auto-advance toggle |
| `src/components/MetadataOverlay.tsx` | EXIF info overlay (toggleable) |
| `src/components/RatingBar.tsx` | Star rating display and click targets |
| `src/components/ShortcutHints.tsx` | Keyboard shortcut overlay |
| `src/hooks/useKeyboardNav.ts` | Arrow key + rating + zoom keyboard handler |
| `src/hooks/useImageLoader.ts` | Image loading via custom protocol with tier swap |
| `src/stores/projectStore.ts` | Zustand store: images, current index, ratings, UI state |
| `src/types/index.ts` | Shared TypeScript interfaces |
| `src/styles/globals.css` | Tailwind base + dark theme variables |

---

## Task 1: Tauri + React Project Scaffolding

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `package.json`, `tsconfig.json`, `vite.config.ts`, `tailwind.config.js`
- Create: `src/main.tsx`, `src/App.tsx`, `src/styles/globals.css`
- Create: `src/types/index.ts`

- [ ] **Step 1: Initialize the Tauri 2.x project**

Run:
```bash
npm create tauri-app@latest photosift-init -- --template react-ts --manager npm
```

Then copy the generated files into our project root. This gives us the Tauri + Vite + React + TypeScript scaffolding.

- [ ] **Step 2: Configure Rust dependencies in Cargo.toml**

Edit `src-tauri/Cargo.toml`:
```toml
[package]
name = "photosift"
version = "0.1.0"
edition = "2021"

[lib]
name = "photosift_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.31", features = ["bundled"] }
rawler = "0.7"
image = { version = "0.25", features = ["jpeg", "tiff", "png"] }
kamadak-exif = "0.5"
quick-xml = "0.36"
rayon = "1.10"
lru = "0.12"
sha2 = "0.10"
chrono = { version = "0.4", features = ["serde"] }
log = "0.4"
env_logger = "0.11"
thiserror = "2"
http = "1"
```

- [ ] **Step 3: Configure Tauri app settings**

Edit `src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/schema.json",
  "productName": "PhotoSift",
  "version": "0.1.0",
  "identifier": "com.photosift.app",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "title": "PhotoSift",
    "windows": [
      {
        "title": "PhotoSift",
        "width": 1400,
        "height": 900,
        "resizable": true,
        "fullscreen": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' http://photosift.localhost; style-src 'self' 'unsafe-inline'"
    }
  },
  "plugins": {
    "dialog": {}
  }
}
```

- [ ] **Step 4: Install frontend dependencies**

Run:
```bash
npm install zustand react-window @tauri-apps/api @tauri-apps/plugin-dialog
npm install -D tailwindcss @tailwindcss/vite @types/react-window
```

- [ ] **Step 5: Configure Tailwind CSS**

Edit `vite.config.ts`:
```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
```

Edit `src/styles/globals.css`:
```css
@import "tailwindcss";

:root {
  --bg-primary: #0a0a0a;
  --bg-secondary: #141414;
  --bg-tertiary: #1e1e1e;
  --text-primary: #e5e5e5;
  --text-secondary: #a3a3a3;
  --accent: #3b82f6;
  --accent-hover: #2563eb;
  --star-filled: #f59e0b;
  --star-empty: #404040;
}

body {
  margin: 0;
  padding: 0;
  background-color: var(--bg-primary);
  color: var(--text-primary);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  overflow: hidden;
  user-select: none;
}
```

- [ ] **Step 6: Create TypeScript types**

Create `src/types/index.ts`:
```typescript
export interface ImageEntry {
  id: number;
  filepath: string;
  filename: string;
  captureTime: string | null;
  cameraModel: string | null;
  lens: string | null;
  focalLength: number | null;
  aperture: number | null;
  shutterSpeed: string | null;
  iso: number | null;
  width: number | null;
  height: number | null;
  orientation: number | null;
  starRating: number;
}

export interface ProjectInfo {
  folderPath: string;
  imageCount: number;
  lastViewedIndex: number;
}

export interface ExifMetadata {
  filename: string;
  captureTime: string | null;
  cameraModel: string | null;
  lens: string | null;
  focalLength: number | null;
  aperture: number | null;
  shutterSpeed: string | null;
  iso: number | null;
  width: number | null;
  height: number | null;
}
```

- [ ] **Step 7: Create minimal App shell**

Edit `src/main.tsx`:
```typescript
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/globals.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

Edit `src/App.tsx`:
```typescript
function App() {
  return (
    <div className="h-screen w-screen flex items-center justify-center bg-[var(--bg-primary)]">
      <p className="text-[var(--text-secondary)] text-lg">
        PhotoSift — Drop a folder or press Ctrl+O to open
      </p>
    </div>
  );
}

export default App;
```

- [ ] **Step 8: Create Rust entry points**

Edit `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    photosift_lib::run();
}
```

Edit `src-tauri/src/lib.rs`:
```rust
mod commands;
mod db;
mod metadata;
mod pipeline;
mod state;

use state::AppState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let app_state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Mutex::new(app_state))
        .invoke_handler(tauri::generate_handler![
            commands::project::open_project,
            commands::project::get_project_info,
            commands::image::get_image_list,
            commands::image::get_image_metadata,
            commands::rating::set_rating,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 9: Create stub modules so the project compiles**

Create `src-tauri/src/state.rs`:
```rust
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
```

Create `src-tauri/src/commands/mod.rs`:
```rust
pub mod image;
pub mod project;
pub mod rating;
```

Create `src-tauri/src/commands/project.rs`:
```rust
#[tauri::command]
pub fn open_project() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn get_project_info() -> Result<(), String> {
    Ok(())
}
```

Create `src-tauri/src/commands/image.rs`:
```rust
#[tauri::command]
pub fn get_image_list() -> Result<Vec<()>, String> {
    Ok(vec![])
}

#[tauri::command]
pub fn get_image_metadata() -> Result<(), String> {
    Ok(())
}
```

Create `src-tauri/src/commands/rating.rs`:
```rust
#[tauri::command]
pub fn set_rating() -> Result<(), String> {
    Ok(())
}
```

Create `src-tauri/src/pipeline/mod.rs`:
```rust
pub mod cache;
pub mod decoder;
pub mod embedded;
pub mod prefetch;
pub mod protocol;
```

Create `src-tauri/src/pipeline/cache.rs`:
```rust
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
```

Create `src-tauri/src/pipeline/decoder.rs`:
```rust
// RAW and JPEG decode — implemented in Task 4
```

Create `src-tauri/src/pipeline/embedded.rs`:
```rust
// Embedded JPEG extraction — implemented in Task 3
```

Create `src-tauri/src/pipeline/prefetch.rs`:
```rust
// Prefetch manager — implemented in Task 8
```

Create `src-tauri/src/pipeline/protocol.rs`:
```rust
// Custom protocol handler — implemented in Task 7
```

Create `src-tauri/src/metadata/mod.rs`:
```rust
pub mod exif;
pub mod xmp;
```

Create `src-tauri/src/metadata/exif.rs`:
```rust
// EXIF extraction — implemented in Task 5
```

Create `src-tauri/src/metadata/xmp.rs`:
```rust
// XMP sidecar I/O — implemented in Task 11
```

Create `src-tauri/src/db/mod.rs`:
```rust
pub mod schema;
```

Create `src-tauri/src/db/schema.rs`:
```rust
// Database — implemented in Task 2
pub struct Database;
```

- [ ] **Step 10: Verify the project builds**

Run:
```bash
npm run tauri dev
```

Expected: Tauri window opens showing "PhotoSift — Drop a folder or press Ctrl+O to open" on a dark background.

- [ ] **Step 11: Commit**

```bash
git init
git add -A
git commit -m "feat: scaffold Tauri 2.x + React + TypeScript project

Tauri app with Rust backend and React frontend. All Rust dependencies
configured. Stub modules compile. Dark theme base CSS."
```

---

## Task 2: SQLite Database Layer

**Files:**
- Modify: `src-tauri/src/db/schema.rs`

- [ ] **Step 1: Implement the Database struct with schema creation**

Edit `src-tauri/src/db/schema.rs`:
```rust
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
```

- [ ] **Step 2: Write tests for the database layer**

Create `src-tauri/src/db/tests.rs`:
```rust
#[cfg(test)]
mod tests {
    use crate::db::schema::Database;
    use std::path::PathBuf;
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
        db.insert_image("/test.nef", "test.nef", "abc", 100, 0).unwrap(); // Should not error

        assert_eq!(db.get_image_count().unwrap(), 1);
    }
}
```

Add to `src-tauri/src/db/mod.rs`:
```rust
pub mod schema;
#[cfg(test)]
mod tests;
```

Add `tempfile` as a dev dependency in `Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

Run:
```bash
cd src-tauri && cargo test db::tests -- --nocapture
```

Expected: All 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/db/ src-tauri/Cargo.toml
git commit -m "feat: SQLite database layer with image CRUD and tests

Schema for images table with EXIF, ratings, thumbnails. Full test
coverage for insert, update, query, and edge cases."
```

---

## Task 3: Embedded JPEG Extraction from NEF

**Files:**
- Modify: `src-tauri/src/pipeline/embedded.rs`

NEF files are TIFF-based. The embedded full-size JPEG preview is stored in IFD0 as a JPEG-compressed strip. We parse the TIFF header to find it.

- [ ] **Step 1: Implement NEF embedded JPEG extractor**

Edit `src-tauri/src/pipeline/embedded.rs`:
```rust
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbeddedError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not a valid TIFF/NEF file")]
    InvalidTiff,
    #[error("No embedded JPEG found")]
    NoJpeg,
}

/// Extract the largest embedded JPEG preview from a NEF (TIFF-based) file.
/// NEF files contain one or more JPEG previews in their IFDs.
/// We scan for JPEG markers (FFD8) and return the largest blob found.
pub fn extract_embedded_jpeg(path: &Path) -> Result<Vec<u8>, EmbeddedError> {
    let mut file = BufReader::new(File::open(path)?);
    let mut header = [0u8; 4];
    file.read_exact(&mut header)?;

    // Check TIFF magic: II (little-endian) or MM (big-endian) + 42
    let little_endian = match &header[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err(EmbeddedError::InvalidTiff),
    };

    let magic = if little_endian {
        u16::from_le_bytes([header[2], header[3]])
    } else {
        u16::from_be_bytes([header[2], header[3]])
    };
    if magic != 42 {
        return Err(EmbeddedError::InvalidTiff);
    }

    // Read the full file to scan for JPEG blobs
    file.seek(SeekFrom::Start(0))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;

    // Find all JPEG blobs (FFD8...FFD9) and return the largest
    let mut best_jpeg: Option<Vec<u8>> = None;
    let mut i = 0;

    while i < data.len().saturating_sub(1) {
        if data[i] == 0xFF && data[i + 1] == 0xD8 {
            // Found JPEG start, find the end (FFD9)
            if let Some(end) = find_jpeg_end(&data, i) {
                let jpeg_data = &data[i..=end];
                let is_larger = best_jpeg
                    .as_ref()
                    .map(|b| jpeg_data.len() > b.len())
                    .unwrap_or(true);
                if is_larger {
                    best_jpeg = Some(jpeg_data.to_vec());
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    best_jpeg.ok_or(EmbeddedError::NoJpeg)
}

/// Find the end of a JPEG blob starting at `start`.
/// Returns the index of the final 0xD9 byte.
fn find_jpeg_end(data: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2; // Skip initial FFD8

    while i < data.len().saturating_sub(1) {
        if data[i] == 0xFF {
            match data[i + 1] {
                0xD9 => return Some(i + 1), // End of JPEG
                0x00 => { i += 2; }          // Escaped FF
                0xD0..=0xD7 => { i += 2; }   // Restart markers (no length)
                0xD8 => { i += 2; }           // Embedded SOI (shouldn't happen but skip)
                _ => {
                    // Marker with length field
                    if i + 3 < data.len() {
                        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                        i += 2 + len;
                    } else {
                        return None;
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Check if a file is a supported RAW format (NEF) based on extension.
pub fn is_raw_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("nef"))
        .unwrap_or(false)
}

/// Check if a file is a supported image format.
pub fn is_supported_image(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "nef" | "jpg" | "jpeg" | "tif" | "tiff"
    )
}
```

- [ ] **Step 2: Write tests**

Create `src-tauri/src/pipeline/tests.rs`:
```rust
#[cfg(test)]
mod embedded_tests {
    use crate::pipeline::embedded::{is_raw_file, is_supported_image};
    use std::path::Path;

    #[test]
    fn test_is_raw_file() {
        assert!(is_raw_file(Path::new("photo.nef")));
        assert!(is_raw_file(Path::new("photo.NEF")));
        assert!(!is_raw_file(Path::new("photo.jpg")));
        assert!(!is_raw_file(Path::new("photo.txt")));
    }

    #[test]
    fn test_is_supported_image() {
        assert!(is_supported_image(Path::new("a.nef")));
        assert!(is_supported_image(Path::new("a.NEF")));
        assert!(is_supported_image(Path::new("a.jpg")));
        assert!(is_supported_image(Path::new("a.JPEG")));
        assert!(is_supported_image(Path::new("a.tif")));
        assert!(is_supported_image(Path::new("a.tiff")));
        assert!(!is_supported_image(Path::new("a.png")));
        assert!(!is_supported_image(Path::new("a.txt")));
    }

    // Note: extract_embedded_jpeg requires real NEF test files.
    // Add integration tests with sample files in Task 16.
}
```

Add to `src-tauri/src/pipeline/mod.rs`:
```rust
pub mod cache;
pub mod decoder;
pub mod embedded;
pub mod prefetch;
pub mod protocol;
#[cfg(test)]
mod tests;
```

- [ ] **Step 3: Run tests**

Run:
```bash
cd src-tauri && cargo test pipeline::tests -- --nocapture
```

Expected: Tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/pipeline/
git commit -m "feat: embedded JPEG extraction from NEF files

Scans TIFF-based NEF for JPEG blobs, returns the largest. Handles
JPEG marker parsing with proper segment length skipping."
```

---

## Task 4: RAW and JPEG Decode Pipeline

**Files:**
- Modify: `src-tauri/src/pipeline/decoder.rs`

- [ ] **Step 1: Implement the decode pipeline**

Edit `src-tauri/src/pipeline/decoder.rs`:
```rust
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageReader};
use rawler::decoders::RawDecodeParams;
use rawler::RawFile;
use std::io::Cursor;
use std::path::Path;
use thiserror::Error;

use crate::pipeline::embedded;

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Image decode error: {0}")]
    Image(#[from] image::ImageError),
    #[error("RAW decode error: {0}")]
    Raw(String),
    #[error("Unsupported format: {0}")]
    Unsupported(String),
}

/// Decode tier for the three-tier preview strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecodeTier {
    /// Embedded JPEG from RAW file header (~1600px)
    Embedded,
    /// Half-res decoded preview (~3000px long edge)
    Preview,
    /// Full resolution decode
    Full,
}

/// Decode an image file at the specified tier and return JPEG bytes.
pub fn decode_to_jpeg(path: &Path, tier: DecodeTier, quality: u8) -> Result<Vec<u8>, DecodeError> {
    if embedded::is_raw_file(path) {
        decode_raw_to_jpeg(path, tier, quality)
    } else {
        decode_standard_to_jpeg(path, tier, quality)
    }
}

fn decode_raw_to_jpeg(path: &Path, tier: DecodeTier, quality: u8) -> Result<Vec<u8>, DecodeError> {
    match tier {
        DecodeTier::Embedded => {
            // Extract embedded JPEG from RAW — no further processing needed
            embedded::extract_embedded_jpeg(path).map_err(|e| DecodeError::Raw(e.to_string()))
        }
        DecodeTier::Preview | DecodeTier::Full => {
            // Decode RAW to pixel data using rawler
            let mut rawfile = RawFile::file(path).map_err(|e| DecodeError::Raw(e.to_string()))?;
            let params = RawDecodeParams::default();
            let raw_image = rawler::decode(&mut rawfile, &params)
                .map_err(|e| DecodeError::Raw(e.to_string()))?;

            // Convert raw pixel data to a DynamicImage
            let width = raw_image.width as u32;
            let height = raw_image.height as u32;

            let dynamic = match raw_image.data {
                rawler::RawImageData::Integer(ref pixels) => {
                    // Convert 16-bit Bayer data to 8-bit RGB via simple demosaic
                    // rawler provides the full pipeline for this
                    let rgb = rawler::imgop::develop::develop_raw_srgb(&raw_image)
                        .map_err(|e| DecodeError::Raw(e.to_string()))?;
                    let (w, h) = (rgb.width as u32, rgb.height as u32);
                    DynamicImage::ImageRgb8(
                        image::RgbImage::from_raw(w, h, rgb.data)
                            .ok_or_else(|| DecodeError::Raw("Failed to create image buffer".into()))?,
                    )
                }
                rawler::RawImageData::Float(ref pixels) => {
                    // Float data — convert to 8-bit
                    let rgb: Vec<u8> = pixels.iter().map(|&p| (p.clamp(0.0, 1.0) * 255.0) as u8).collect();
                    DynamicImage::ImageRgb8(
                        image::RgbImage::from_raw(width, height, rgb)
                            .ok_or_else(|| DecodeError::Raw("Failed to create image buffer".into()))?,
                    )
                }
            };

            let img = if tier == DecodeTier::Preview {
                // Resize to ~3000px long edge for preview
                let long_edge = dynamic.width().max(dynamic.height());
                if long_edge > 3000 {
                    dynamic.resize(3000, 3000, image::imageops::FilterType::Lanczos3)
                } else {
                    dynamic
                }
            } else {
                dynamic
            };

            encode_jpeg(&img, quality)
        }
    }
}

fn decode_standard_to_jpeg(
    path: &Path,
    tier: DecodeTier,
    quality: u8,
) -> Result<Vec<u8>, DecodeError> {
    let img = ImageReader::open(path)?.decode()?;

    let img = match tier {
        DecodeTier::Embedded | DecodeTier::Preview => {
            let long_edge = img.width().max(img.height());
            let target = if tier == DecodeTier::Embedded { 1600 } else { 3000 };
            if long_edge > target {
                img.resize(target, target, image::imageops::FilterType::Lanczos3)
            } else {
                img
            }
        }
        DecodeTier::Full => img,
    };

    encode_jpeg(&img, quality)
}

fn encode_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, DecodeError> {
    let mut buf = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    img.write_with_encoder(encoder)?;
    Ok(buf.into_inner())
}

/// Generate a small thumbnail (200px long edge) from any supported image.
pub fn generate_thumbnail(path: &Path) -> Result<Vec<u8>, DecodeError> {
    if embedded::is_raw_file(path) {
        // Use embedded JPEG as source for thumbnail
        let jpeg_bytes = embedded::extract_embedded_jpeg(path)
            .map_err(|e| DecodeError::Raw(e.to_string()))?;
        let img = image::load_from_memory(&jpeg_bytes)?;
        let thumb = img.resize(200, 200, image::imageops::FilterType::Triangle);
        encode_jpeg(&thumb, 80)
    } else {
        let img = ImageReader::open(path)?.decode()?;
        let thumb = img.resize(200, 200, image::imageops::FilterType::Triangle);
        encode_jpeg(&thumb, 80)
    }
}
```

- [ ] **Step 2: Run compilation check**

Run:
```bash
cd src-tauri && cargo check
```

Expected: Compiles. Note: `rawler::imgop::develop::develop_raw_srgb` may not exist exactly as written — the rawler API may differ. If compilation fails on the rawler develop call, replace with a basic Bayer demosaic using the `image` crate (convert the raw 16-bit data to 8-bit and create an RgbImage directly). The embedded JPEG path is the critical path for Phase 1 — full RAW decode can be refined later.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/pipeline/decoder.rs
git commit -m "feat: image decode pipeline with three-tier strategy

Supports embedded JPEG extraction from NEF, decoded preview (half-res),
and full resolution. JPEG/TIFF pass-through. Thumbnail generation."
```

---

## Task 5: EXIF Metadata Extraction

**Files:**
- Modify: `src-tauri/src/metadata/exif.rs`

- [ ] **Step 1: Implement EXIF extraction**

Edit `src-tauri/src/metadata/exif.rs`:
```rust
use exif::{In, Reader, Tag, Value};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExifData {
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
}

pub fn extract_exif(path: &Path) -> Result<ExifData, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let exif = Reader::new()
        .read_from_container(&mut BufReader::new(&file))
        .map_err(|e| e.to_string())?;

    let capture_time = exif
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .map(|f| f.display_value().to_string());

    let camera_model = exif
        .get_field(Tag::Model, In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string());

    let lens = exif
        .get_field(Tag::LensModel, In::PRIMARY)
        .map(|f| f.display_value().to_string().trim_matches('"').to_string());

    let focal_length = exif.get_field(Tag::FocalLength, In::PRIMARY).and_then(|f| {
        if let Value::Rational(ref v) = f.value {
            v.first().map(|r| r.to_f64())
        } else {
            None
        }
    });

    let aperture = exif.get_field(Tag::FNumber, In::PRIMARY).and_then(|f| {
        if let Value::Rational(ref v) = f.value {
            v.first().map(|r| r.to_f64())
        } else {
            None
        }
    });

    let shutter_speed = exif
        .get_field(Tag::ExposureTime, In::PRIMARY)
        .map(|f| f.display_value().to_string());

    let iso = exif
        .get_field(Tag::PhotographicSensitivity, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let width = exif
        .get_field(Tag::PixelXDimension, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::ImageWidth, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let height = exif
        .get_field(Tag::PixelYDimension, In::PRIMARY)
        .or_else(|| exif.get_field(Tag::ImageLength, In::PRIMARY))
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    let orientation = exif
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0).map(|v| v as i32));

    Ok(ExifData {
        capture_time,
        camera_model,
        lens,
        focal_length,
        aperture,
        shutter_speed,
        iso,
        width,
        height,
        orientation,
    })
}
```

- [ ] **Step 2: Run compilation check**

Run:
```bash
cd src-tauri && cargo check
```

Expected: Compiles. Note: `Tag::PhotographicSensitivity` is the correct tag name in kamadak-exif for ISO. If it doesn't exist, try `Tag::ISOSpeedRatings`.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/metadata/exif.rs
git commit -m "feat: EXIF metadata extraction from NEF/JPEG files

Extracts capture time, camera, lens, focal length, aperture, shutter
speed, ISO, dimensions, and orientation via kamadak-exif."
```

---

## Task 6: XMP Sidecar Read/Write

**Files:**
- Modify: `src-tauri/src/metadata/xmp.rs`

- [ ] **Step 1: Implement XMP sidecar read/write with merge support**

Edit `src-tauri/src/metadata/xmp.rs`:
```rust
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Get the XMP sidecar path for an image file.
/// e.g., "/photos/DSC_1234.NEF" -> "/photos/DSC_1234.xmp"
pub fn sidecar_path(image_path: &Path) -> PathBuf {
    image_path.with_extension("xmp")
}

/// Read the star rating from an existing XMP sidecar.
/// Returns None if the sidecar doesn't exist or has no rating.
pub fn read_rating(image_path: &Path) -> Option<i32> {
    let xmp_path = sidecar_path(image_path);
    let content = std::fs::read_to_string(&xmp_path).ok()?;
    parse_rating_from_xml(&content)
}

fn parse_rating_from_xml(xml: &str) -> Option<i32> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                // Look for xmp:Rating attribute on rdf:Description
                for attr in e.attributes().flatten() {
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                    if key == "xmp:Rating" {
                        let val = std::str::from_utf8(&attr.value).unwrap_or("");
                        return val.parse().ok();
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

/// Write or update the star rating in an XMP sidecar.
/// If the sidecar exists, merges (only updates xmp:Rating).
/// If it doesn't exist, creates a new one.
pub fn write_rating(image_path: &Path, rating: i32) -> Result<(), String> {
    let xmp_path = sidecar_path(image_path);

    if xmp_path.exists() {
        let content = std::fs::read_to_string(&xmp_path).map_err(|e| e.to_string())?;
        let updated = update_rating_in_xml(&content, rating)?;
        std::fs::write(&xmp_path, updated).map_err(|e| e.to_string())
    } else {
        let xml = create_xmp_with_rating(rating);
        std::fs::write(&xmp_path, xml).map_err(|e| e.to_string())
    }
}

fn create_xmp_with_rating(rating: i32) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmp:Rating="{}">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#,
        rating
    )
}

fn update_rating_in_xml(xml: &str, rating: i32) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let mut found_rating = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                let mut elem = e.clone();
                if has_rdf_description_name(e) {
                    elem = update_or_add_rating_attr(e, rating, &mut found_rating);
                }
                writer.write_event(Event::Start(elem)).map_err(|e| e.to_string())?;
            }
            Ok(Event::Empty(ref e)) => {
                let mut elem = e.clone();
                if has_rdf_description_name(e) {
                    elem = update_or_add_rating_attr(e, rating, &mut found_rating);
                }
                writer.write_event(Event::Empty(elem)).map_err(|e| e.to_string())?;
            }
            Ok(event) => {
                writer.write_event(event).map_err(|e| e.to_string())?;
            }
            Err(e) => return Err(format!("XML parse error: {}", e)),
        }
        buf.clear();
    }

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).map_err(|e| e.to_string())
}

fn has_rdf_description_name(e: &BytesStart) -> bool {
    let name = std::str::from_utf8(e.name().as_ref()).unwrap_or("");
    name == "rdf:Description"
}

fn update_or_add_rating_attr(
    e: &BytesStart,
    rating: i32,
    found: &mut bool,
) -> BytesStart<'static> {
    let mut new_elem = BytesStart::new(
        String::from_utf8_lossy(e.name().as_ref()).to_string(),
    );

    let mut has_rating = false;
    let mut has_xmp_ns = false;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        if key == "xmp:Rating" {
            new_elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
            has_rating = true;
            *found = true;
        } else {
            let val = String::from_utf8_lossy(&attr.value).to_string();
            new_elem.push_attribute((key.as_str(), val.as_str()));
            if key == "xmlns:xmp" {
                has_xmp_ns = true;
            }
        }
    }

    if !has_rating {
        if !has_xmp_ns {
            new_elem.push_attribute(("xmlns:xmp", "http://ns.adobe.com/xap/1.0/"));
        }
        new_elem.push_attribute(("xmp:Rating", rating.to_string().as_str()));
        *found = true;
    }

    new_elem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_xmp_with_rating() {
        let xml = create_xmp_with_rating(3);
        assert!(xml.contains("xmp:Rating=\"3\""));
        assert!(xml.contains("xmlns:xmp"));
        assert!(xml.contains("rdf:Description"));
    }

    #[test]
    fn test_parse_rating_from_xml() {
        let xml = create_xmp_with_rating(5);
        assert_eq!(parse_rating_from_xml(&xml), Some(5));
    }

    #[test]
    fn test_parse_rating_missing() {
        let xml = r#"<?xml version="1.0"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description/></rdf:RDF></x:xmpmeta>"#;
        assert_eq!(parse_rating_from_xml(xml), None);
    }

    #[test]
    fn test_update_rating_in_existing_xml() {
        let xml = create_xmp_with_rating(2);
        let updated = update_rating_in_xml(&xml, 4).unwrap();
        assert_eq!(parse_rating_from_xml(&updated), Some(4));
        assert!(!updated.contains("xmp:Rating=\"2\""));
    }

    #[test]
    fn test_merge_preserves_other_content() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmlns:dc="http://purl.org/dc/elements/1.1/"
      xmp:Rating="2"
      xmp:CreatorTool="DxO PhotoLab">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>"#;

        let updated = update_rating_in_xml(xml, 5).unwrap();
        assert!(updated.contains("xmp:Rating=\"5\""));
        assert!(updated.contains("DxO PhotoLab"));
        assert!(updated.contains("xmlns:dc"));
    }

    #[test]
    fn test_sidecar_path() {
        assert_eq!(
            sidecar_path(Path::new("/photos/DSC_1234.NEF")),
            PathBuf::from("/photos/DSC_1234.xmp")
        );
        assert_eq!(
            sidecar_path(Path::new("C:\\Photos\\img.jpg")),
            PathBuf::from("C:\\Photos\\img.xmp")
        );
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let img_path = dir.path().join("test.nef");
        std::fs::write(&img_path, b"fake").unwrap();

        write_rating(&img_path, 3).unwrap();
        assert_eq!(read_rating(&img_path), Some(3));

        write_rating(&img_path, 5).unwrap();
        assert_eq!(read_rating(&img_path), Some(5));
    }
}
```

- [ ] **Step 2: Run tests**

Run:
```bash
cd src-tauri && cargo test metadata::xmp::tests -- --nocapture
```

Expected: All 7 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/metadata/xmp.rs
git commit -m "feat: XMP sidecar read/write with merge support

Creates new XMP sidecars or merges ratings into existing ones,
preserving all other metadata (DxO PhotoLab compatibility)."
```

---

## Task 7: Project Scan & Open Command

**Files:**
- Modify: `src-tauri/src/commands/project.rs`
- Modify: `src-tauri/src/state.rs`

- [ ] **Step 1: Implement the project open command**

Edit `src-tauri/src/state.rs`:
```rust
use crate::db::schema::Database;
use crate::pipeline::cache::ImageCache;
use std::path::PathBuf;

pub struct AppState {
    pub db: Option<Database>,
    pub cache: ImageCache,
    pub project_folder: Option<PathBuf>,
    pub current_index: usize,
    pub image_ids: Vec<i64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            db: None,
            cache: ImageCache::new(20),
            project_folder: None,
            current_index: 0,
            image_ids: Vec::new(),
        }
    }

    pub fn current_image_id(&self) -> Option<i64> {
        self.image_ids.get(self.current_index).copied()
    }

    pub fn image_count(&self) -> usize {
        self.image_ids.len()
    }
}
```

Edit `src-tauri/src/commands/project.rs`:
```rust
use crate::db::schema::Database;
use crate::metadata::exif;
use crate::metadata::xmp;
use crate::pipeline::decoder;
use crate::pipeline::embedded;
use crate::state::AppState;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::State;

#[derive(serde::Serialize)]
pub struct ProjectInfo {
    #[serde(rename = "folderPath")]
    pub folder_path: String,
    #[serde(rename = "imageCount")]
    pub image_count: usize,
    #[serde(rename = "lastViewedIndex")]
    pub last_viewed_index: usize,
}

#[derive(serde::Serialize, Clone)]
pub struct ScanProgress {
    pub processed: usize,
    pub total: usize,
    pub phase: String,
}

#[tauri::command]
pub fn open_project(
    folder_path: String,
    state: State<'_, Mutex<AppState>>,
    app_handle: tauri::AppHandle,
) -> Result<ProjectInfo, String> {
    let folder = PathBuf::from(&folder_path);
    if !folder.is_dir() {
        return Err("Not a valid directory".into());
    }

    // Create .photosift directory
    let photosift_dir = folder.join(".photosift");
    fs::create_dir_all(&photosift_dir).map_err(|e| e.to_string())?;

    // Open/create SQLite database
    let db_path = photosift_dir.join("cache.sqlite");
    let db = Database::open(&db_path).map_err(|e| e.to_string())?;

    // Scan for supported image files
    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| embedded::is_supported_image(path))
        .collect();

    // Sort by filename for initial ordering
    files.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .cmp(b.file_name().unwrap_or_default())
    });

    let total = files.len();

    // Insert images into database and process metadata
    let mut image_ids = Vec::with_capacity(total);
    for (idx, path) in files.iter().enumerate() {
        let filepath = path.to_string_lossy().to_string();
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Skip if already in database
        if db.image_exists(&filepath).unwrap_or(false) {
            continue;
        }

        // Hash first 64KB for change detection
        let file_hash = hash_file_header(path).unwrap_or_default();
        let file_size = fs::metadata(path).map(|m| m.len() as i64).unwrap_or(0);

        let id = db
            .insert_image(&filepath, &filename, &file_hash, file_size, idx as i32)
            .map_err(|e| e.to_string())?;
        image_ids.push(id);

        // Extract EXIF metadata
        if let Ok(exif_data) = exif::extract_exif(path) {
            let _ = db.update_exif(
                id,
                exif_data.capture_time.as_deref(),
                exif_data.camera_model.as_deref(),
                exif_data.lens.as_deref(),
                exif_data.focal_length,
                exif_data.aperture,
                exif_data.shutter_speed.as_deref(),
                exif_data.iso,
                exif_data.width,
                exif_data.height,
                exif_data.orientation,
            );
        }

        // Import existing XMP sidecar ratings
        if let Some(rating) = xmp::read_rating(path) {
            let _ = db.set_star_rating(id, rating);
        }

        // Generate thumbnail
        if let Ok(thumb_bytes) = decoder::generate_thumbnail(path) {
            let _ = db.set_thumbnail(id, &thumb_bytes);
        }
    }

    // If image_ids is empty, load existing IDs from the database
    if image_ids.is_empty() {
        image_ids = db
            .get_all_images()
            .map_err(|e| e.to_string())?
            .iter()
            .map(|img| img.id)
            .collect();
    }

    let image_count = db.get_image_count().map_err(|e| e.to_string())? as usize;

    // Read last-viewed index from project.json
    let project_json_path = photosift_dir.join("project.json");
    let last_viewed = read_last_viewed(&project_json_path).unwrap_or(0);

    // Update state
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    app_state.db = Some(db);
    app_state.project_folder = Some(folder);
    app_state.image_ids = image_ids;
    app_state.current_index = last_viewed;

    Ok(ProjectInfo {
        folder_path,
        image_count,
        last_viewed_index: last_viewed,
    })
}

#[tauri::command]
pub fn get_project_info(state: State<'_, Mutex<AppState>>) -> Result<Option<ProjectInfo>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    match &app_state.project_folder {
        Some(folder) => Ok(Some(ProjectInfo {
            folder_path: folder.to_string_lossy().to_string(),
            image_count: app_state.image_count(),
            last_viewed_index: app_state.current_index,
        })),
        None => Ok(None),
    }
}

fn hash_file_header(path: &Path) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; 65536]; // 64KB
    let bytes_read = std::io::Read::read(&mut file, &mut buffer)?;
    buffer.truncate(bytes_read);

    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_last_viewed(project_json_path: &Path) -> Option<usize> {
    let content = fs::read_to_string(project_json_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get("last_viewed_index")?.as_u64().map(|v| v as usize)
}

pub fn save_last_viewed(project_folder: &Path, index: usize) {
    let project_json = project_folder.join(".photosift").join("project.json");
    let json = serde_json::json!({
        "last_viewed_index": index
    });
    let _ = fs::write(&project_json, serde_json::to_string_pretty(&json).unwrap_or_default());
}
```

- [ ] **Step 2: Run compilation check**

Run:
```bash
cd src-tauri && cargo check
```

Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/project.rs src-tauri/src/state.rs
git commit -m "feat: project open command with file scan and metadata extraction

Scans folder for NEF/JPEG files, creates .photosift/ directory,
populates SQLite with EXIF, thumbnails, and imported XMP ratings."
```

---

## Task 8: Custom Protocol Handler

**Files:**
- Modify: `src-tauri/src/pipeline/protocol.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Implement the custom protocol handler**

Edit `src-tauri/src/pipeline/protocol.rs`:
```rust
use crate::pipeline::decoder::{decode_to_jpeg, DecodeTier};
use crate::state::AppState;
use std::sync::Mutex;
use tauri::Manager;

/// Register the `photosift://` custom protocol for serving images and thumbnails.
/// URL format:
///   photosift://localhost/image/{image_id}?tier=embedded|preview|full
///   photosift://localhost/thumb/{image_id}
pub fn register_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("photosift", move |ctx, request| {
        let uri = request.uri().to_string();

        // Parse the path from the URI
        // URI looks like: photosift://localhost/image/42?tier=preview
        let path = uri
            .strip_prefix("photosift://localhost")
            .or_else(|| uri.strip_prefix("https://photosift.localhost"))
            .unwrap_or("");

        let state = ctx.state::<Mutex<AppState>>();
        let app_state = match state.lock() {
            Ok(s) => s,
            Err(_) => {
                return http::Response::builder()
                    .status(500)
                    .body(b"State lock failed".to_vec())
                    .unwrap();
            }
        };

        let db = match &app_state.db {
            Some(db) => db,
            None => {
                return http::Response::builder()
                    .status(404)
                    .body(b"No project open".to_vec())
                    .unwrap();
            }
        };

        // Route: /thumb/{id}
        if let Some(id_str) = path.strip_prefix("/thumb/") {
            let id_str = id_str.split('?').next().unwrap_or(id_str);
            if let Ok(image_id) = id_str.parse::<i64>() {
                if let Ok(Some(thumb)) = db.get_thumbnail(image_id) {
                    return http::Response::builder()
                        .status(200)
                        .header("Content-Type", "image/jpeg")
                        .header("Cache-Control", "max-age=3600")
                        .body(thumb)
                        .unwrap();
                }
            }
            return http::Response::builder()
                .status(404)
                .body(b"Thumbnail not found".to_vec())
                .unwrap();
        }

        // Route: /image/{id}?tier=...
        if let Some(rest) = path.strip_prefix("/image/") {
            let id_str = rest.split('?').next().unwrap_or(rest);
            let tier_str = uri
                .split("tier=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .unwrap_or("preview");

            let tier = match tier_str {
                "embedded" => DecodeTier::Embedded,
                "full" => DecodeTier::Full,
                _ => DecodeTier::Preview,
            };

            if let Ok(image_id) = id_str.parse::<i64>() {
                // Check LRU cache first (for preview tier)
                // Note: we can't mutate cache while holding state lock in this sync handler
                // For now, decode directly. Prefetch cache is used in the async prefetch manager.

                if let Ok(img) = db.get_image_by_id(image_id) {
                    let filepath = std::path::Path::new(&img.filepath);
                    let quality = match tier {
                        DecodeTier::Embedded => 85,
                        DecodeTier::Preview => 90,
                        DecodeTier::Full => 95,
                    };

                    match decode_to_jpeg(filepath, tier, quality) {
                        Ok(jpeg_bytes) => {
                            return http::Response::builder()
                                .status(200)
                                .header("Content-Type", "image/jpeg")
                                .header("Content-Length", jpeg_bytes.len().to_string())
                                .header("Cache-Control", "max-age=60")
                                .body(jpeg_bytes)
                                .unwrap();
                        }
                        Err(e) => {
                            log::error!("Decode failed for {}: {}", img.filepath, e);
                        }
                    }
                }
            }

            return http::Response::builder()
                .status(404)
                .body(b"Image not found".to_vec())
                .unwrap();
        }

        http::Response::builder()
            .status(404)
            .body(b"Unknown route".to_vec())
            .unwrap()
    })
}
```

- [ ] **Step 2: Wire up the protocol in lib.rs**

Edit `src-tauri/src/lib.rs`:
```rust
mod commands;
mod db;
mod metadata;
mod pipeline;
mod state;

use pipeline::protocol;
use state::AppState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let app_state = AppState::new();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(Mutex::new(app_state))
        .invoke_handler(tauri::generate_handler![
            commands::project::open_project,
            commands::project::get_project_info,
            commands::image::get_image_list,
            commands::image::get_image_metadata,
            commands::rating::set_rating,
        ]);

    let builder = protocol::register_protocol(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Update CSP to allow the custom protocol**

In `src-tauri/tauri.conf.json`, ensure the CSP includes:
```json
"csp": "default-src 'self'; img-src 'self' http://photosift.localhost https://photosift.localhost photosift:; style-src 'self' 'unsafe-inline'"
```

- [ ] **Step 4: Run compilation check**

Run:
```bash
cd src-tauri && cargo check
```

Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pipeline/protocol.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat: custom protocol handler for serving images and thumbnails

Routes photosift://localhost/image/{id}?tier= and /thumb/{id} to
the decode pipeline. Returns JPEG bytes with proper headers."
```

---

## Task 9: Image and Rating Backend Commands

**Files:**
- Modify: `src-tauri/src/commands/image.rs`
- Modify: `src-tauri/src/commands/rating.rs`

- [ ] **Step 1: Implement image commands**

Edit `src-tauri/src/commands/image.rs`:
```rust
use crate::db::schema::ImageRow;
use crate::state::AppState;
use std::sync::Mutex;
use tauri::State;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub id: i64,
    pub filepath: String,
    pub filename: String,
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
}

impl From<ImageRow> for ImageInfo {
    fn from(row: ImageRow) -> Self {
        Self {
            id: row.id,
            filepath: row.filepath,
            filename: row.filename,
            capture_time: row.capture_time,
            camera_model: row.camera_model,
            lens: row.lens,
            focal_length: row.focal_length,
            aperture: row.aperture,
            shutter_speed: row.shutter_speed,
            iso: row.iso,
            width: row.width,
            height: row.height,
            orientation: row.orientation,
            star_rating: row.star_rating,
        }
    }
}

#[tauri::command]
pub fn get_image_list(state: State<'_, Mutex<AppState>>) -> Result<Vec<ImageInfo>, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("No project open")?;
    let rows = db.get_all_images().map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(ImageInfo::from).collect())
}

#[tauri::command]
pub fn get_image_metadata(
    image_id: i64,
    state: State<'_, Mutex<AppState>>,
) -> Result<ImageInfo, String> {
    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("No project open")?;
    let row = db.get_image_by_id(image_id).map_err(|e| e.to_string())?;
    Ok(ImageInfo::from(row))
}
```

- [ ] **Step 2: Implement rating command with debounced XMP write**

Edit `src-tauri/src/commands/rating.rs`:
```rust
use crate::metadata::xmp;
use crate::state::AppState;
use std::path::Path;
use std::sync::Mutex;
use tauri::State;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingResult {
    pub image_id: i64,
    pub star_rating: i32,
}

#[tauri::command]
pub fn set_rating(
    image_id: i64,
    rating: i32,
    state: State<'_, Mutex<AppState>>,
) -> Result<RatingResult, String> {
    if !(0..=5).contains(&rating) {
        return Err("Rating must be 0-5".into());
    }

    let app_state = state.lock().map_err(|e| e.to_string())?;
    let db = app_state.db.as_ref().ok_or("No project open")?;

    // Update SQLite cache
    db.set_star_rating(image_id, rating)
        .map_err(|e| e.to_string())?;

    // Get filepath for XMP sidecar write
    let img = db.get_image_by_id(image_id).map_err(|e| e.to_string())?;
    let filepath = img.filepath.clone();

    // Drop the lock before doing I/O
    drop(app_state);

    // Write XMP sidecar (synchronous for now — debounce in frontend)
    xmp::write_rating(Path::new(&filepath), rating)?;

    Ok(RatingResult {
        image_id,
        star_rating: rating,
    })
}
```

- [ ] **Step 3: Run compilation check**

Run:
```bash
cd src-tauri && cargo check
```

Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/
git commit -m "feat: image list, metadata, and rating Tauri commands

get_image_list, get_image_metadata, set_rating commands. Rating
updates SQLite and writes XMP sidecar."
```

---

## Task 10: Zustand Store

**Files:**
- Create: `src/stores/projectStore.ts`

- [ ] **Step 1: Implement the Zustand store**

Create `src/stores/projectStore.ts`:
```typescript
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { ImageEntry, ProjectInfo } from "../types";

interface UndoEntry {
  imageId: number;
  field: "starRating";
  oldValue: number;
  newValue: number;
}

interface ProjectState {
  // Project
  projectInfo: ProjectInfo | null;
  images: ImageEntry[];
  currentIndex: number;
  isLoading: boolean;

  // UI state
  showMetadata: boolean;
  showShortcutHints: boolean;
  autoAdvance: boolean;
  isZoomed: boolean;

  // Undo/redo
  undoStack: UndoEntry[];
  redoStack: UndoEntry[];

  // Actions
  openProject: (folderPath: string) => Promise<void>;
  setCurrentIndex: (index: number) => void;
  navigateNext: () => void;
  navigatePrev: () => void;
  setRating: (rating: number) => Promise<void>;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
  toggleMetadata: () => void;
  toggleShortcutHints: () => void;
  toggleAutoAdvance: () => void;
  toggleZoom: () => void;
}

export const useProjectStore = create<ProjectState>((set, get) => ({
  projectInfo: null,
  images: [],
  currentIndex: 0,
  isLoading: false,
  showMetadata: false,
  showShortcutHints: false,
  autoAdvance: true,
  isZoomed: false,
  undoStack: [],
  redoStack: [],

  openProject: async (folderPath: string) => {
    set({ isLoading: true });
    try {
      const info = await invoke<ProjectInfo>("open_project", {
        folderPath,
      });
      const images = await invoke<ImageEntry[]>("get_image_list");
      set({
        projectInfo: info,
        images,
        currentIndex: info.lastViewedIndex,
        isLoading: false,
        undoStack: [],
        redoStack: [],
      });
    } catch (e) {
      console.error("Failed to open project:", e);
      set({ isLoading: false });
    }
  },

  setCurrentIndex: (index: number) => {
    const { images } = get();
    if (index >= 0 && index < images.length) {
      set({ currentIndex: index, isZoomed: false });
    }
  },

  navigateNext: () => {
    const { currentIndex, images } = get();
    if (currentIndex < images.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
    }
  },

  navigatePrev: () => {
    const { currentIndex } = get();
    if (currentIndex > 0) {
      set({ currentIndex: currentIndex - 1, isZoomed: false });
    }
  },

  setRating: async (rating: number) => {
    const { images, currentIndex, autoAdvance, undoStack } = get();
    const image = images[currentIndex];
    if (!image) return;

    const oldRating = image.starRating;
    if (oldRating === rating) return;

    // Optimistic update
    const updatedImages = [...images];
    updatedImages[currentIndex] = { ...image, starRating: rating };
    set({
      images: updatedImages,
      undoStack: [
        ...undoStack.slice(-49),
        { imageId: image.id, field: "starRating", oldValue: oldRating, newValue: rating },
      ],
      redoStack: [],
    });

    // Auto-advance
    if (autoAdvance && currentIndex < images.length - 1) {
      set({ currentIndex: currentIndex + 1, isZoomed: false });
    }

    // Send to backend (fire-and-forget, already optimistically updated)
    try {
      await invoke("set_rating", { imageId: image.id, rating });
    } catch (e) {
      console.error("Failed to set rating:", e);
      // Revert on failure
      const revertImages = [...get().images];
      const idx = revertImages.findIndex((img) => img.id === image.id);
      if (idx >= 0) {
        revertImages[idx] = { ...revertImages[idx], starRating: oldRating };
        set({ images: revertImages });
      }
    }
  },

  undo: async () => {
    const { undoStack, redoStack, images } = get();
    const entry = undoStack[undoStack.length - 1];
    if (!entry) return;

    const idx = images.findIndex((img) => img.id === entry.imageId);
    if (idx < 0) return;

    const updatedImages = [...images];
    updatedImages[idx] = { ...updatedImages[idx], starRating: entry.oldValue };

    set({
      images: updatedImages,
      undoStack: undoStack.slice(0, -1),
      redoStack: [...redoStack, entry],
      currentIndex: idx,
    });

    try {
      await invoke("set_rating", { imageId: entry.imageId, rating: entry.oldValue });
    } catch (e) {
      console.error("Undo failed:", e);
    }
  },

  redo: async () => {
    const { redoStack, undoStack, images } = get();
    const entry = redoStack[redoStack.length - 1];
    if (!entry) return;

    const idx = images.findIndex((img) => img.id === entry.imageId);
    if (idx < 0) return;

    const updatedImages = [...images];
    updatedImages[idx] = { ...updatedImages[idx], starRating: entry.newValue };

    set({
      images: updatedImages,
      redoStack: redoStack.slice(0, -1),
      undoStack: [...undoStack, entry],
      currentIndex: idx,
    });

    try {
      await invoke("set_rating", { imageId: entry.imageId, rating: entry.newValue });
    } catch (e) {
      console.error("Redo failed:", e);
    }
  },

  toggleMetadata: () => set((s) => ({ showMetadata: !s.showMetadata })),
  toggleShortcutHints: () => set((s) => ({ showShortcutHints: !s.showShortcutHints })),
  toggleAutoAdvance: () => set((s) => ({ autoAdvance: !s.autoAdvance })),
  toggleZoom: () => set((s) => ({ isZoomed: !s.isZoomed })),
}));
```

- [ ] **Step 2: Update TypeScript types to match backend**

Edit `src/types/index.ts`:
```typescript
export interface ImageEntry {
  id: number;
  filepath: string;
  filename: string;
  captureTime: string | null;
  cameraModel: string | null;
  lens: string | null;
  focalLength: number | null;
  aperture: number | null;
  shutterSpeed: string | null;
  iso: number | null;
  width: number | null;
  height: number | null;
  orientation: number | null;
  starRating: number;
}

export interface ProjectInfo {
  folderPath: string;
  imageCount: number;
  lastViewedIndex: number;
}
```

- [ ] **Step 3: Commit**

```bash
git add src/stores/ src/types/
git commit -m "feat: Zustand store with image navigation, ratings, undo/redo

Manages project state, optimistic rating updates, 50-deep undo stack,
auto-advance, and UI toggle state."
```

---

## Task 11: Keyboard Navigation Hook

**Files:**
- Create: `src/hooks/useKeyboardNav.ts`

- [ ] **Step 1: Implement the keyboard handler**

Create `src/hooks/useKeyboardNav.ts`:
```typescript
import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";

export function useKeyboardNav() {
  const {
    navigateNext,
    navigatePrev,
    setRating,
    undo,
    redo,
    toggleMetadata,
    toggleShortcutHints,
    toggleZoom,
    images,
  } = useProjectStore();

  useEffect(() => {
    if (images.length === 0) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't handle if user is typing in an input
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

      // Ctrl+Z / Ctrl+Shift+Z
      if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
        return;
      }
      if (e.ctrlKey && e.key === "Z") {
        e.preventDefault();
        redo();
        return;
      }
      if (e.ctrlKey && e.shiftKey && e.key === "z") {
        e.preventDefault();
        redo();
        return;
      }

      // Navigation
      switch (e.key) {
        case "ArrowRight":
        case "ArrowDown":
          e.preventDefault();
          navigateNext();
          break;
        case "ArrowLeft":
        case "ArrowUp":
          e.preventDefault();
          navigatePrev();
          break;

        // Star ratings
        case "1":
          setRating(1);
          break;
        case "2":
          setRating(2);
          break;
        case "3":
          setRating(3);
          break;
        case "4":
          setRating(4);
          break;
        case "5":
          setRating(5);
          break;
        case "0":
          setRating(0);
          break;

        // Toggle views
        case " ": // Spacebar
          e.preventDefault();
          toggleZoom();
          break;
        case "i":
        case "I":
          toggleMetadata();
          break;
        case "?":
          toggleShortcutHints();
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    images.length,
    navigateNext,
    navigatePrev,
    setRating,
    undo,
    redo,
    toggleMetadata,
    toggleShortcutHints,
    toggleZoom,
  ]);
}
```

- [ ] **Step 2: Commit**

```bash
git add src/hooks/useKeyboardNav.ts
git commit -m "feat: keyboard navigation hook

Arrow keys for navigation, 0-5 for ratings, spacebar zoom,
I for metadata, ? for shortcut hints, Ctrl+Z/Y for undo/redo."
```

---

## Task 12: LoupeView Component

**Files:**
- Create: `src/components/LoupeView.tsx`
- Create: `src/hooks/useImageLoader.ts`

- [ ] **Step 1: Implement the image loader hook**

Create `src/hooks/useImageLoader.ts`:
```typescript
import { useState, useEffect, useRef } from "react";

const PROTOCOL_BASE = "http://photosift.localhost";

export function imageUrl(imageId: number, tier: "embedded" | "preview" | "full"): string {
  return `${PROTOCOL_BASE}/image/${imageId}?tier=${tier}`;
}

export function thumbUrl(imageId: number): string {
  return `${PROTOCOL_BASE}/thumb/${imageId}`;
}

/**
 * Manages the two-image swap strategy for zero-flash navigation.
 * Returns the current display URL and a loading state.
 */
export function useImageLoader(imageId: number | null) {
  const [displayUrl, setDisplayUrl] = useState<string | null>(null);
  const [isUpgrading, setIsUpgrading] = useState(false);
  const prevIdRef = useRef<number | null>(null);

  useEffect(() => {
    if (imageId === null) {
      setDisplayUrl(null);
      return;
    }

    // If same image, don't reload
    if (imageId === prevIdRef.current) return;
    prevIdRef.current = imageId;

    // Step 1: Show embedded JPEG immediately
    const embeddedSrc = imageUrl(imageId, "embedded");
    setDisplayUrl(embeddedSrc);
    setIsUpgrading(true);

    // Step 2: Preload preview tier, swap when ready
    const previewImg = new Image();
    previewImg.onload = () => {
      // Only swap if we're still on the same image
      if (prevIdRef.current === imageId) {
        setDisplayUrl(previewImg.src);
        setIsUpgrading(false);
      }
    };
    previewImg.onerror = () => {
      setIsUpgrading(false);
    };
    previewImg.src = imageUrl(imageId, "preview");

    return () => {
      // Cancel the preview load if we navigate away
      previewImg.onload = null;
      previewImg.onerror = null;
    };
  }, [imageId]);

  return { displayUrl, isUpgrading };
}
```

- [ ] **Step 2: Implement the LoupeView component**

Create `src/components/LoupeView.tsx`:
```typescript
import { useRef, useState, useCallback } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useImageLoader, imageUrl } from "../hooks/useImageLoader";

export function LoupeView() {
  const { images, currentIndex, isZoomed, toggleZoom } = useProjectStore();
  const currentImage = images[currentIndex] ?? null;
  const { displayUrl, isUpgrading } = useImageLoader(currentImage?.id ?? null);

  const containerRef = useRef<HTMLDivElement>(null);
  const [zoomOrigin, setZoomOrigin] = useState({ x: 50, y: 50 });
  const [panOffset, setPanOffset] = useState({ x: 0, y: 0 });
  const isDragging = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!isZoomed) return;
      isDragging.current = true;
      dragStart.current = { x: e.clientX - panOffset.x, y: e.clientY - panOffset.y };
    },
    [isZoomed, panOffset],
  );

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (!isDragging.current) return;
      setPanOffset({
        x: e.clientX - dragStart.current.x,
        y: e.clientY - dragStart.current.y,
      });
    },
    [],
  );

  const handleMouseUp = useCallback(() => {
    isDragging.current = false;
  }, []);

  const handleDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      if (!containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const x = ((e.clientX - rect.left) / rect.width) * 100;
      const y = ((e.clientY - rect.top) / rect.height) * 100;
      setZoomOrigin({ x, y });
      setPanOffset({ x: 0, y: 0 });
      toggleZoom();
    },
    [toggleZoom],
  );

  if (!currentImage || !displayUrl) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[var(--bg-primary)]">
        <p className="text-[var(--text-secondary)]">No image selected</p>
      </div>
    );
  }

  const imgStyle: React.CSSProperties = isZoomed
    ? {
        transform: `scale(3) translate(${panOffset.x / 3}px, ${panOffset.y / 3}px)`,
        transformOrigin: `${zoomOrigin.x}% ${zoomOrigin.y}%`,
        cursor: "grab",
      }
    : {
        transform: "scale(1)",
        transformOrigin: "center center",
        cursor: "default",
      };

  return (
    <div
      ref={containerRef}
      className="flex-1 relative overflow-hidden bg-[var(--bg-primary)]"
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onDoubleClick={handleDoubleClick}
    >
      <img
        src={displayUrl}
        alt={currentImage.filename}
        className="w-full h-full object-contain transition-transform duration-100"
        style={imgStyle}
        draggable={false}
      />
      {isUpgrading && (
        <div className="absolute top-2 left-2 px-2 py-1 rounded bg-black/50 text-xs text-white/60">
          Loading...
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/LoupeView.tsx src/hooks/useImageLoader.ts
git commit -m "feat: LoupeView with two-tier image loading and zoom

Loads embedded JPEG instantly, swaps to decoded preview when ready.
Double-click or spacebar for 3x zoom with mouse pan."
```

---

## Task 13: Filmstrip Component

**Files:**
- Create: `src/components/Filmstrip.tsx`

- [ ] **Step 1: Implement the virtualized filmstrip**

Create `src/components/Filmstrip.tsx`:
```typescript
import { useRef, useEffect, useCallback } from "react";
import { FixedSizeList as List } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";

const THUMB_WIDTH = 100;
const THUMB_HEIGHT = 80;
const FILMSTRIP_HEIGHT = THUMB_HEIGHT + 8; // padding

export function Filmstrip() {
  const { images, currentIndex, setCurrentIndex } = useProjectStore();
  const listRef = useRef<List>(null);

  // Auto-scroll to keep current image centered
  useEffect(() => {
    if (listRef.current && images.length > 0) {
      listRef.current.scrollToItem(currentIndex, "center");
    }
  }, [currentIndex, images.length]);

  const ThumbnailItem = useCallback(
    ({ index, style }: { index: number; style: React.CSSProperties }) => {
      const image = images[index];
      if (!image) return null;

      const isCurrent = index === currentIndex;

      return (
        <div
          style={style}
          className="flex items-center justify-center p-1"
          onClick={() => setCurrentIndex(index)}
        >
          <div
            className={`relative cursor-pointer rounded overflow-hidden transition-all ${
              isCurrent
                ? "ring-2 ring-[var(--accent)] brightness-100"
                : "brightness-75 hover:brightness-90"
            }`}
            style={{ width: THUMB_WIDTH - 8, height: THUMB_HEIGHT - 8 }}
          >
            <img
              src={thumbUrl(image.id)}
              alt={image.filename}
              className="w-full h-full object-cover"
              loading="lazy"
              draggable={false}
            />
            {/* Star rating indicator */}
            {image.starRating > 0 && (
              <div className="absolute bottom-0 left-0 right-0 flex justify-center gap-0.5 pb-0.5 bg-gradient-to-t from-black/60 to-transparent">
                {Array.from({ length: image.starRating }, (_, i) => (
                  <div
                    key={i}
                    className="w-1.5 h-1.5 rounded-full bg-[var(--star-filled)]"
                  />
                ))}
              </div>
            )}
          </div>
        </div>
      );
    },
    [images, currentIndex, setCurrentIndex],
  );

  if (images.length === 0) return null;

  return (
    <div
      className="bg-[var(--bg-secondary)] border-t border-white/10"
      style={{ height: FILMSTRIP_HEIGHT }}
    >
      <List
        ref={listRef}
        height={FILMSTRIP_HEIGHT}
        width={window.innerWidth}
        itemCount={images.length}
        itemSize={THUMB_WIDTH}
        layout="horizontal"
        overscanCount={10}
      >
        {ThumbnailItem}
      </List>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add src/components/Filmstrip.tsx
git commit -m "feat: virtualized filmstrip with thumbnails and rating dots

Horizontal scrollable filmstrip using react-window. Auto-centers on
current image. Shows star rating dots overlay."
```

---

## Task 14: Toolbar, RatingBar, MetadataOverlay, ShortcutHints

**Files:**
- Create: `src/components/Toolbar.tsx`, `src/components/RatingBar.tsx`, `src/components/MetadataOverlay.tsx`, `src/components/ShortcutHints.tsx`

- [ ] **Step 1: Implement Toolbar**

Create `src/components/Toolbar.tsx`:
```typescript
import { useProjectStore } from "../stores/projectStore";

export function Toolbar() {
  const { projectInfo, images, currentIndex, autoAdvance, toggleAutoAdvance } =
    useProjectStore();

  if (!projectInfo) return null;

  const folderName = projectInfo.folderPath.split(/[/\\]/).pop() || "Unknown";

  return (
    <div className="flex items-center justify-between px-4 py-2 bg-[var(--bg-secondary)] border-b border-white/10 text-sm">
      <div className="flex items-center gap-4">
        <span className="font-medium text-[var(--text-primary)]">
          {folderName}
        </span>
        <span className="text-[var(--text-secondary)]">
          {images.length > 0
            ? `${currentIndex + 1} / ${images.length}`
            : "No images"}
        </span>
      </div>
      <div className="flex items-center gap-3">
        <button
          onClick={toggleAutoAdvance}
          className={`px-2 py-1 rounded text-xs transition-colors ${
            autoAdvance
              ? "bg-[var(--accent)] text-white"
              : "bg-[var(--bg-tertiary)] text-[var(--text-secondary)]"
          }`}
        >
          Auto-advance {autoAdvance ? "ON" : "OFF"}
        </button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Implement RatingBar**

Create `src/components/RatingBar.tsx`:
```typescript
import { useProjectStore } from "../stores/projectStore";

export function RatingBar() {
  const { images, currentIndex, setRating } = useProjectStore();
  const image = images[currentIndex];
  if (!image) return null;

  return (
    <div className="flex items-center justify-center gap-4 py-2 bg-[var(--bg-secondary)] border-t border-white/10">
      <div className="flex items-center gap-1">
        {[1, 2, 3, 4, 5].map((star) => (
          <button
            key={star}
            onClick={() => setRating(image.starRating === star ? 0 : star)}
            className="text-lg transition-colors hover:scale-110"
            style={{
              color:
                star <= image.starRating
                  ? "var(--star-filled)"
                  : "var(--star-empty)",
            }}
          >
            ★
          </button>
        ))}
      </div>
      <span className="text-xs text-[var(--text-secondary)]">
        {image.filename}
      </span>
    </div>
  );
}
```

- [ ] **Step 3: Implement MetadataOverlay**

Create `src/components/MetadataOverlay.tsx`:
```typescript
import { useProjectStore } from "../stores/projectStore";

export function MetadataOverlay() {
  const { images, currentIndex, showMetadata } = useProjectStore();
  if (!showMetadata) return null;

  const image = images[currentIndex];
  if (!image) return null;

  const lines = [
    image.filename,
    image.captureTime,
    [
      image.focalLength ? `${image.focalLength}mm` : null,
      image.aperture ? `f/${image.aperture}` : null,
      image.shutterSpeed,
      image.iso ? `ISO ${image.iso}` : null,
    ]
      .filter(Boolean)
      .join("  ·  ") || null,
    image.cameraModel,
    image.lens,
    image.width && image.height ? `${image.width} × ${image.height}` : null,
  ].filter(Boolean);

  return (
    <div className="absolute top-2 right-2 px-3 py-2 rounded bg-black/70 text-xs text-white/80 space-y-1 pointer-events-none">
      {lines.map((line, i) => (
        <div key={i}>{line}</div>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Implement ShortcutHints**

Create `src/components/ShortcutHints.tsx`:
```typescript
import { useProjectStore } from "../stores/projectStore";

const SHORTCUTS = [
  { key: "← →", action: "Previous / Next image" },
  { key: "1-5", action: "Set star rating" },
  { key: "0", action: "Clear rating" },
  { key: "Space", action: "Toggle zoom" },
  { key: "I", action: "Toggle metadata" },
  { key: "?", action: "Toggle this overlay" },
  { key: "Ctrl+Z", action: "Undo" },
  { key: "Ctrl+Shift+Z", action: "Redo" },
];

export function ShortcutHints() {
  const { showShortcutHints, toggleShortcutHints } = useProjectStore();
  if (!showShortcutHints) return null;

  return (
    <div
      className="absolute inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={toggleShortcutHints}
    >
      <div className="bg-[var(--bg-secondary)] rounded-lg p-6 shadow-2xl border border-white/10 min-w-80">
        <h2 className="text-lg font-medium text-[var(--text-primary)] mb-4">
          Keyboard Shortcuts
        </h2>
        <div className="space-y-2">
          {SHORTCUTS.map(({ key, action }) => (
            <div key={key} className="flex justify-between gap-8 text-sm">
              <kbd className="px-2 py-0.5 rounded bg-[var(--bg-tertiary)] text-[var(--text-primary)] font-mono text-xs">
                {key}
              </kbd>
              <span className="text-[var(--text-secondary)]">{action}</span>
            </div>
          ))}
        </div>
        <p className="mt-4 text-xs text-[var(--text-secondary)] text-center">
          Press ? or click to close
        </p>
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Commit**

```bash
git add src/components/Toolbar.tsx src/components/RatingBar.tsx src/components/MetadataOverlay.tsx src/components/ShortcutHints.tsx
git commit -m "feat: toolbar, rating bar, metadata overlay, and shortcut hints

Toolbar shows project name and image counter. RatingBar with clickable
stars. MetadataOverlay shows EXIF. ShortcutHints modal on ? key."
```

---

## Task 15: Wire Up App.tsx — Complete UI Assembly

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Assemble all components into the main App**

Edit `src/App.tsx`:
```typescript
import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "./stores/projectStore";
import { useKeyboardNav } from "./hooks/useKeyboardNav";
import { LoupeView } from "./components/LoupeView";
import { Filmstrip } from "./components/Filmstrip";
import { Toolbar } from "./components/Toolbar";
import { RatingBar } from "./components/RatingBar";
import { MetadataOverlay } from "./components/MetadataOverlay";
import { ShortcutHints } from "./components/ShortcutHints";

function App() {
  const { projectInfo, isLoading, openProject } = useProjectStore();
  useKeyboardNav();

  const handleOpen = useCallback(async () => {
    const selected = await open({ directory: true });
    if (selected) {
      await openProject(selected);
    }
  }, [openProject]);

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      const items = e.dataTransfer.items;
      if (items.length > 0) {
        const item = items[0];
        const entry = item.webkitGetAsEntry?.();
        if (entry?.isDirectory) {
          // Tauri drag-and-drop gives us the path in the file list
          const files = e.dataTransfer.files;
          if (files.length > 0) {
            // The path comes from the dropped folder
            const path = (files[0] as any).path;
            if (path) {
              await openProject(path);
            }
          }
        }
      }
    },
    [openProject],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
  }, []);

  // Welcome screen
  if (!projectInfo) {
    return (
      <div
        className="h-screen w-screen flex flex-col items-center justify-center bg-[var(--bg-primary)]"
        onDrop={handleDrop}
        onDragOver={handleDragOver}
      >
        <h1 className="text-3xl font-light text-[var(--text-primary)] mb-2">
          PhotoSift
        </h1>
        <p className="text-[var(--text-secondary)] mb-6">
          Fast photo culling for photographers
        </p>
        <button
          onClick={handleOpen}
          disabled={isLoading}
          className="px-6 py-3 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors disabled:opacity-50"
        >
          {isLoading ? "Opening..." : "Open Folder"}
        </button>
        <p className="mt-4 text-xs text-[var(--text-secondary)]">
          or drag a folder here · Ctrl+O
        </p>
      </div>
    );
  }

  // Main culling view
  return (
    <div
      className="h-screen w-screen flex flex-col bg-[var(--bg-primary)]"
      onDrop={handleDrop}
      onDragOver={handleDragOver}
    >
      <Toolbar />
      <div className="flex-1 relative overflow-hidden">
        <LoupeView />
        <MetadataOverlay />
        <ShortcutHints />
      </div>
      <Filmstrip />
      <RatingBar />
    </div>
  );
}

export default App;
```

- [ ] **Step 2: Add Ctrl+O handler to keyboard nav**

Edit `src/hooks/useKeyboardNav.ts` — add this case inside the `handleKeyDown` function, before the existing Ctrl+Z check:

```typescript
      // Ctrl+O to open folder
      if (e.ctrlKey && e.key === "o") {
        e.preventDefault();
        // Dispatch a custom event that App.tsx listens to
        window.dispatchEvent(new CustomEvent("photosift:open-folder"));
        return;
      }
```

Then in `src/App.tsx`, add a listener in the component body:

```typescript
  // Listen for Ctrl+O from keyboard handler
  useEffect(() => {
    const handler = () => handleOpen();
    window.addEventListener("photosift:open-folder", handler);
    return () => window.removeEventListener("photosift:open-folder", handler);
  }, [handleOpen]);
```

Add the `useEffect` import at the top of `App.tsx`:
```typescript
import { useCallback, useEffect } from "react";
```

- [ ] **Step 3: Run the app**

Run:
```bash
npm run tauri dev
```

Expected: The app opens showing the welcome screen with "PhotoSift" title, "Open Folder" button, and drag-drop hint. Clicking "Open Folder" opens a native folder picker. After selecting a folder with images, the Loupe View, Filmstrip, Toolbar, and RatingBar all appear. Arrow keys navigate, 1-5 set ratings, ? shows shortcuts.

- [ ] **Step 4: Commit**

```bash
git add src/App.tsx src/hooks/useKeyboardNav.ts
git commit -m "feat: complete UI assembly with welcome screen and culling view

Welcome screen with open/drag-drop. Main view assembles toolbar,
loupe, metadata overlay, filmstrip, and rating bar. Ctrl+O support."
```

---

## Task 16: Prefetch Manager

**Files:**
- Modify: `src-tauri/src/pipeline/prefetch.rs`
- Modify: `src-tauri/src/pipeline/cache.rs`

- [ ] **Step 1: Enhance the cache with thread-safe access**

Edit `src-tauri/src/pipeline/cache.rs`:
```rust
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

    pub fn contains(&self, image_id: i64) -> bool {
        self.inner
            .lock()
            .ok()
            .map(|mut c| c.get(&image_id).is_some())
            .unwrap_or(false)
    }
}
```

- [ ] **Step 2: Implement the prefetch manager**

Edit `src-tauri/src/pipeline/prefetch.rs`:
```rust
use crate::pipeline::cache::ImageCache;
use crate::pipeline::decoder::{decode_to_jpeg, DecodeTier};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

/// Manages background prefetching of images around the current position.
pub struct PrefetchManager {
    cache: ImageCache,
    window_size: usize,
    /// Map of image_id -> filepath for decoding
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

    /// Update the list of known images (called after project open).
    pub fn set_images(&self, images: Vec<(i64, PathBuf)>) {
        if let Ok(mut paths) = self.image_paths.lock() {
            *paths = images;
        }
    }

    /// Trigger prefetch around the given index.
    /// Direction: positive = navigating forward, negative = backward.
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

            // Build prefetch order: direction-biased
            let mut indices = Vec::new();

            // Add forward indices first if navigating forward, backward first otherwise
            let (primary_range, secondary_range) = if direction >= 0 {
                (
                    (1..=window as i64).collect::<Vec<_>>(),
                    (1..=window as i64).map(|i| -i).collect::<Vec<_>>(),
                )
            } else {
                (
                    (1..=window as i64).map(|i| -i).collect::<Vec<_>>(),
                    (1..=window as i64).collect::<Vec<_>>(),
                )
            };

            for offset in primary_range.iter().chain(secondary_range.iter()) {
                let idx = current_index as i64 + offset;
                if idx >= 0 && (idx as usize) < paths.len() {
                    indices.push(idx as usize);
                }
            }

            // Decode and cache each image
            for idx in indices {
                let (image_id, ref filepath) = paths[idx];
                if cache.contains(image_id) {
                    continue;
                }

                match decode_to_jpeg(filepath, DecodeTier::Preview, 90) {
                    Ok(jpeg_bytes) => {
                        cache.put(image_id, jpeg_bytes);
                    }
                    Err(e) => {
                        log::warn!("Prefetch decode failed for {:?}: {}", filepath, e);
                    }
                }
            }
        });
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/pipeline/cache.rs src-tauri/src/pipeline/prefetch.rs
git commit -m "feat: thread-safe LRU cache and direction-aware prefetch manager

Background prefetch of +/-5 images with navigation direction bias.
Thread-safe Arc<Mutex> LRU cache for decoded preview JPEGs."
```

---

## Task 17: Wire Prefetch into Protocol Handler

**Files:**
- Modify: `src-tauri/src/pipeline/protocol.rs`
- Modify: `src-tauri/src/state.rs`

- [ ] **Step 1: Add prefetch manager to AppState**

Edit `src-tauri/src/state.rs`:
```rust
use crate::db::schema::Database;
use crate::pipeline::cache::ImageCache;
use crate::pipeline::prefetch::PrefetchManager;
use std::path::PathBuf;

pub struct AppState {
    pub db: Option<Database>,
    pub cache: ImageCache,
    pub prefetch: PrefetchManager,
    pub project_folder: Option<PathBuf>,
    pub current_index: usize,
    pub image_ids: Vec<i64>,
}

impl AppState {
    pub fn new() -> Self {
        let cache = ImageCache::new(20);
        let prefetch = PrefetchManager::new(cache.clone(), 5);
        Self {
            db: None,
            cache,
            prefetch,
            project_folder: None,
            current_index: 0,
            image_ids: Vec::new(),
        }
    }

    pub fn current_image_id(&self) -> Option<i64> {
        self.image_ids.get(self.current_index).copied()
    }

    pub fn image_count(&self) -> usize {
        self.image_ids.len()
    }
}
```

- [ ] **Step 2: Update protocol handler to use cache and trigger prefetch**

Edit `src-tauri/src/pipeline/protocol.rs`:
```rust
use crate::pipeline::decoder::{decode_to_jpeg, DecodeTier};
use crate::state::AppState;
use std::sync::Mutex;
use tauri::Manager;

pub fn register_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_uri_scheme_protocol("photosift", move |ctx, request| {
        let uri = request.uri().to_string();

        let path = uri
            .strip_prefix("photosift://localhost")
            .or_else(|| uri.strip_prefix("https://photosift.localhost"))
            .unwrap_or("");

        let state = ctx.state::<Mutex<AppState>>();
        let app_state = match state.lock() {
            Ok(s) => s,
            Err(_) => {
                return http::Response::builder()
                    .status(500)
                    .body(b"State lock failed".to_vec())
                    .unwrap();
            }
        };

        let db = match &app_state.db {
            Some(db) => db,
            None => {
                return http::Response::builder()
                    .status(404)
                    .body(b"No project open".to_vec())
                    .unwrap();
            }
        };

        // Route: /thumb/{id}
        if let Some(id_str) = path.strip_prefix("/thumb/") {
            let id_str = id_str.split('?').next().unwrap_or(id_str);
            if let Ok(image_id) = id_str.parse::<i64>() {
                if let Ok(Some(thumb)) = db.get_thumbnail(image_id) {
                    return http::Response::builder()
                        .status(200)
                        .header("Content-Type", "image/jpeg")
                        .header("Cache-Control", "max-age=3600")
                        .body(thumb)
                        .unwrap();
                }
            }
            return http::Response::builder()
                .status(404)
                .body(b"Thumbnail not found".to_vec())
                .unwrap();
        }

        // Route: /image/{id}?tier=...
        if let Some(rest) = path.strip_prefix("/image/") {
            let id_str = rest.split('?').next().unwrap_or(rest);
            let tier_str = uri
                .split("tier=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .unwrap_or("preview");

            let tier = match tier_str {
                "embedded" => DecodeTier::Embedded,
                "full" => DecodeTier::Full,
                _ => DecodeTier::Preview,
            };

            if let Ok(image_id) = id_str.parse::<i64>() {
                // Check LRU cache first for preview tier
                if tier == DecodeTier::Preview {
                    if let Some(cached) = app_state.cache.get(image_id) {
                        // Trigger prefetch for surrounding images
                        let idx = app_state
                            .image_ids
                            .iter()
                            .position(|&id| id == image_id)
                            .unwrap_or(0);
                        let direction = if idx >= app_state.current_index { 1 } else { -1 };
                        app_state.prefetch.prefetch_around(idx, direction);

                        return http::Response::builder()
                            .status(200)
                            .header("Content-Type", "image/jpeg")
                            .header("Content-Length", cached.len().to_string())
                            .header("X-Cache", "hit")
                            .body(cached)
                            .unwrap();
                    }
                }

                if let Ok(img) = db.get_image_by_id(image_id) {
                    let filepath = std::path::Path::new(&img.filepath);
                    let quality = match tier {
                        DecodeTier::Embedded => 85,
                        DecodeTier::Preview => 90,
                        DecodeTier::Full => 95,
                    };

                    match decode_to_jpeg(filepath, tier, quality) {
                        Ok(jpeg_bytes) => {
                            // Cache preview tier results
                            if tier == DecodeTier::Preview {
                                app_state.cache.put(image_id, jpeg_bytes.clone());

                                // Trigger prefetch
                                let idx = app_state
                                    .image_ids
                                    .iter()
                                    .position(|&id| id == image_id)
                                    .unwrap_or(0);
                                app_state.prefetch.prefetch_around(idx, 1);
                            }

                            return http::Response::builder()
                                .status(200)
                                .header("Content-Type", "image/jpeg")
                                .header("Content-Length", jpeg_bytes.len().to_string())
                                .header("X-Cache", "miss")
                                .body(jpeg_bytes)
                                .unwrap();
                        }
                        Err(e) => {
                            log::error!("Decode failed for {}: {}", img.filepath, e);
                        }
                    }
                }
            }

            return http::Response::builder()
                .status(404)
                .body(b"Image not found".to_vec())
                .unwrap();
        }

        http::Response::builder()
            .status(404)
            .body(b"Unknown route".to_vec())
            .unwrap()
    })
}
```

- [ ] **Step 3: Initialize prefetch images after project open**

Add to the end of `open_project` in `src-tauri/src/commands/project.rs`, just before the final `Ok(...)`:

```rust
    // Initialize prefetch manager with image paths
    let prefetch_images: Vec<(i64, std::path::PathBuf)> = app_state
        .db
        .as_ref()
        .unwrap()
        .get_all_images()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|img| (img.id, std::path::PathBuf::from(&img.filepath)))
        .collect();
    app_state.prefetch.set_images(prefetch_images);
```

- [ ] **Step 4: Run compilation check**

Run:
```bash
cd src-tauri && cargo check
```

Expected: Compiles.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pipeline/protocol.rs src-tauri/src/state.rs src-tauri/src/commands/project.rs
git commit -m "feat: wire prefetch cache into protocol handler

Protocol handler checks LRU cache before decoding. Cache miss triggers
background prefetch of surrounding images. Prefetch initialized on
project open."
```

---

## Task 18: End-to-End Test & Polish

**Files:**
- Modify: various files for bug fixes discovered during testing

- [ ] **Step 1: Build and run the full application**

Run:
```bash
npm run tauri dev
```

- [ ] **Step 2: Manual test — Project open**

1. Click "Open Folder" and select a folder with NEF files
2. Verify: filmstrip populates with thumbnails
3. Verify: first image appears in the loupe view
4. Verify: toolbar shows folder name and "1 / N" counter

- [ ] **Step 3: Manual test — Navigation**

1. Press Right Arrow repeatedly — images advance
2. Press Left Arrow — go back
3. Hold Right Arrow — rapid navigation
4. Verify: no blank flashes between images
5. Verify: image counter updates in toolbar

- [ ] **Step 4: Manual test — Ratings**

1. Press `3` — star rating appears (three gold dots on filmstrip thumbnail)
2. Press `5` — rating changes to 5
3. Press `0` — rating clears
4. Verify: `.xmp` sidecar file created next to the image
5. Press `Ctrl+Z` — last rating undone
6. Press `Ctrl+Shift+Z` — redo

- [ ] **Step 5: Manual test — UI features**

1. Press `I` — metadata overlay appears (EXIF info)
2. Press `I` again — overlay hides
3. Press `?` — shortcut hints modal appears
4. Press `?` or click — modal closes
5. Double-click image — zoom in
6. Drag while zoomed — pan works
7. Double-click again — zoom out

- [ ] **Step 6: Manual test — Filmstrip**

1. Click a thumbnail in the filmstrip — loupe jumps to that image
2. Navigate with arrows — filmstrip auto-scrolls
3. Star rating dots visible on rated thumbnails

- [ ] **Step 7: Manual test — Re-open**

1. Close the app
2. Re-open and select the same folder
3. Verify: ratings are preserved
4. Verify: resumes at last viewed image

- [ ] **Step 8: Commit any fixes**

```bash
git add -A
git commit -m "fix: end-to-end testing fixes and polish

Addressed issues discovered during manual testing of the complete
Phase 1 culling workflow."
```

---

## Summary

**17 implementation tasks + 1 testing task.** The build order ensures each task produces compilable, testable code:

1. **Scaffolding** — Tauri + React project structure
2. **SQLite** — Data layer with full test coverage
3. **Embedded JPEG** — NEF preview extraction
4. **Decoder** — Three-tier RAW/JPEG decode pipeline
5. **EXIF** — Metadata extraction
6. **XMP** — Sidecar read/write/merge with tests
7. **Project scan** — File discovery and metadata population
8. **Custom protocol** — Image serving to webview
9. **Backend commands** — Image list, metadata, rating
10. **Zustand store** — Frontend state with optimistic updates and undo
11. **Keyboard nav** — Key handler hook
12. **LoupeView** — Main image display with two-img swap
13. **Filmstrip** — Virtualized thumbnail strip
14. **UI components** — Toolbar, rating bar, metadata, shortcuts
15. **App assembly** — Wire everything together
16. **Prefetch** — Background image cache
17. **Protocol + prefetch** — Wire cache into serving layer
18. **E2E testing** — Full manual test pass
