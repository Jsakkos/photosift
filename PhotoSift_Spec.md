# PhotoSift — Photo Pipeline Spec

## Overview

PhotoSift is a local-first photo pipeline tool built with Tauri (Rust + React). It covers the first two stages of a four-stage photo workflow: **Ingest → Cull/Organize → Edit → Publish**. PhotoSift handles Ingest and Cull/Organize, then hands off to Capture One or DxO for editing, and to Immich / Google Photos for publishing.

The design philosophy is speed-first, keyboard-driven, and non-destructive. All decisions are stored as metadata — no files are deleted or modified. The tool treats photo processing as a data pipeline, with structured passes that progressively refine a large set of images down to a curated, routed collection.

### Tech Stack

| Layer | Choice | Notes |
|---|---|---|
| Shell | Tauri | Native performance, cross-platform |
| Backend | Rust | File I/O, RAW preview extraction, EXIF, hashing, ONNX inference |
| Frontend | React | Keyboard-driven culling UI |
| Database | SQLite | Single file, embedded, local |
| AI (post-MVP) | ONNX Runtime | Sharpness scoring, face detection, scene classification |
| Metadata interchange | XMP sidecars | Portable ratings/labels for Capture One and DxO |

---

## Import Mode

### Sources (MVP)

- **SD card**: Detect mounted volume, copy RAW files into canonical folder structure.
- **Folder / NAS import**: Point at an existing directory of images to ingest.

### Sources (Post-MVP)

- **Phone sync**: Syncthing to a NAS landing zone, or use Immich as the phone ingest point. PhotoSift watches the landing folder and auto-ingests with minimal metadata.

### Folder Structure

On import, the user is prompted for a description slug. Files are organized by EXIF capture date:

```
DSLR/
  YYYY/
    YYYY-MM_Description/
      RAW/
        DSC_0001.NEF
        DSC_0002.NEF
        ...
```

The slug is assigned at import time via a lightweight prompt (e.g., "Greece Trip"). The year and month are extracted from EXIF data.

### Import Pipeline

For each file in the source:

1. **Copy** the RAW file into the canonical folder structure.
2. **Extract EXIF** metadata (date, camera, lens, focal length, aperture, shutter speed, ISO).
3. **Extract embedded JPEG preview** from the RAW file (full-resolution, e.g., 6016×4016 for Nikon D750 NEFs). Write to local preview cache.
4. **Generate thumbnail** (512px longest edge) for grid view. Write to local thumbnail cache.
5. **Compute perceptual hash** (pHash via DCT) on the embedded JPEG preview.
6. **Register** the photo in SQLite with all extracted metadata.
7. **Cluster** photos by perceptual hash similarity into groups.

### Deduplication

Content-based hashing (SHA-256 on file bytes) prevents importing the same file twice. Perceptual hashing (pHash) identifies near-duplicates for grouping but does not block import — similar but distinct shots are valid.

### Perceptual Hash Grouping

Similarity is measured by Hamming distance between 64-bit pHash values. Two tiers:

| Tier | Hamming Distance | Meaning | Cull Behavior |
|---|---|---|---|
| Near-duplicate | ≤ 4 | Same shot / burst / bracket | Collapsed by default, show cover only |
| Related | 5–12 | Same scene, different moment or angle | Loose group with visual separator |

Clustering: single-linkage agglomerative clustering. Walk the sorted hash list, merge any pair within threshold. For typical shoot sizes (200–500 images), this runs in milliseconds.

Groups are computed at import time and stored in the database. They can be recomputed if similarity thresholds are adjusted.

---

## Cull Mode

### Core Principles

- **Every action persists immediately.** There is no save operation. Closing the app mid-cull is a non-event.
- **Each pass answers one question.** Triage: keep or toss? Select: which is the best of similar shots? Route: edit or publish as-is?
- **Views are filters on shared state.** A "pass" is a view preset — a combination of filter, sort, group behavior, and available actions.
- **Decisions are reversible.** Undo stack per session, flag toggling in any direction, "show rejected" toggle.

### Photo State Model

Each photo carries three orthogonal attributes:

| Attribute | Values | Set During |
|---|---|---|
| `flag` | `unreviewed` · `pick` · `reject` | Triage & Select passes |
| `destination` | `unrouted` · `edit` · `publish_direct` | Route pass |
| `star_rating` | `0–5` | Set during Select when `route_min_star > 0` (optional gate); otherwise post-edit |

When the `route_min_star` setting is above 0 (default `3`), Route view only shows picks rated at or above that threshold, so rating happens in Select as part of the narrowing pass. With `route_min_star = 0`, stars remain a post-edit concept and are not part of the culling flow. Either way, a rating's "meaning" is still primarily post-edit quality — the in-cull value is just the selection gate.

