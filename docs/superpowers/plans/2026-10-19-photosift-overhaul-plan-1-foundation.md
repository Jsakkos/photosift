# PhotoSift Overhaul — Plan 1: Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Tauri prototype with a Rust localhost server + browser UI that imports an SD card into consistently named event folders, enriches every photo on the GPU, and lets the user review by moments and set one outcome per photo — written to XMP for Lightroom Classic.

**Architecture:** One Rust binary (`server/`, axum) owns ingest, enrichment, SQLite and XMP, and serves a React app (`web/`) built to static files. The browser talks JSON over HTTP and receives live progress over one WebSocket. Every operation the UI uses is an HTTP endpoint, so Plan 2's MCP endpoint and the e2e tests drive the same surface.

**Tech Stack:** Rust 2021 (axum 0.8, tokio, tower-http, rusqlite 0.31 bundled, ort =2.0.0-rc.12 + CUDA, zune-jpeg 0.4, jpeg-encoder 0.6, fast_image_resize 5, kamadak-exif 0.5, quick-xml 0.36, sysinfo 0.32, trash 5, rayon, chrono, toml); React 19 + TypeScript strict + Vite 6 + react-router 7; Vitest; Playwright (`@playwright/test`).

**Spec:** `docs/superpowers/specs/2026-10-19-photosift-overhaul-design.md`

## Roadmap (this is Plan 1 of 4)

