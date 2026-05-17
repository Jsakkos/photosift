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
| AI (on-device) | ONNX Runtime | Sharpness (Laplacian variance), YuNet face detection, eye/mouth state, cat heuristic. Scene classification still future. |
| AI (cloud) | Curator subsystem | Optional aesthetic/compositional culling via Anthropic, Gemini, or local OpenAI-compatible providers. Suggestion-only. |
| Metadata interchange | XMP sidecars | `xmp:Rating` + custom `photosift:destination` written for Capture One / DxO. `xmp:Label` is **not** written — see XMP Export below. |

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

Similarity is measured by Hamming distance between 64-bit pHash values.
Single tier: any two photos within one user-tunable threshold (default 16,
also gated by a capture-time window) form an edge; groups are the
transitive closure. Every photo belongs to at most one group.

> **Divergence note (code wins):** earlier drafts of this spec defined two
> tiers (near-duplicate ≤ 4, related 5–12). The implemented model collapsed
> to a single tier — grouping only needs to be "good enough" to funnel
> similar frames into the Select tournament, and one tunable threshold is
> simpler to reason about and retune per shoot.

Clustering: union-find over the photo pairs within threshold. For typical
shoot sizes (200–500 images), this runs in milliseconds.

Groups are computed at import time and stored in the database. They can be
recomputed (per shoot) if the similarity threshold is adjusted — the Select
view's inline "regroup" control does exactly this.

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
| `flag` | `unreviewed` · `pick` · `reject` | Triage (primary) and Select rejections |
| `destination` | `unrouted` · `edit` · `publish_direct` | Route pass |
| `star_rating` | `0–5` | Primary action during Select. Stars express iterative promotion through rating passes (1★ = survived Pass 1, 5★ = top shelf). |

Triage is strictly `pick` / `reject` — no stars, no ambiguity. Select is strictly star-rating driven (plus `X` to demote a photo out of the pick pool). Route view optionally gates on `route_min_star` (default `3`); set to `0` to disable the gate and treat stars as post-edit quality signal only.

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

**Question**: "Of my kept photos, which deserve more attention? Which are the best of the best?"

| | |
|---|---|
| **Filter** | `flag = pick AND star_rating >= select_min_star` (default floor `0`; raised one tier at a time via `]` or the pass chips) |
| **Groups** | Expanded for comparison. |
| **Tempo** | Moderate. Iterative — multiple passes, each narrower than the last. |
| **Actions** | `1`–`5` rate (and promote) · `0` clear rating · `X` reject (demotes out of Select) · `[` / `]` decrement / increment the pass floor · `Tab` enter 2-up comparison |

Select is an iterative pass, modeled on the Lightroom "rate-then-raise-the-floor" workflow. Start at Pass 1 (floor `0`) and rate anything you want to keep at 1★. Press `]` to jump to Pass 2 (floor `1`) — now only 1★+ photos are visible, and you rate the best of those up to 2★. Repeat up to 5★ for your top shelf. Each pass is narrower than the last; stars are both the rating and the gesture that moves a photo into the next pass.

When every visible photo has already been rated above the current floor, PhotoSift auto-advances the floor one tier so the user doesn't have to press `]` manually. `[` steps back to a lower floor for second thoughts.

When the Select pass completes, `sync_shoot_layout` partitions the unrouted picks on disk into `RAW/selects/{0,1,2,3+}/` by their current star floor (`3+` collapses 3–5, since the Route pass treats ★≥3 as a clear keeper). The folders mirror the in-app passes, so a decision made in an earlier round stays visible — and out of the way — outside the app. Re-rating a photo moves it to the matching bin on the next sync.

`Tab` opens 2-up comparison with linked zoom/pan. Inside comparison, `1` picks the left panel and `2` picks the right (auto-rejects the other) — this is a burst-disambiguation shortcut distinct from the non-comparison `1`–`5` rating keys.

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

### Flagging (Triage)

| Key | Action |
|---|---|
| `P` | Pick |
| `X` | Reject |
| `U` | Reset to unreviewed |
| `Cmd+Z` | Undo |

### Rating (Select)

| Key | Action |
|---|---|
| `1`–`5` | Rate current photo (and promote to next pass) |
| `0` | Clear rating |
| `X` | Reject (demotes out of Select) |
| `[` / `]` | Decrement / increment the pass floor |
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
| `C` | Set as group cover image |
| `Shift+A` | Accept AI's suggested photo as group cover |

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
| `photosift:destination` | `destination` | Custom namespace; written as `edit` or `export` |

`xmp:Rating` is read natively by Capture One and DxO. The custom `photosift:destination` field is for PhotoSift's own use and is ignored by editors.

**`xmp:Label` is deliberately not written.** Capture One and DxO interpret `xmp:Label` as a user-chosen color tag — writing one from a pick/reject flag would pollute the user's color-tag palette with a value they never asked for. The pick/reject decision is fully carried by the file's physical location (`RAW/` vs `RAW/rejects/`, etc.) and by `photosift:destination`. User-set `xmp:Label` values in pre-existing sidecars are passed through untouched. Enforced in `src-tauri/src/metadata/xmp.rs` (see comment at line 107 and the regression test at line 418).