### Three-Pass Workflow

#### Pass 1 — Triage

**Question**: "Is this obviously bad?"

| | |
|---|---|
| **Filter** | `flag = unreviewed` |
| **Groups** | Collapsed, cover image only. Count badge on stack. |
| **Tempo** | Fast. 1–2 seconds per image. |
| **Actions** | `P` pick · `X` reject · `Space` advance (skip) |
| **Auto-advance** | On by default (toggleable). Flash red on reject, green on pick. |

Perceptual hash groups save the most time here. A burst of 12 near-identical shots shows as one thumbnail. If the whole burst is a throwaway composition, one `X` rejects all 12.

#### Pass 2 — Select

**Question**: "Which of these similar shots is the best?"

| | |
|---|---|
| **Filter** | `flag = pick` by default (configurable via `select_requires_pick` setting; off = legacy `flag != reject`) |
| **Groups** | Expanded for comparison. |
| **Tempo** | Moderate. 5–10 seconds per group. |
| **Actions** | `P` pick (auto-rejects others in group) · `Shift+P` pick without auto-reject · `X` reject · `Tab` enter 2-up comparison |

This is where comparison mode matters. Expand a group, compare in 2-up with linked zoom/pan, pick the sharpest or best-composed, move on.

#### Pass 3 — Route

**Question**: "Does this need editing, or is it ready to publish?"

| | |
|---|---|
| **Filter** | `flag = pick, destination = unrouted, star_rating >= route_min_star` (default `3`; set to `0` to disable the star gate) |
| **Groups** | None. Groups have served their purpose. |
| **Tempo** | Moderate. Per-photo decision. |
| **Actions** | `E` mark for edit · `D` mark as publish direct · `U` reset to unrouted |

Route view shows a summary: "12 → Edit, 34 → Publish Direct, 19 unrouted." EXIF info panel visible to help assess whether exposure/WB needs correction.

### Session Persistence

Every flag, destination, and cursor position writes to SQLite on the same keystroke. The app tracks a `view_cursor` per shoot per view, so reopening a partially-culled shoot drops you exactly where you left off.

The shoot list shows status at a glance:

```
Greece Trip — June 2026
412 photos · 187 reviewed · 94 picks · 93 rejects · 225 unreviewed
Last opened: Triage view, 3 days ago
[Continue Triage]
```

### Undo

Per-session undo stack. `Cmd+Z` / `Ctrl+Z` reverses the last action, including bulk group rejects. The stack is a list of `(photo_id, field, old_value)` tuples. It persists for the session and clears on app close. For revisiting decisions made in past sessions, just toggle the flag directly.

---

## Views

### Sequential View (Default)

Single large preview, full-screen. Thin group context strip at the bottom showing thumbnails of the current group with the active image highlighted. Top bar shows shoot name, progress bar, and current view label.

### Grid View

Toggle with `G` from any pass. Configurable thumbnail size (`+` / `-` to cycle small / medium / large). Each thumbnail shows:

- Flag badge (green = pick, red = reject, gray = unreviewed)
- Destination badge if assigned
- Group stack effect (offset shadow + count badge) for cover images

Supports multi-select: `Shift+click` for range, `Cmd+click` for toggle. Bulk actions apply to selection (`X` reject all, `P` pick all). `Enter` on a selected thumbnail jumps to sequential view at that photo.

### 2-Up Comparison View

Entered with `Tab` when inside an expanded group (Select pass). Screen splits into two panels, each showing a group member. Features:

- **Linked zoom/pan**: Scroll wheel zooms both panels. Click-drag pans both. Viewport transform (x, y, scale) is shared.
- **Cycling**: Arrow keys swap which group member appears in the right panel. Left panel stays pinned.
- **Quick pick**: `1` picks left, `2` picks right (auto-rejects the other).
- Exit with `Shift+Tab` or `Escape`.

---

## Keyboard Map

### Navigation

| Key | Action |
|---|---|
| `→` / `←` | Next / previous photo in current view |
| `↑` / `↓` | Next / previous group (jump to cover) |
| `Space` | Advance to next unreviewed |
| `Home` / `End` | First / last in current view |
| `G` | Toggle grid view |
| `Enter` | Grid: jump to sequential. Sequential: expand/collapse group. |

### Flagging (Triage & Select)

| Key | Action |
|---|---|
| `P` | Pick |
| `X` | Reject |
| `U` | Reset to unreviewed |
| `Cmd+Z` | Undo |

### Routing (Route view)

| Key | Action |
|---|---|
| `E` | Mark for edit |
| `D` | Mark as publish direct |
| `U` | Reset to unrouted |

### Group Actions (Select view)