| Plan | Delivers | Depends on |
|---|---|---|
| **1 — Foundation** (this doc) | Server, card import with event split + naming, previews, enrichment (faces, identity, sharpness, exposure, moments/bursts), review screen with manual outcomes, XMP for Lightroom, delete-to-Recycle-Bin, Windows startup, Playwright e2e, golden fixture, Tauri app removed | — |
| 2 — Helper + evaluation | Subscription-usage spike first; MCP endpoint over the same operations; `claude -p` runner; proposals with reasons + "your call"; expression signals (eyes/smile); taste profile + learning from overrides; chat panel; evaluation harness scored against historical `Export\` folders; helper summary on the review screen | Plan 1 |
| 3 — Albums + Immich | Event/ongoing albums, best-version-available JPEGs into `Export\`, Immich API album sync (v3), Lightroom-edit swap, "Review album" view | Plans 1–2 |
| 4 — People | Opt-in face naming, recency-weighted matching, `People\|<name>` keywords, album suggestions by person | Plans 1–3 |

"Usable" per the spec (§2) = Plans 1 + 2 at minimum. Plans 2–4 get their own documents when Plan 1 lands, so they can reflect what Plan 1 taught.

## Global Constraints

- **The GitHub repo is public.** Never commit photos, previews, face crops, embeddings, XMP files copied from the library, or personal names. Real-photo fixtures live outside git (`PHOTOSIFT_GOLDEN_DIR`), and test data in git is synthetic.
- Windows 11 only. Bind `127.0.0.1` only; default port `7420`.
- Data root: release `%USERPROFILE%\.photosift\`, debug `%USERPROFILE%\.photosift-dev\`; `PHOTOSIFT_HOME` overrides (empty string = unset). Holds `photosift.db`, `settings.toml`, `cache\preview\`, `cache\thumb\`, `models\`.
- Library root default `E:\photos\DSLR`. Named event folder: `{root}\{YYYY}\{YYYY-MM-DD}_{Slug}\`. Everyday folder: `{root}\{YYYY}\{YYYY-MM}_Everyday\`. Slug: ASCII, words joined by `-`, no spaces or apostrophes ("Baby's First Meal" → `Babys-First-Meal`).
- Files are never moved or renamed after ingest. Original EXIF is never rewritten. The card is never written to.
- Every copied file is SHA-256-verified against the source before an import reports success. Global dedupe by content hash.
- Outcomes: `unreviewed | album | library | archive | delete`, plus a `blooper` boolean tag.
- XMP mapping (provisional, confirmed in Task 17): album ★4, library ★3, archive ★2, delete ★1 + `xmp:Label="Red"`, unreviewed writes nothing. Blooper adds hierarchical keyword `PhotoSift|Blooper`. An existing ★5 is never lowered. Only these fields are written; every other XMP field, including Lightroom's `crs:*`, is preserved byte-for-byte where possible.
- Deletion = Windows Recycle Bin via the `trash` crate, only after explicit batch approval. Never permanent.
- Previews: 2560 px long edge, JPEG q85. Thumbnails: 400 px, q80. EXIF orientation applied. Source: largest embedded JPEG (RAF/NEF) or the JPEG itself.
- UI: every photo rendered whole at its own aspect ratio (fixed-height justified rows), never cropped. Every action persists immediately. Keyboard-first.
- ONNX Runtime via `ort =2.0.0-rc.12` with `cuda`. CUDA/cuDNN DLLs found on `PATH`; CPU fallback if CUDA init fails (logged, never fatal).
- Rust: rustfmt + clippy clean; `anyhow` at handler boundaries, `thiserror` inside modules. TS: strict, no `any`. Conventional commits.

## Review Focus

1. **Re-inserting a card that was already imported** — the scan reports "N already imported" and the import copies only new files. No duplicates in the DB or on disk. → Task 5 test `reimport_same_card_copies_nothing`.
2. **Photos with no EXIF capture time** (screenshots, a camera reset) — fall back to file modified time, flag `time_source = "mtime"`, and still land in an event. → Task 4 test `missing_capture_time_uses_mtime`.
3. **An import interrupted mid-copy** (card pulled, app closed) — re-running the same import finishes without duplicates or orphaned partial files. → Task 5 test `interrupted_import_resumes_cleanly`.
4. **A sidecar where the user set ★5 in Lightroom** — PhotoSift must not lower it when they set Album (★4), and Lightroom's develop settings must survive. → Task 9 tests `five_star_is_never_lowered` and `lightroom_fields_preserved`.
5. **Filename collisions** (two cameras both writing `DSC_0001`, or a counter rollover into a folder that already holds that name) — the RAW and its sibling JPEG must get the *same* new suffix so the pair stays a pair. → Task 5 test `collision_keeps_raw_jpeg_pair_together`.

---

## File Structure

```
server/                              Rust crate `photosift` (binary photosift.exe)
  Cargo.toml
  src/main.rs                        startup: config, db, models, router, drive watcher
  src/config.rs                      Settings (settings.toml) + data-root resolution
  src/db/mod.rs                      open(), migrate(), schema.sql include
  src/db/schema.sql                  all tables
  src/db/photos.rs                   photo/shoot/face CRUD + queries for the review screen
  src/media/embedded.rs              ported from src-tauri/src/pipeline/embedded.rs
  src/media/exif.rs                  ported from src-tauri/src/metadata/exif.rs (+ camera serial)
  src/media/render.rs                decode → orient → preview/thumb JPEGs
  src/media/phash.rs                 matrix-DCT pHash (spike version) + hamming
  src/ingest/scan.rs                 card/folder scan → CardScan
  src/ingest/events.rs               split_into_events, ProposedEvent (pure)
  src/ingest/naming.rs               slugify, folder_path (pure)
  src/ingest/copy.rs                 copy + verify, pair-aware collision naming
  src/ingest/job.rs                  import job orchestration + progress
  src/enrich/faces.rs                YuNet detect with tiling
  src/enrich/identity.rs             SFace align + embed
  src/enrich/quality.rs              sharpness (global + face), exposure
  src/enrich/moments.rs              moments + bursts + signal_score (pure)
  src/enrich/job.rs                  enrichment worker
  src/outcome.rs                     Outcome enum, XMP field mapping
  src/xmp.rs                         merge-write sidecar
  src/trash.rs                       approved delete → Recycle Bin
  src/drives.rs                      removable-drive watcher (injectable lister)
  src/events.rs                      ServerEvent enum + broadcast hub (WebSocket)
  src/api/mod.rs                     router; serves web/dist; /api/*; /ws
  src/api/{scan,import,shoots,photos,media,trash}.rs   handlers
  tests/common/mod.rs                temp data root, synthetic JPEG maker, test server
  tests/*.rs                         integration tests (see tasks)
web/                                 React app
  package.json, vite.config.ts, tsconfig.json, playwright.config.ts
  src/main.tsx, App.tsx
  src/api/client.ts, api/types.ts, api/ws.ts
  src/pages/ShootsPage.tsx, ImportPage.tsx, ReviewPage.tsx
  src/review/{Summary,Highlights,MomentSection,JustifiedRow,Loupe,Compare}.tsx
  src/review/keyboard.ts, review/prefetch.ts
  src/lib/justify.ts                 row-packing math (pure)
  tests/e2e/*.spec.ts                Playwright against a real server + temp data root
scripts/
  fetch-models.ps1                   yunet.onnx + face_recognition_sface_2021dec.onnx → <data root>\models
  make-golden-fixture.ps1            builds a fake card from a local, gitignored manifest
  install-startup.ps1                Task Scheduler entry: run photosift.exe at logon, hidden
```

Removed in Task 18: `src-tauri/`, `src/`, root `package.json`/`vite.config.ts`/`vitest.config.ts`/`tsconfig.json`/`index.html`/`wdio.conf.mjs`, `tests/e2e/` (WebdriverIO), `.github/workflows/screenshots.yml` + `regenerate-screenshots.yml`.

---

### Task 1: Server skeleton, config, health, static web

**Files:**
- Create: `server/Cargo.toml`, `server/src/main.rs`, `server/src/config.rs`, `server/src/api/mod.rs`, `server/tests/common/mod.rs`, `server/tests/health.rs`
- Modify: `.gitignore` (add `server/target/`, `web/node_modules/`, `web/dist/`, `web/test-results/`, `.photosift-golden/`)

**Interfaces:**
- Produces: `config::Settings { library_root: PathBuf, port: u16, event_gap_minutes: u32, everyday_max_photos: usize, moment_gap_seconds: i64, burst_phash_max: u32, burst_window_seconds: i64 }` with defaults `E:\photos\DSLR`, `7420`, `120`, `25`, `90`, `12`, `10`; `config::data_root() -> PathBuf`; `config::load(root: &Path) -> anyhow::Result<Settings>` (writes defaults if the file is missing); `api::router(state: AppState) -> axum::Router`; `AppState { db: DbPool, settings: Arc<Settings>, data_root: PathBuf, events: EventHub }` (fields filled in by later tasks; start with `settings` + `data_root`); test helper `common::spawn_server() -> TestServer { base_url: String, data_root: TempDir }`.

- [ ] **Step 1: Write the failing tests** — `server/tests/health.rs`:
  - `health_reports_ok`: `GET /api/health` → 200, JSON `{"ok": true, "version": env!("CARGO_PKG_VERSION")}`.
  - `settings_defaults_written`: after `spawn_server()`, `<data_root>/settings.toml` exists and parses to `library_root == "E:\\photos\\DSLR"` and `port == 7420`.
  - `serves_index_when_web_dist_present`: put `index.html` containing `<title>PhotoSift</title>` in a temp `web_dist` dir passed through `PHOTOSIFT_WEB_DIST`; `GET /` → 200 containing `PhotoSift`; `GET /review/123` (client route) → the same index.html.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test health` → FAIL (crate doesn't exist).
- [ ] **Step 3: Implement.** `main.rs` resolves the data root (`cfg!(debug_assertions)` → `.photosift-dev`), loads settings, builds the router, and binds `127.0.0.1:{port}`. Web dist comes from `PHOTOSIFT_WEB_DIST`, else `<exe dir>\web`. Unknown non-`/api` GETs fall back to `index.html` (tower-http `ServeDir` + `not_found_service`). Copy the dev-profile `opt-level` overrides for `ort`, `ort-sys`, `zune-jpeg`, `jpeg-encoder` and `fast_image_resize` from `src-tauri/Cargo.toml` (`[profile.dev.package.*]`).
- [ ] **Step 4: Run** the same command → PASS. Also run `cargo clippy --manifest-path server/Cargo.toml -- -D warnings` → clean.
- [ ] **Step 5: Commit** `feat(server): axum skeleton with config, health and static web serving`

### Task 2: Database schema and CRUD

**Files:**
- Create: `server/src/db/mod.rs`, `server/src/db/schema.sql`, `server/src/db/photos.rs`, `server/tests/db.rs`

**Interfaces:**
- Consumes: `config::data_root()`.
- Produces: `db::open(path: &Path) -> anyhow::Result<DbPool>`, where `DbPool` is an `r2d2`-free `Arc<Mutex<Connection>>` wrapper with `fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> anyhow::Result<T>`. Schema (WAL, `foreign_keys=ON`):
  - `shoots(id INTEGER PK, kind TEXT CHECK(kind IN ('named','everyday')), name TEXT, folder TEXT UNIQUE, date_start TEXT, date_end TEXT, created_at TEXT)`
  - `photos(id INTEGER PK, shoot_id INT REFERENCES shoots, raw_path TEXT, jpeg_path TEXT, video_path TEXT, sha256 TEXT UNIQUE, camera TEXT, capture_time_orig TEXT, capture_time TEXT, time_source TEXT CHECK(time_source IN ('exif','mtime')), orientation INT, width INT, height INT, phash INTEGER, sharpness REAL, face_sharpness REAL, mean_luma REAL, clip_hi REAL, clip_lo REAL, moment_idx INT, burst_idx INT, signal_score REAL, outcome TEXT NOT NULL DEFAULT 'unreviewed', blooper INT NOT NULL DEFAULT 0, enriched_at TEXT, trashed_at TEXT)`. Each copied file also gets `files(sha256 TEXT PK, path TEXT, photo_id INT)` for global dedupe, including sibling JPEGs and MOVs.
  - `faces(id INTEGER PK, photo_id INT REFERENCES photos ON DELETE CASCADE, x REAL, y REAL, w REAL, h REAL, score REAL, landmarks TEXT, embedding BLOB, sharpness REAL)` — boxes normalized 0–1 in display orientation.
  - `camera_offsets(shoot_id INT, camera TEXT, offset_seconds INT, PRIMARY KEY(shoot_id, camera))`.
- `photos.rs`: `insert_shoot`, `insert_photo`, `known_hashes(&[String]) -> HashSet<String>`, `set_outcome(photo_id, Outcome, blooper: Option<bool>) -> PhotoRow`, `shoot_review(shoot_id) -> ShootReview` (returned shape defined in Task 10).

- [ ] **Step 1: Write failing tests** (`server/tests/db.rs`):
  - `migrate_is_idempotent`: open twice → no error, `PRAGMA user_version == 1`.
  - `duplicate_sha_rejected`: inserting two files with the same sha256 fails with a constraint error.
  - `set_outcome_persists`: insert → `set_outcome(id, Album, Some(true))` → reopen DB → row has `outcome='album'`, `blooper=1`.
  - `outcome_check_constraint`: raw SQL `UPDATE photos SET outcome='pick'` fails.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test db` → FAIL.
- [ ] **Step 3: Implement** the schema (with `CHECK(outcome IN ('unreviewed','album','library','archive','delete'))`) and the CRUD functions. Migrations are keyed on `user_version`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(server): sqlite schema for shoots, photos, files, faces`

### Task 3: Media — embedded JPEG, EXIF, render, pHash

**Files:**
- Create: `server/src/media/{embedded,exif,render,phash}.rs`, `server/tests/media.rs`, `server/tests/fixtures/synthetic/` (generated in test code, not committed binaries)

**Interfaces:**
- Produces:
  - `embedded::largest_jpeg(raw: &[u8], ext: &str) -> Result<&[u8], MediaError>` — port `extract_all_jpegs` and `find_jpeg_end` from `src-tauri/src/pipeline/embedded.rs`, working on an in-memory buffer (read the file once; Task 5 hashes the same buffer).
  - `exif::read(bytes: &[u8], path: &Path) -> ExifInfo { capture_time: Option<NaiveDateTime>, camera: Option<String>, orientation: u16, width: u32, height: u32 }` — port from `src-tauri/src/metadata/exif.rs`. `camera` = `"{Make} {Model} #{BodySerialNumber}"` when the serial exists, else make+model.
  - `render::make_previews(jpeg: &[u8], orientation: u16, preview_out: &Path, thumb_out: &Path) -> Result<(u32, u32), MediaError>` — returns display width/height after orientation. Use zune-jpeg to decode, fast_image_resize (CatmullRom) for the 2560/400 resizes, and jpeg-encoder for q85/q80.
  - `phash::compute(thumb_rgb: &[u8], w: u32, h: u32) -> i64` (32×32 grayscale, precomputed DCT matrix, 8×8 low-frequency block, median of the AC terms; the algorithm is in `spikes/stack-bench/rust/src/main.rs` `phash`), and `phash::hamming(a: i64, b: i64) -> u32`.
- [ ] **Step 1: Write failing tests**:
  - `orientation_6_rotates_portrait`: a synthetic 600×400 JPEG with orientation 6 (built in-test with jpeg-encoder) → `make_previews` returns `(400, 600)`, and the preview file decodes to 400×600.
  - `preview_and_thumb_long_edges`: a 6000×4000 synthetic → preview 2560×1707 (±1), thumb 400×267 (±1).
  - `phash_stable_and_discriminating`: same image → identical hash; brightness +10% → hamming ≤ 4; a different pattern → hamming ≥ 20.
  - `embedded_finds_largest`: a buffer holding two JPEGs (20 KB and 60 KB) with junk around them → returns the 60 KB one.
  - `golden_raf_and_nef` (`#[ignore]`; runs when `PHOTOSIFT_GOLDEN_DIR` is set): every RAF/NEF in the golden card yields a JPEG ≥ 1 MB and a capture time, and RAF dimensions are 4416×2944 before orientation.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test media` → FAIL.
- [ ] **Step 3: Implement** the four modules.
- [ ] **Step 4: Run** → PASS. Then run `PHOTOSIFT_GOLDEN_DIR=… cargo test --test media -- --ignored` locally once Task 6 exists (note it in the Task 6 checklist).
- [ ] **Step 5: Commit** `feat(server): embedded preview, exif, oriented previews, phash`

### Task 4: Card scan, event split, naming

**Files:**
- Create: `server/src/ingest/{scan,events,naming}.rs`, `server/tests/ingest_plan.rs`

**Interfaces:**
- Consumes: `exif::read`, `embedded::largest_jpeg`; port `pair()` from `src-tauri/src/ingest/pairing.rs` into `scan.rs` (RAW + same-stem JPEG = one item; `.MOV`/`.MP4` = video item).
- Produces:
  - `scan::scan(source: &Path, db: &DbPool) -> anyhow::Result<CardScan>`
    - `CardScan { scan_id: String, source: PathBuf, items: Vec<ScanItem>, cameras: Vec<CameraRange>, already_imported: usize, events: Vec<ProposedEvent> }`
    - `ScanItem { idx: usize, primary: PathBuf, sibling_jpeg: Option<PathBuf>, kind: ItemKind /* Photo|Video */, camera: String, capture_time: NaiveDateTime, time_source: TimeSource /* Exif|Mtime */, bytes: u64 }`
    - `CameraRange { camera: String, count: usize, first: NaiveDateTime, last: NaiveDateTime }`
  - The scan reads only EXIF headers, not whole files. `already_imported` uses a cheap key `(file name, size, capture_time)` matched against `files` joined to `photos`. The authoritative check is the SHA in Task 5.
  - `events::split_into_events(items: &[ScanItem], offsets: &HashMap<String, i64>, gap_minutes: u32, everyday_max: usize) -> Vec<ProposedEvent>`
    - `ProposedEvent { item_idxs: Vec<usize>, start: NaiveDateTime, end: NaiveDateTime, kind: EventKind /* Named|Everyday */, suggested_name: String }`
    - Sort by `capture_time + offset[camera]`. Start a new event when the gap exceeds `gap_minutes` or the local calendar date changes. Events with fewer than `everyday_max` photos (videos don't count) default to `Everyday`. `suggested_name` is `""` for now (Plan 2 fills it).
  - `naming::slugify(name: &str) -> String`, `naming::folder_path(root: &Path, kind: EventKind, date: NaiveDate, name: &str) -> PathBuf`.
- [ ] **Step 1: Write failing tests**:
  - `slugify_cases`: `"Baby's First Meal"` → `"Babys-First-Meal"`; `"  Birthday — at the Park! "` → `"Birthday-at-the-Park"`; `"Café día"` → `"Cafe-dia"`; `""` → `"Untitled"`.
  - `folder_paths`: Named on 2026-10-18 "Birthday Party" → `E:\photos\DSLR\2026\2026-10-18_Birthday-Party`; Everyday on 2026-10-14 → `E:\photos\DSLR\2026\2026-10_Everyday`.
  - `splits_by_gap_and_date`: times 10:00, 10:05, 13:00 (gap 175 min), and next day 09:00 → 3 events.
  - `small_event_defaults_everyday`: 12 photos → `Everyday`; 30 photos → `Named` (`everyday_max = 25`).
  - `camera_offset_shifts_items`: camera B at 12:00 with offset +3600 s joins camera A's 13:00 event.
  - `missing_capture_time_uses_mtime` (Review Focus 2): a JPEG with no EXIF → `time_source == Mtime`, `capture_time` equals the file's mtime, and it's included in an event.
  - `pairs_raf_with_jpg`: temp dir with `DSCF0001.RAF` (synthetic bytes) + `DSCF0001.JPG` + `DSCF0002.JPG` + `CLIP0001.MOV` → 3 items: one pair, one lone JPEG, one video.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test ingest_plan` → FAIL.