### Editor Handoff

After XMP export, open a Capture One session (or DxO project) pointed at the shoot's `RAW/` folder. Ratings and labels appear automatically. The photographer edits, exports finished images to the shoot's top-level `Export/` folder. PhotoSift itself also uses `Export/`: picks routed to `destination='export'` (and their sibling JPEGs, if any) are moved there by `sync_shoot_layout`, so the folder collects everything publishable for the shoot:

```
DSLR/2026/2026-06_Greece/
  RAW/
    DSC_0001.NEF
    DSC_0001.xmp
  Export/
    DSC_0002.NEF        # routed to export by PhotoSift
    DSC_0002.JPG        # sibling JPEG, followed the RAW
    DSC_0002.xmp
    DSC_0001.jpg        # finished export from Capture One / DxO
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

### Post-MVP tables

The schema has grown beyond the original six. These tables are added by idempotent migrations in `src-tauri/src/db/schema.rs`:

- **`settings`** — single-row key/value store for user-configurable settings: import path template, bucket folder names, per-view filter gates (`select_requires_pick`, `route_min_star`), first-run dismissal flags, curator-provider preference, etc.
- **`file_moves`** — audit trail of layout-sync operations (which RAW moved from where to where, when). Used to reverse moves and to drive the `?` shortcuts overlay's recent-activity panel.
- **`curator_judgments`** — append-only record of cloud-LLM verdicts: per photo, per provider, with confidence and reasoning. Powers `get_curator_agreement_stats` and the curator audit UI.
- **`faces`** — face crop metadata produced by the on-device AI worker: bounding box, landmarks, eye state, mouth state, sharpness — keyed on `photo_id`.

See `src-tauri/src/db/schema.rs` for the authoritative column list.

---

## Rust Backend Modules

The backend has outgrown the original five single-file modules. Current layout in `src-tauri/src/`:

### `ingest/`

File copy, EXIF extraction, embedded JPEG preview extraction, thumbnail generation, pHash, single-linkage clustering, sibling-JPEG pairing, walker, progress tracking.

**Crates**: `rawler` (NEF), `kamadak-exif`, `image` (JPEG decode/resize), custom pHash, `sha2` (content hash dedup).

### `db/`

`schema.rs` is authoritative for the table list — idempotent additive migrations, typed CRUD via `rusqlite` (bundled).

### `pipeline/`

Preview cache, prefetch manager (N+1..N+5 forward, N-1..N-3 backward), decoder tiers, embedded-preview extraction, and the `photosift://` Tauri custom protocol handler that streams previews into the webview without IPC serialization.

### `metadata/`

XMP read/write via `quick-xml`, EXIF parsing, orientation handling, and a write-coalescing `xmp_queue` that batches updates so rapid rating changes don't thrash the disk.

### `ai/`

ONNX Runtime (`ort` with `cuda` feature) wrappers behind provider traits:
- `face` — YuNet, bundled
- `eye_onnx` / `mouth_onnx` — drop-in ONNX classifiers in `~/.photosift/models/`, fall back to `mock.rs` if absent
- `cat` — heuristic cat detector
- `sharpness` — Laplacian variance
- `worker` — background analysis thread with cancel/progress atomics

Suggest-only by design — never auto-applies a verdict.

### `curator/`

Optional cloud LLM aesthetic culling. `CuratorProvider` trait with three implementations: Anthropic (Claude), Gemini, and local OpenAI-compatible (`api_anthropic.rs`, `api_gemini.rs`, `api_local.rs`). Cost estimation, prompts, worker, per-provider API keys in the OS keychain via `keyring`. Judgments stored in `curator_judgments` for audit and agreement stats.

### `commands/`

Thin Tauri command wrappers organized one-file-per-domain (`shoots`, `import`, `scan`, `drives`, `image`, `rating`, `culling`, `settings`, `export`, `ai`, `layout`, `curator`). 56 commands total.

### `layout.rs` / `folder_template.rs`

`sync_shoot_layout` is the layout-management heart. Maps `(flag, destination, star_rating)` → bucket folder (`RAW/`, `RAW/rejects/`, `RAW/selects/{0,1,2,3+}/`, `RAW/edit/`, top-level `Export/`) and moves RAWs + sibling JPEGs + their XMP sidecars on every transition. Idempotent and reversible. `folder_template.rs` holds the user-configurable path template + bucket names (the `selects/{0,1,2,3+}` star bins themselves are fixed).

### `drives/`

Removable-drive detection for SD-card import.

### `state.rs`

`AppState` carries shared atomics (AI cancel flag, AI/curator analyzed/failed/total counters) and the XMP write queue.

---

## Tauri IPC Commands

**56 commands** are registered in the `invoke_handler` block of `src-tauri/src/lib.rs` — the authoritative list. Grouped by domain:

| Domain | Count | Examples |
|---|---|---|
| Shoots | 3 | `list_shoots`, `get_shoot`, `delete_shoot` |
| Import | 2 | `start_import`, `cancel_import` |
| Scan | 2 | `scan_folder`, `extract_thumbnails_for_paths` |
| Drives | 1 | `list_removable_drives` |
| Image | 2 | `get_image_list`, `get_image_metadata` |
| Rating | 1 | `set_rating` |
| Culling | 9 | `set_flag`, `set_destination`, `bulk_set_flag`, `undo_last`, view cursors, group CRUD |
| Settings | 3 | `get_settings`, `update_settings`, `recluster_shoot` |
| Export | 1 | `export_publish_direct` |
| AI | 6 | `get_ai_status`, `cancel_ai_analysis`, `reanalyze_shoot`, `get_faces_for_photo`, `get_heatmap`, `get_shoot_sharpness_percentiles` |
| Layout | 5 | `sync_layout_if_eligible`, `mark_photo_visited_in_select`, `bump_select_max_floor`, `get_shoot_bucket_path`, `open_shoot_folder` |
| Curator | 17 | per-provider key management (Anthropic, Curator, Local), `test_*_connection`, `start_curator_for_shoot`, `cancel_curator`, `resume_curator_for_shoot`, `get_curator_status`, `get_curator_judgments_for_shoot`, `get_curator_summary`, `get_curator_agreement_stats`, `estimate_curator_cost_cents`, `accept_curator_suggestion`, `record_curator_override` |

Import streams progress events via Tauri's event system (file count, current file, errors). Preview retrieval bypasses the JSON IPC entirely — it goes through a custom `photosift://` Tauri protocol handler registered in `src-tauri/src/pipeline/protocol.rs` so previews are streamed directly into the webview without serializing large pixel buffers.

Command handlers return typed structs, not raw JSON. Shared types are defined in Rust and mirrored in `src/types/` on the TypeScript side.

---

## Post-MVP Roadmap

### Phone Sync

Syncthing to NAS landing zone, or Immich as phone ingest. PhotoSift watches the landing folder. Auto-generates slug from date + source tag ("Phone" / "Partner Phone"). Simplified culling flow since phone shots are rarely burst/group situations.

### Post-Edit Star Rating

PhotoSift watches the `Export/` folder for a shoot. When new exports are detected, a "Rate Exports" view shows finished images and lets you assign stars (1–5). Stars write back to XMP on both the export and the original RAW.

### AI Enrichment — *shipped*

On-device AI runs at import as optional enrichment, with results stored in the `faces` table and surfaced in culling views:

- **Sharpness** — Laplacian variance via `ai/sharpness.rs`. Per-shoot percentiles computed for relative scoring.
- **Face detection** — YuNet (bundled) via `ai/face.rs`. Returns boxes + landmarks. Heatmap overlay (`H` key) visualizes detections.
- **Eye state, mouth state, cat heuristic** — drop-in ONNX classifiers. Place `eye_state.onnx`, `mouth_state.onnx`, `cat_detector.onnx` in `~/.photosift/models/` to enable; falls back to mocks if absent or if ORT init fails.
- **CUDA** — optional via `ort`'s `cuda` feature. Provider DLL is bundled; cuBLAS / cuDNN must be supplied separately (extract from `nvidia-*-cu12` pip wheels).
- **AI suggests, never decides.** Group cover suggestions, sort orders, and pick badges are surfaced in the UI but never auto-applied.

**Scene classification** (landscape, architecture, portrait) is still future work — not implemented in the current AI module.

### Curator (cloud LLM aesthetic culling) — *shipped*

Optional cloud-LLM layer for compositional / aesthetic judgments, complementary to the on-device technical-quality AI. Implemented in `src-tauri/src/curator/`:

- **Providers behind a trait** — `CuratorProvider` with Anthropic (Claude), Gemini, and local OpenAI-compatible (e.g. self-hosted Ollama / llama.cpp) implementations. Adding a provider means implementing one trait.
- **API keys in OS keychain** — per-provider, via the `keyring` crate. Service name `photosift`; never returned to the frontend after write.
- **Cost-aware** — `estimate_curator_cost_cents` runs before the worker so the user sees expected spend; the worker supports cancel and resume.
- **Suggestion-only** — judgments are stored in `curator_judgments` for review. The user accepts (`accept_curator_suggestion`) or overrides (`record_curator_override`) explicitly; nothing is applied automatically. Agreement stats compare curator verdicts against the user's eventual decisions for calibration.

### Tag / Collection System

Subject tags (architecture, family, landscape) or event collections. Useful for cross-shoot organization but not needed while folder-based shoot structure covers the primary use case.

### Configurable State Machine

Replace hardcoded flag/destination values with a user-configurable pipeline. Define custom states, transitions, and which keystrokes map to which state. The three-pass model becomes the default config rather than the only option.