| Key | Action |
|---|---|
| `P` | Pick this photo, auto-reject others in group |
| `Shift+P` | Pick without auto-rejecting others |
| `C` | Set as group cover image |

### Comparison (2-Up)

| Key | Action |
|---|---|
| `Tab` | Enter 2-up comparison |
| `Shift+Tab` | Exit comparison |
| `1` / `2` | Pick left / right (reject the other) |
| Scroll wheel | Synced zoom |
| Click-drag | Synced pan |

### Zoom (Sequential)

| Key | Action |
|---|---|
| `Z` | Toggle fit-to-screen vs. 100% crop |
| Scroll wheel | Zoom in/out |
| Click-drag | Pan when zoomed |

### Global

| Key | Action |
|---|---|
| `Cmd+E` | Export XMP sidecars |
| `Cmd+I` | Open import dialog |
| `Escape` | Back / exit current mode |
| `?` | Show keyboard shortcut overlay |

---

## Preview Cache

All cached data is local-only. RAW folders stay clean.

```
~/.photosift/
  photosift.db
  cache/
    {shoot_id}/
      previews/           # Full-res embedded JPEGs (~2-4 MB each)
      thumbs/             # 512px longest edge (~30-50 KB each)
```

### Preloading Strategy

When viewing photo N in sequential view:

- **Forward preload**: Decode photos N+1 through N+5 into memory (pixel buffers, ready to render).
- **Backward preload**: Decode N-1 through N-3.
- **Preload window** adjusts based on available RAM.
- **Grid view**: Loads from thumbnail cache. Entire shoot's thumbnails fit comfortably in memory (~50 MB for 1000 photos).

Target: **zero perceptible load time** when advancing between photos in sequential view.

---

## XMP Export

On `Cmd+E`, PhotoSift writes XMP sidecar files alongside each RAW file for the current shoot. Sidecars are written for picks (or a configurable filter — e.g., picks marked for edit only).

### Fields Written

| XMP Field | Source | Notes |
|---|---|---|
| `xmp:Rating` | `star_rating` | 0 during culling, meaningful post-edit |
| `xmp:Label` | `flag` | Green = Pick, Red = Reject |
| `photosift:destination` | `destination` | Custom namespace: edit / publish_direct |

Both Capture One and DxO read `xmp:Rating` and `xmp:Label` natively. The custom `photosift:destination` field is for PhotoSift's own use and is ignored by editors.

### Editor Handoff

After XMP export, open a Capture One session (or DxO project) pointed at the shoot's `RAW/` folder. Ratings and labels appear automatically. The photographer edits, exports finished images to an `Export/` subfolder:

```
DSLR/2026/2026-06_Greece/
  RAW/
    DSC_0001.NEF
    DSC_0001.xmp
  Export/
    DSC_0001.jpg
```

### Publishing

- **Edit path**: RAW → Capture One / DxO → Export folder → Immich external library / Google Photos
- **Publish direct path**: PhotoSift copies the embedded JPEG preview (or applies a basic auto-adjust) to the Immich ingest path

---

## Data Model

### `shoots`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | Auto-increment |
| `slug` | TEXT | User-provided description |
| `date` | TEXT | ISO date from earliest EXIF in set |
| `source_path` | TEXT | Original import source |
| `dest_path` | TEXT | Canonical folder path |
| `photo_count` | INTEGER | Total photos in shoot |
| `imported_at` | TEXT | ISO timestamp |

### `photos`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | Auto-increment |
| `shoot_id` | INTEGER FK | References shoots.id |
| `filename` | TEXT | e.g., DSC_0001.NEF |
| `raw_path` | TEXT | Full path to RAW file |
| `preview_path` | TEXT | Path in local preview cache |
| `thumb_path` | TEXT | Path in local thumbnail cache |
| `content_hash` | BLOB | SHA-256 for deduplication |
| `phash` | BLOB | 8-byte perceptual hash |
| `exif_date` | TEXT | Capture timestamp |
| `camera` | TEXT | Camera body |
| `lens` | TEXT | Lens model |
| `focal_length` | REAL | mm |
| `aperture` | REAL | f-number |
| `shutter_speed` | TEXT | e.g., "1/250" |
| `iso` | INTEGER | |
| `flag` | TEXT | `unreviewed` · `pick` · `reject` |
| `destination` | TEXT | `unrouted` · `edit` · `publish_direct` |
| `star_rating` | INTEGER | 0–5, default 0 |
| `sharpness_score` | REAL | Nullable, post-MVP AI enrichment |

### `groups`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | Auto-increment |
| `shoot_id` | INTEGER FK | References shoots.id |
| `group_type` | TEXT | `near_duplicate` · `related` |

### `group_members`