- [ ] **Step 3: Implement.** `slugify`: transliterate Latin-1 accents, drop apostrophes, replace any other run of non-alphanumerics with `-`, trim the dashes.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(ingest): card scan, event split, folder naming`

### Task 5: Import job — copy, verify, dedupe, previews, progress

**Files:**
- Create: `server/src/ingest/{copy,job}.rs`, `server/src/events.rs`, `server/src/api/{scan,import}.rs`, `server/tests/import.rs`
- Modify: `server/src/api/mod.rs` (routes), `server/src/main.rs` (EventHub in AppState)

**Interfaces:**
- Consumes: Task 4 `CardScan`, `ProposedEvent`, `naming::folder_path`; Task 3 media fns; Task 2 CRUD.
- Produces:
  - `events::ServerEvent` (serde, `#[serde(tag = "type", rename_all = "snake_case")]`): `CardDetected { scan: CardScanSummary }` with `CardScanSummary { scan_id, source, photo_count, video_count, already_imported, cameras: Vec<CameraRange>, events: [{ start, end, count, kind }] }`, `CardRemoved { source: String }`, `ImportProgress { job_id: String, done: usize, total: usize, current: String }`, `ImportDone { job_id: String, shoot_ids: Vec<i64>, copied: usize, skipped_duplicates: usize }`, `ImportFailed { job_id: String, error: String }`, `EnrichProgress { shoot_id: i64, done: usize, total: usize }`, `EnrichDone { shoot_id: i64 }`. `EventHub` is a `tokio::sync::broadcast::Sender<ServerEvent>` wrapper. `GET /ws` streams events as JSON text frames.
  - `POST /api/scan { source: String }` → `CardScan` (cached in memory by `scan_id`).
  - `POST /api/import { scan_id, events: [{ item_idxs: [usize], kind: "named"|"everyday", name: String }], camera_offsets: { camera: seconds } }` → `{ job_id }`, then runs in the background.
  - `copy::copy_verified(src: &Path, dest_dir: &Path, stem_override: Option<&str>) -> io::Result<CopiedFile { dest: PathBuf, sha256: String }>`. Copy to `<name>.partial`, hash the bytes as written, re-read the dest and compare, then rename to the final name. On a name collision, pick the lowest `_{n}` suffix that's free for the RAW, its sibling JPEG and its XMP together, and pass it to the siblings through `stem_override`.