| Column | Type | Notes |
|---|---|---|
| `group_id` | INTEGER FK | References groups.id |
| `photo_id` | INTEGER FK | References photos.id |
| `is_cover` | BOOLEAN | Cover image for collapsed display |

### `view_cursors`

| Column | Type | Notes |
|---|---|---|
| `shoot_id` | INTEGER FK | References shoots.id |
| `view_name` | TEXT | `triage` · `select` · `route` |
| `last_photo_id` | INTEGER FK | References photos.id |
| `updated_at` | TEXT | ISO timestamp |

### `undo_log`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PK | Auto-increment |
| `shoot_id` | INTEGER FK | References shoots.id |
| `session_id` | TEXT | UUID per app session, pruned on close |
| `photo_id` | INTEGER FK | References photos.id |
| `field` | TEXT | `flag` · `destination` · `star_rating` |
| `old_value` | TEXT | Previous value |
| `new_value` | TEXT | New value |
| `timestamp` | TEXT | ISO timestamp |

---

## Rust Backend Modules

### `ingest`

File copy, EXIF extraction, preview extraction, thumbnail generation, pHash computation, clustering, database registration.

**Crates**:
- `kamadak-exif` — EXIF parsing
- `rawloader` or `libraw-rs` — RAW preview extraction (test D750/NEF support)
- `image` — JPEG decode/resize for thumbnails
- `img_hash` — Perceptual hashing (DCT-based)
- `sha2` — Content hashing for dedup
- Custom clustering (single-linkage agglomerative, small enough to inline)

### `database`

SQLite operations via `rusqlite`. CRUD for all tables. Exposes a typed API to the Tauri command layer.

### `preview`

Preview cache management. Background thread pool decodes embedded JPEGs into pixel buffers. Ring buffer holds preloaded images. Manages the forward/backward preload window based on current cursor position and available memory.

### `xmp`

XMP sidecar writing. Template-based — XMP is XML, so a simple template with string substitution for rating, label, and custom fields. No heavy XML library needed.

### `ai` (Post-MVP)

ONNX Runtime integration for optional enrichment at import time:
- Sharpness scoring (Laplacian variance or learned model)
- Face detection (for filtering group/people shots)
- Scene classification (landscape, architecture, portrait, etc.)

Results stored in the photos table and surfaced as sortable/filterable attributes in culling views.

---

## Tauri IPC Commands

```rust
// --- Import ---
start_import(source_path: String, slug: String) -> Stream<ImportProgress>

// --- Shoots ---
list_shoots() -> Vec<ShootSummary>
get_shoot(shoot_id: i64) -> ShootDetail

// --- Photos ---
get_preview(photo_id: i64) -> Binary
get_thumbnail(photo_id: i64) -> Binary
preload_range(photo_ids: Vec<i64>) -> ()

// --- Culling ---
set_flag(photo_id: i64, flag: Flag) -> ()
set_destination(photo_id: i64, dest: Destination) -> ()
set_star_rating(photo_id: i64, rating: u8) -> ()
bulk_set_flag(photo_ids: Vec<i64>, flag: Flag) -> ()
set_group_cover(group_id: i64, photo_id: i64) -> ()

// --- View State ---
get_view_cursor(shoot_id: i64, view: ViewName) -> Option<i64>
set_view_cursor(shoot_id: i64, view: ViewName, photo_id: i64) -> ()

// --- Undo ---
undo(shoot_id: i64) -> Option<UndoAction>

// --- Export ---
export_xmp(shoot_id: i64, filter: ExportFilter) -> ExportResult
```

Import streams progress events via Tauri's event system (file count, current file, errors). Preview retrieval is the hot path — consider Tauri's asset protocol or shared memory to avoid serializing large image buffers through IPC.

---

## Post-MVP Roadmap

### Phone Sync

Syncthing to NAS landing zone, or Immich as phone ingest. PhotoSift watches the landing folder. Auto-generates slug from date + source tag ("Phone" / "Partner Phone"). Simplified culling flow since phone shots are rarely burst/group situations.

### Post-Edit Star Rating

PhotoSift watches the `Export/` folder for a shoot. When new exports are detected, a "Rate Exports" view shows finished images and lets you assign stars (1–5). Stars write back to XMP on both the export and the original RAW.

### AI Enrichment

Sharpness, face detection, and scene classification run at import as optional enrichment. Scores surface in culling views as sortable columns and filter criteria. AI suggests the "best" cover image for each group (sharpest + best exposed). The human always decides.

### Tag / Collection System

Subject tags (architecture, family, landscape) or event collections. Useful for cross-shoot organization but not needed while folder-based shoot structure covers the primary use case.

### Configurable State Machine

Replace hardcoded flag/destination values with a user-configurable pipeline. Define custom states, transitions, and which keystrokes map to which state. The three-pass model becomes the default config rather than the only option.