- [ ] **Step 1: Write failing tests** (`server/tests/import.rs`, synthetic JPEG cards built in temp dirs):
  - `import_creates_folders_rows_previews`: a 3-file card on one day, imported as Named "Test Day" → folder `<lib>\2026\2026-10-18_Test-Day\` holds the 3 files, `photos` has 3 rows with `capture_time`, and `cache\preview\{id}.jpg` and `cache\thumb\{id}.jpg` exist. WebSocket receives `ImportProgress` frames and then `ImportDone { copied: 3 }`.
  - `reimport_same_card_copies_nothing` (Review Focus 1): import again → scan `already_imported == 3`, and the import reports `copied: 0, skipped_duplicates: 3` with no new files on disk.
  - `interrupted_import_resumes_cleanly` (Review Focus 3): leave a stale `x.jpg.partial` in the destination plus one file already copied, then re-run → no `.partial` files remain, no `_1` duplicates, and 3 rows.
  - `collision_keeps_raw_jpeg_pair_together` (Review Focus 5): the destination already holds an unrelated `DSCF0001.RAF`, and the card has a `DSCF0001.RAF` + `DSCF0001.JPG` pair → both land as `DSCF0001_1.RAF` and `DSCF0001_1.JPG`, and the row's `raw_path`/`jpeg_path` point at them.
  - `card_never_written`: after import, the card dir's file list and mtimes are unchanged.
  - `video_copied_not_rendered`: a `.MOV` lands in the event folder with `photos.video_path` set and no preview.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test import` → FAIL.
- [ ] **Step 3: Implement.** Each item is processed in a rayon pool (16 threads): read the source once → SHA-256 → skip if the hash is known → `copy_verified` → embedded JPEG → EXIF → previews → pHash. DB inserts are batched per event in one transaction. `capture_time = capture_time_orig + offset`, and `camera_offsets` rows are stored. When an import finishes, `enrich::job::enqueue(shoot_id)` is called for every shoot it touched (Task 7 implements it; until then it's a no-op stub with that signature).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(ingest): verified, deduplicated import with progress events`

### Task 6: Golden fixture (local, gitignored)

**Files:**
- Create: `scripts/make-golden-fixture.ps1`, `tests/golden/manifest.example.txt`, `tests/golden/README.md`, `server/tests/golden.rs`
- Modify: `.gitignore` (`tests/golden/manifest.txt`)

**Interfaces:**
- Produces: a fake card at `$env:PHOTOSIFT_GOLDEN_DIR` (default `<repo>\.photosift-golden\card`) shaped like `DCIM\100_FUJI\*.RAF|*.JPG` and `DCIM\100NIKON\*.NEF`, plus `expected.toml` with counts the tests assert.
- The manifest (`tests/golden/manifest.txt`, gitignored; the committed example uses placeholder paths) lists absolute source paths. The real local manifest already exists on this machine (16 files from a 2026-10 party shoot plus 3 from another day). Its files: `DSCF1613 DSCF1615 DSCF1616 DSCF1617` (near-identical burst), `DSCF1620` (blooper), `DSCF1636`, `DSCF1651 DSCF1654` (portraits), `DSCF1724` (action), `DSCF1765 DSCF1766` (your-call pair), `DSCF1790`, `DSCF1890 DSCF1921` (portraits), each as RAF + JPG, plus `DSC_4749.NEF` (12:48) and `DSC_4774.NEF` (16:07). Add 3 photos from a different day (any 2026 shoot) to exercise the event split.
- [ ] **Step 1: Write the failing test** `server/tests/golden.rs` (`#[ignore]`, requires `PHOTOSIFT_GOLDEN_DIR`): scan the golden card → `cameras.len() == 2`, events = 2 (the party plus the other day), `items.len()` equals `expected.toml` `photos`. Then import into a temp library → every photo has a preview and pHash, and the portrait files' previews are taller than they are wide.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test golden -- --ignored` → FAIL (no fixture).
- [ ] **Step 3: Implement** the PowerShell script: read the manifest, copy files (and same-stem JPGs) into the fake card's DCIM folders using `[System.IO.File]::Copy` (not Get-Content/Set-Content), and write `expected.toml`. Document running it in `tests/golden/README.md`.
- [ ] **Step 4: Run** the script, then the test with `--ignored` → PASS. Also run the Task 3 `golden_raf_and_nef` test.
- [ ] **Step 5: Commit** `test: local golden-card fixture and end-to-end import check` (commit the script, the example manifest and the README only).

### Task 7: Enrichment — faces (tiled), identity, quality

**Files:**
- Create: `server/src/enrich/{faces,identity,quality,job}.rs`, `scripts/fetch-models.ps1`, `server/tests/enrich.rs`

**Interfaces:**
- Consumes: previews in `cache\preview\{id}.jpg` (display orientation).
- Produces:
  - `faces::Detector::load(models_dir: &Path, cuda: bool) -> anyhow::Result<(Detector, Backend /* Cuda|Cpu */)>`; `Detector::detect(&self, img: &RgbImage) -> Vec<Face { bbox: (f32,f32,f32,f32) /* normalized */, landmarks: [(f32,f32);5], score: f32 }>`. Run YuNet (640×640 letterbox; decoding and NMS ported from `src-tauri/src/ai/face.rs`) on the full frame **and** on 2×2 tiles with 15% overlap, then merge with NMS (IoU 0.3). Confidence 0.5.
  - `identity::Embedder::load(models_dir, cuda)`; `Embedder::embed(&self, img: &RgbImage, landmarks: &[(f32,f32);5]) -> [f32; 128]` — similarity transform to the ArcFace 112×112 template, RGB planes, raw 0–255 floats. Port it from `spikes/stack-bench/rust/src/main.rs` (`similarity`, `warp112`).
  - `quality::measure(gray_1024: &[f32], w: usize, h: usize, faces: &[Face]) -> Quality { sharpness: f64, face_sharpness: Option<f64>, mean_luma: f64, clip_hi: f64, clip_lo: f64 }`. Sharpness is Laplacian variance; `face_sharpness` is the max Laplacian variance over face boxes, measured on the 2560 preview.
  - `enrich::job::enqueue(shoot_id)`: a single background worker processes photos where `enriched_at IS NULL`, writes `faces` + quality columns, emits `EnrichProgress`/`EnrichDone`, then calls `moments::assign(&db, shoot_id)` (Task 8).
  - `GET /api/status` → `{ ai_backend: "cuda"|"cpu"|"unavailable", queue: usize }`.
- [ ] **Step 1: Write failing tests**:
  - `quality_blur_scores_lower`: a synthetic checkerboard vs the same image box-blurred → `sharpness` strictly higher for the sharp one. Over-exposed synthetic (all 255) → `clip_hi > 0.99`.
  - `similarity_maps_template_to_itself`: landmarks equal to the ArcFace template → transform ≈ identity (|a−1| < 1e-4, |b| < 1e-4, translation < 1e-3).
  - `golden_faces_found` (`#[ignore]`): on the golden card, `DSCF1616` has ≥ 3 faces. Over the whole card, tiled detection finds ≥ as many faces as full-frame-only (record both counts in test output). Two faces of the same child in `DSCF1616` and `DSCF1617` have embedding cosine ≥ 0.8.
  - `missing_models_is_not_fatal`: an empty models dir → `GET /api/status` reports `ai_backend: "unavailable"`, the import still succeeds, and photos get quality signals but no faces.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test enrich` → FAIL.
- [ ] **Step 3: Implement.** `fetch-models.ps1` downloads `face_detection_yunet_2023mar.onnx` and `face_recognition_sface_2021dec.onnx` from `opencv/opencv_zoo` into `<data root>\models` and checks file sizes. Inference sessions sit behind `Mutex<Session>`. Decode, preprocessing and quality run in rayon.
- [ ] **Step 4: Run** → PASS, including `--ignored` locally with CUDA DLLs on `PATH`. Record the enrichment wall time for the golden card in the commit message body.
- [ ] **Step 5: Commit** `feat(enrich): tiled face detection, identity embeddings, sharpness and exposure`

### Task 8: Moments, bursts, signal score

**Files:**
- Create: `server/src/enrich/moments.rs`, `server/tests/moments.rs`

**Interfaces:**
- Produces:
  - `moments::split(photos: &[MomentInput]) -> Vec<(usize /*moment_idx*/, usize /*burst_idx*/)>`, where `MomentInput { capture_time: NaiveDateTime, phash: i64 }` is sorted by time. A new **moment** (review-screen section) starts when the gap exceeds `moment_gap_seconds` (90). A new **burst** (the unit a pick is made from) starts when the gap exceeds `burst_window_seconds` (10) or the pHash distance to the burst's first frame exceeds `burst_phash_max` (12). That makes a pose change a new burst even seconds apart (spec §6: 1572 vs 1573).
  - `moments::signal_score(s: &Signals) -> f32`, where `Signals { face_count: usize, largest_face_area: f32, face_sharpness: Option<f64>, sharpness: f64, clip_hi: f64, clip_lo: f64 }`. This is a temporary, explainable ranking used for Highlights and in-burst ordering until Plan 2's helper.
  - `moments::assign(db: &DbPool, shoot_id: i64) -> anyhow::Result<()>` writes `moment_idx`, `burst_idx` and `signal_score`.
- [ ] **Step 1: Write failing tests**:
  - `moment_gap_splits_sections`: times 0 s, 30 s, 200 s → moments `[0,0,1]`.
  - `pose_change_starts_new_burst`: two frames 1 s apart with pHash distance 30 → bursts differ; distance 5 → same burst.
  - `score_prefers_sharp_faces`: same faces, face_sharpness 400 vs 80 → the first scores higher.
  - `score_prefers_faces`: one face vs none, equal sharpness → the face scores higher.
  - `score_penalizes_clipping`: `clip_hi` 0.15 vs 0.0 → the clipped one scores lower.
  - `golden_burst_groups_near_identicals` (`#[ignore]`): `DSCF1615/1616/1617` share a burst, and `DSCF1765/1766` share a burst.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test moments` → FAIL.
- [ ] **Step 3: Implement** `signal_score` as a weighted sum of normalized terms. Pick the weights, then document each weight's reason in one `//` line per term.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(enrich): moments, bursts and signal score`

### Task 9: Outcomes and XMP merge-writer

**Files:**
- Create: `server/src/outcome.rs`, `server/src/xmp.rs`, `server/src/api/photos.rs`, `server/tests/xmp.rs`, `server/tests/fixtures/lightroom_sidecar.xmp` (synthetic: hand-written to mimic a Lightroom sidecar with `crs:ProcessVersion`, `crs:WhiteBalance`, `xmp:Rating="5"`, `xmp:Label="Blue"`, `dc:subject` with one keyword; no real serials or names)

**Interfaces:**
- Consumes: the quick-xml read/merge pattern from `src-tauri/src/metadata/xmp.rs` (`update_cull_attrs_in_xml`).
- Produces:
  - `outcome::Outcome` (serde lowercase) and `outcome::xmp_fields(o: Outcome, blooper: bool) -> XmpFields { rating: Option<i32>, label: Option<&'static str>, add_keywords: Vec<&'static str>, remove_keywords: Vec<&'static str> }`, following the Global Constraints mapping. Hierarchical keywords go in `lr:hierarchicalSubject`, and the leaf is mirrored in `dc:subject`.
  - `xmp::apply(image_path: &Path, fields: &XmpFields) -> Result<(), XmpError>`. Sidecar path = `<stem>.xmp` beside the RAW (or JPEG-only). Rules: write the rating only if the current one isn't 5. Write the label only for Delete, and remove `Red` when leaving Delete only if PhotoSift set it (track with `photosift:LabelSet="1"`). The file is never rewritten when nothing changes.
  - `POST /api/photos/{id}/outcome { outcome, blooper? }` → `PhotoRow`. Persists to the DB, then applies XMP. An XMP failure is reported in the response (`xmp_error`) and doesn't roll back the DB.
- [ ] **Step 1: Write failing tests**:
  - `writes_rating_for_each_outcome`: a fresh sidecar per outcome → ratings 4/3/2/1. Unreviewed → no file is created.
  - `delete_sets_red_and_reverting_clears_it`: Delete → `xmp:Label="Red"`; then Archive → label removed.
  - `user_label_untouched`: the fixture's `Blue` label survives Album and Library.
  - `five_star_is_never_lowered` (Review Focus 4): the fixture holds ★5 → setting Album leaves ★5.
  - `lightroom_fields_preserved` (Review Focus 4): after Album + blooper, every `crs:*` attribute in the fixture is still present with identical values, the existing keyword is still present, and `PhotoSift|Blooper` was added once (applying twice doesn't duplicate it).
  - `no_write_when_unchanged`: applying the same fields twice leaves mtime unchanged.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test xmp` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(xmp): outcome mapping with Lightroom-safe sidecar merge`

### Task 10: Review data API and delete-to-Recycle-Bin

**Files:**
- Create: `server/src/api/{shoots,media,trash}.rs`, `server/src/trash.rs`, `server/tests/review_api.rs`

**Interfaces:**
- Produces:
  - `GET /api/shoots` → `[{ id, name, kind, folder, date_start, date_end, counts: { total, unreviewed, album, library, archive, delete }, enriched: bool }]`, newest first.
  - `GET /api/shoots/{id}/review` → `ShootReview { shoot, counts, highlights: [photo_id] /* best photo of each of the 8 highest-scoring bursts */, moments: [{ idx, start, end, bursts: [{ idx, photo_ids /* ordered by signal_score desc */ }] }], photos: { [id]: { id, w, h, capture_time, camera, outcome, blooper, face_count, signal_score, has_video } } }`.
  - `GET /media/preview/{id}`, `GET /media/thumb/{id}` → JPEG with `Cache-Control: public, max-age=31536000, immutable` (a preview never changes for a given id).
  - `POST /api/shoots/{id}/trash` → `{ trashed: usize, failed: [{photo_id, error}] }`. This sends every `outcome='delete'` photo of the shoot (RAW, sibling JPEG and sidecar) to the Recycle Bin via `trash::delete_all` and sets `trashed_at`. Trashed photos drop out of `review` and `counts`.
- [ ] **Step 1: Write failing tests**:
  - `review_shape_and_order`: after a synthetic import, the review has moments/bursts covering every non-trashed photo exactly once, `highlights.len() <= 8`, and burst `photo_ids` sorted by `signal_score` descending.
  - `preview_is_cacheable`: the response header contains `immutable`.
  - `trash_only_approved_deletes`: 3 photos, one marked delete → `trashed: 1`, its files are gone from the folder, the other two remain, and the counts update.
  - `trash_failure_reported_not_fatal`: one file locked open → reported in `failed`, and the rest are trashed.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test review_api` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(api): review data, cacheable media, recycle-bin delete`

### Task 11: SD-card watcher

**Files:**
- Create: `server/src/drives.rs`, `server/tests/drives.rs`
- Modify: `server/src/main.rs`

**Interfaces:**
- Produces: `drives::Watcher::new(lister: Box<dyn Fn() -> Vec<DriveInfo> + Send>, hub: EventHub, scan: impl Fn(&Path) -> anyhow::Result<CardScan>)`, plus `Watcher::tick(&mut self)`, which `main` calls every 2 s on the `sysinfo` lister ported from `src-tauri/src/drives/mod.rs`. A newly appearing removable drive with a `DCIM` folder triggers a scan and `ServerEvent::CardDetected { scan }`. A drive disappearing triggers `CardRemoved` (`CardScanSummary` is defined in Task 5).
- [ ] **Step 1: Write failing tests** with a fake lister: no DCIM → no event; new drive with DCIM → exactly one `CardDetected` across 3 ticks; drive removed → `CardRemoved`; reinserted → `CardDetected` again.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test drives` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(server): removable-card watcher with scan on insert`

### Task 12: Web app skeleton, typed client, live events

**Files:**
- Create: `server/src/bin/make-synthetic-card.rs` (args: out dir + JSON list of `{name, time, camera, w, h, orientation}`; writes JPEGs with that EXIF), `web/package.json`, `web/vite.config.ts` (dev proxy `/api`, `/media`, `/ws` → `127.0.0.1:7420`), `web/tsconfig.json`, `web/index.html`, `web/src/{main,App}.tsx`, `web/src/api/{types,client,ws}.ts`, `web/src/pages/ShootsPage.tsx`, `web/src/api/client.test.ts`, `web/playwright.config.ts`, `web/tests/e2e/fixtures.ts`, `web/tests/e2e/shoots.spec.ts`

**Interfaces:**
- Consumes: the endpoints and `ServerEvent` from Tasks 5 and 10.
- Produces: `api/types.ts` mirroring the Rust JSON shapes exactly (`Outcome`, `CardScan`, `ShootReview`, `ServerEvent` as a discriminated union on `type`); `client.ts` functions `scan`, `startImport`, `listShoots`, `getReview`, `setOutcome`, `trashDeletes`; `ws.ts` `useServerEvents(handler)`, which reconnects with backoff. Routes: `/` Shoots, `/import/:scanId`, `/review/:shootId`. The Playwright fixture (`fixtures.ts`) builds `server/` once and starts `photosift.exe` with a temp `PHOTOSIFT_HOME`, a temp library root, `PHOTOSIFT_WEB_DIST=web/dist` and a free port. It exposes `makeCard(files)`, which writes synthetic JPEGs with given EXIF times (via a small `server` test binary, `cargo run --bin make-synthetic-card`).
- [ ] **Step 1: Write failing tests**: Vitest `client.test.ts` — `setOutcome` posts `{outcome:"album"}` to `/api/photos/7/outcome` and returns the parsed row (fetch mocked). Playwright `shoots.spec.ts` — with an empty library, `/` shows "No shoots yet". After importing a 3-photo synthetic card through the API, `/` lists one shoot with "3 photos".
- [ ] **Step 2: Run** `npm --prefix web run test` and `npm --prefix web run e2e -- shoots.spec.ts` → FAIL.
- [ ] **Step 3: Implement.** Scripts: `dev`, `build`, `test` (vitest run), `e2e` (playwright test). No state library. React state plus the server as the source of truth is enough at this size.
- [ ] **Step 4: Run** both → PASS.
- [ ] **Step 5: Commit** `feat(web): app skeleton, typed client, live server events`

### Task 13: Import screen

**Files:**
- Create: `web/src/pages/ImportPage.tsx`, `web/src/components/CardBanner.tsx`, `web/tests/e2e/import.spec.ts`

**Interfaces:**
- Consumes: `CardDetected`, `POST /api/scan`, `POST /api/import`, `ImportProgress`/`ImportDone`.
- Behaviour: `CardBanner` (global) reads "SD card: {photo_count} photos, {n} days. Import?" plus "{already_imported} already imported" when > 0. The import page lists proposed events (date range, count, kind toggle, name field with a live folder-path preview using the server's slug rule via `POST /api/slug {name}` → `{slug}`), merge-with-next and split-at-photo controls (thumbnails come from the scan's embedded JPEGs via `GET /media/scan/{scan_id}/{idx}`), and per-camera time ranges with an offset input (minutes) that re-groups live. "Import" starts the job. A progress bar follows `ImportProgress`, and when it's done the page links to each shoot's review.
- [ ] **Step 1: Write failing Playwright tests** (`import.spec.ts`):
  - Synthetic card: 30 photos on day A + 5 on day B. The page shows 2 events, with day B toggled to Everyday. Typing "Baby's First Meal" for day A shows `2026-10-18_Babys-First-Meal`. Clicking Import → progress reaches 100%, the review links appear, and the folders exist on disk.
  - Two cameras, camera B 3 h earlier, and a +180 min offset entered for B → the events re-group from 2 to 1 before import.
  - The same card imported twice → the banner says "35 already imported", and the import copies 0.
- [ ] **Step 2: Run** `npm --prefix web run e2e -- import.spec.ts` → FAIL.
- [ ] **Step 3: Implement** (plus `POST /api/slug` and `GET /media/scan/...` on the server, with a Rust test each in `server/tests/import.rs`).
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(web): card banner and event-split import screen`

### Task 14: Review screen

**Files:**
- Create: `web/src/pages/ReviewPage.tsx`, `web/src/review/{Summary,Highlights,MomentSection,JustifiedRow,Loupe,Compare}.tsx`, `web/src/review/{keyboard,prefetch}.ts`, `web/src/lib/justify.ts`, `web/src/lib/justify.test.ts`, `web/tests/e2e/review.spec.ts`

**Interfaces:**
- Consumes: `GET /api/shoots/{id}/review`, `POST /api/photos/{id}/outcome`, `POST /api/shoots/{id}/trash`, media URLs.
- Produces: `justify.packRows(items: {id:number; aspect:number}[], containerWidth: number, targetHeight: number, gap: number) -> {height:number; items:{id:number; width:number}[]}[]`. Every row except the last fills the width exactly (±1 px). Every item width = height × aspect, so nothing is cropped.
- Layout per spec §10 and mockup `highlights-combined-v3`: Summary (shoot name, date, cameras, outcome counts as filter chips) → Highlights (from `review.highlights`, large) → Moments in time order, with each burst's top photo large and the burst's other frames in a dim strip → a "Delete? (n) — Move to Recycle Bin" action with a confirm dialog listing the count. Its copy also says: "Already imported this shoot into Lightroom? Use Lightroom's Delete Rejected Photos instead, so the catalog stays in sync" (spec §7). The page remembers the last focused photo per shoot (`localStorage`, wrapped in try/catch) and restores it on return. The helper's reasons are absent until Plan 2, so leave a slot under each photo.
- Keyboard (`keyboard.ts`, one binding table also rendered by `?`): `↑/↓` moment, `←/→` photo, `A` album, `L` library, `R` archive, `X` delete?, `B` toggle blooper, `U` unreviewed, `Enter` loupe, `C` compare the current burst side-by-side, `Esc` back, `1-5` filter chips. Keys work while the loupe has focus.
- Prefetch (`prefetch.ts`): preload `/media/preview/{id}` for the next 3 and previous 1 photos in navigation order using `new Image()` + `decode()`.
- [ ] **Step 1: Write failing tests**:
  - Vitest `justify.test.ts`: aspects [1.5, 0.667, 1.5] at width 1000, target 200 → one row whose widths sum to 1000 − 2×gap (±1), each width / row height = aspect (±0.01); a single very wide panorama becomes its own row; empty input → `[]`.
  - Playwright `review.spec.ts` on a synthetic 40-photo shoot (portraits and landscapes, 3 moments): (a) every `img` in highlights and moments has rendered width / height within 2% of its natural aspect (nothing cropped); (b) focus a photo, press `A` → its chip count increments, then reload → still album, and its sidecar has ★4; (c) `Enter` opens the loupe, `→` advances, and `A` there also persists; (d) mark 2 with `X`, click "Move to Recycle Bin", confirm → both disappear and the Delete? count is 0; (e) `C` shows the current burst's frames side by side; (f) after focusing the 12th photo, reload → focus is restored to the 12th photo.
  - Playwright latency check: in the loupe, press `→` 50 times with a 300 ms pause → p50 paint time < 30 ms, measured with `performance.now()` around `img.decode()` plus two `requestAnimationFrame`s as in `spikes/stack-bench/viewer/index.html`. The 400-thumb grid of the golden or synthetic shoot finishes loading in < 1000 ms.
- [ ] **Step 2: Run** `npm --prefix web run test` and `npm --prefix web run e2e -- review.spec.ts` → FAIL.
- [ ] **Step 3: Implement.** Use `loading="lazy"` on moment strips and eager loading on highlights. A virtualized list isn't needed at 400 photos. If the grid test fails, add it.
- [ ] **Step 4: Run** → PASS. Then exercise the real golden shoot in a browser yourself: screenshot the review screen, check the portraits are upright and nothing is cropped, and attach the screenshot to the task notes (not git).
- [ ] **Step 5: Commit** `feat(web): moments review screen with keyboard outcomes`

### Task 15: Windows startup and packaging

**Files:**
- Create: `scripts/install-startup.ps1`, `scripts/build-release.ps1`
- Modify: `server/src/main.rs` (`--open` flag; single-instance guard)

**Interfaces:**
- Produces: `build-release.ps1` → `dist\photosift\photosift.exe` + `web\` (from `web/dist`) + the ORT provider DLLs copied from `server\target\release\`. `install-startup.ps1` registers Task Scheduler task `PhotoSift` (at logon, hidden, restart on failure up to 3 times) running that exe. A second launch with `--open` doesn't start a second server. It opens `http://127.0.0.1:7420/` in the default browser and exits 0 (single-instance via binding the port; if the bind fails and `/api/health` answers, open and exit).
- [ ] **Step 1: Write the failing test** `server/tests/single_instance.rs`: start a server, then run the binary again with `--no-browser --open` → it exits 0 within 2 s, and the first server still answers `/api/health`.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test single_instance` → FAIL.
- [ ] **Step 3: Implement** the guard and both scripts.
- [ ] **Step 4: Run** → PASS. Then run `build-release.ps1` and `install-startup.ps1`, log off and on (or `Start-ScheduledTask PhotoSift`), and confirm `/api/health` answers and `/api/status` shows `ai_backend: "cuda"`.
- [ ] **Step 5: Commit** `feat: release build and start-at-logon scripts`

### Task 16: CI

**Files:**
- Create: `.github/workflows/ci.yml`
- Delete: `.github/workflows/screenshots.yml`, `.github/workflows/regenerate-screenshots.yml`

**Interfaces:**
- Produces: on PRs, on `windows-latest`: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (the ignored golden tests are skipped, and models are absent so the enrichment tests assert the CPU/unavailable path), `npm --prefix web ci`, `npm --prefix web run test`, `npm --prefix web run build`, and `npm --prefix web run e2e` (Playwright Chromium). On failure, upload the Playwright report.
- [ ] **Step 1:** Push a branch with the workflow and confirm it runs. Expect FAIL if any earlier task left warnings.
- [ ] **Step 2:** Fix what it finds until it's green.
- [ ] **Step 3: Commit** `ci: rust + web unit + playwright e2e on windows`

### Task 17: Lightroom Classic verification checkpoint

No code unless the checks fail. This confirms the provisional XMP mapping (spec §7, §12).

- [ ] **Step 1:** With the release build, import the golden card into a scratch library root (e.g. `E:\photos\_photosift-lr-test`). Set one photo to each outcome, mark one blooper, and give one RAF ★5 in an existing sidecar first.
- [ ] **Step 2:** In Lightroom Classic, create a scratch catalog and import that folder (Add, no move). Record what Lightroom shows for each photo: stars, color label, keywords (`PhotoSift > Blooper`), and whether the RAF+JPG pair shows as one photo or two.
- [ ] **Step 3:** In Lightroom, change one photo's star rating, then change its outcome in PhotoSift, run "Read Metadata from Files" in Lightroom, and record the result. Check that the develop settings from Step 1's existing sidecar survived.
- [ ] **Step 4:** If anything differs from the Global Constraints mapping, adjust `outcome::xmp_fields` with a failing test first, and record the findings in `docs/superpowers/specs/2026-10-19-photosift-overhaul-design.md` §7 (replacing "provisional"). Also decide whether to use `xmpDM:pick` for album/delete.
- [ ] **Step 5: Commit** `docs: record verified Lightroom XMP behaviour` (plus the fix commit if one was needed). Delete the scratch catalog and folder.

### Task 18: Remove the Tauri prototype and update docs

**Files:**
- Delete: `src-tauri/`, `src/`, `index.html`, root `package.json`, `package-lock.json`, `vite.config.ts`, `vitest.config.ts`, `tsconfig.json`, `wdio.conf.mjs`, `tests/e2e/`, `docs/design_handoff_photosift/`, `PhotoSift_FutureWork.md`
- Modify: `CLAUDE.md` (rewrite for the new architecture: layout, commands, the verification loop with Playwright, the public-repo rule, the data roots), `README.md`, `PhotoSift_Spec.md` (replace its body with a pointer to the new design doc, or delete it; pick one and say which in the commit)

- [ ] **Step 1:** Confirm nothing in `server/` or `web/` imports from the removed paths: `rg -n "src-tauri|@tauri-apps" server web` → no matches.
- [ ] **Step 2:** Delete the files, then run `cargo test --manifest-path server/Cargo.toml`, `npm --prefix web run test` and `npm --prefix web run e2e` → all PASS.
- [ ] **Step 3:** Rewrite `CLAUDE.md`. It must contain the run commands (`cargo run --manifest-path server/Cargo.toml`, `npm --prefix web run dev`), the test commands, the golden-fixture instructions, "repo is public: no photos/faces/names in git", and "verify UI changes with Playwright or the in-app browser against a running server and read the screenshots yourself".
- [ ] **Step 4: Commit** `refactor!: remove Tauri prototype; document the new server + web architecture`
