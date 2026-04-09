# PhotoSift Phase 1 — Design Spec

## Context

PhotoSift is a keyboard-driven photo culling application for Windows, built with Tauri 2.x (Rust + React). The goal is zero-lag navigation through large sets of Nikon NEF RAW files, with star ratings exported as XMP sidecars compatible with Lightroom/DxO PhotoLab.

Phase 1 delivers a usable culling tool: open a folder of NEF/JPEG files, navigate instantly, rate with stars, and write XMP sidecars. AI features (face detection, scene grouping) are deferred to Phase 2+.

**Target hardware:** AMD Ryzen 7 9700X, RTX 3090, NVMe SSD, 2560x1440+ display.

---

## Architecture

**Stack:** Tauri 2.x, Rust backend, React 18 + TypeScript + Zustand + Tailwind CSS frontend.

**Display strategy:** Custom Protocol (Approach A). Rust decodes RAW files and serves display-ready JPEGs via Tauri's custom protocol handler (`photosift://image/{id}?tier=embedded|preview|full`). The webview displays images via standard `<img>` elements. wgpu is integrated from Phase 1 for GPU-accelerated demosaicing and image processing in the Rust backend. The webview handles display; wgpu handles compute.

**Why custom protocol over IPC:** Tauri's `invoke()` serializes as JSON/base64, adding ~33% overhead. The custom protocol streams raw bytes directly (~5ms vs ~200ms for a 3MB JPEG).

**RapidRAW lessons applied:**
- Keep decoded image data in Rust memory, serve display-ready frames to webview
- LRU cache for instant navigation between recently viewed images
- Direction-aware prefetch during decode
- Decode once, cache in memory, never re-decode on revisit

---

## Project Structure

```
photosift/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs                 # Tauri entry point
│   │   ├── lib.rs                  # Module declarations
│   │   ├── commands/               # Tauri IPC command handlers
│   │   │   ├── mod.rs
│   │   │   ├── project.rs          # Open folder, scan files
│   │   │   ├── image.rs            # Image navigation, metadata queries
│   │   │   └── rating.rs           # Star rating, XMP write
│   │   ├── pipeline/               # Image decode & cache
│   │   │   ├── mod.rs
│   │   │   ├── decoder.rs          # rawler + image crate decoding
│   │   │   ├── embedded.rs         # Embedded JPEG extraction from RAW
│   │   │   ├── prefetch.rs         # Sliding window prefetch manager
│   │   │   ├── cache.rs            # LRU decode cache (~20 images)
│   │   │   └── protocol.rs         # Tauri custom protocol handler
│   │   ├── metadata/
│   │   │   ├── mod.rs
│   │   │   ├── exif.rs             # EXIF extraction (kamadak-exif)
│   │   │   └── xmp.rs              # XMP sidecar read/write (quick-xml)
│   │   └── db/
│   │       ├── mod.rs
│   │       └── schema.rs           # SQLite schema + queries (rusqlite)
│   └── icons/
├── src/                             # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── LoupeView.tsx           # Main single-image display
│   │   ├── Filmstrip.tsx           # Virtualized horizontal thumbnails
│   │   ├── Toolbar.tsx             # Minimal toolbar
│   │   ├── MetadataOverlay.tsx     # EXIF info overlay (toggleable)
│   │   ├── RatingBar.tsx           # Star rating controls
│   │   └── ShortcutHints.tsx       # Keyboard shortcut overlay (? key)
│   ├── hooks/
│   │   ├── useKeyboardNav.ts       # Arrow key navigation
│   │   ├── useImageLoader.ts       # Image loading via custom protocol
│   │   └── useRating.ts            # Rating state + backend calls
│   ├── stores/
│   │   └── projectStore.ts         # Zustand: project state, current image, ratings
│   ├── types/
│   │   └── index.ts
│   └── styles/
│       └── globals.css             # Tailwind + dark theme
├── package.json
├── tsconfig.json
├── tailwind.config.js
└── vite.config.ts
```

### Key Rust Crates

| Crate | Purpose |
|---|---|
| `rawler` | RAW decode (NEF) |
| `image` | JPEG/TIFF decode, thumbnail generation |
| `kamadak-exif` | EXIF metadata extraction |
| `quick-xml` | XMP sidecar read/write |
| `rusqlite` | SQLite cache (bundled feature) |
| `rayon` | Parallel processing for scan/decode |
| `lru` | LRU cache for decoded images |
| `tauri` | Application framework (v2) |

### Frontend Dependencies

| Package | Purpose |
|---|---|
| `react` + `react-dom` | UI framework |
| `zustand` | State management |
| `react-window` | Virtualized filmstrip |
| `tailwindcss` | Styling |
| `@tauri-apps/api` | IPC + event system |

---

## Image Decode & Display Pipeline

### Three-Tier Preview Strategy

| Tier | Source | Resolution | Latency | Usage |
|---|---|---|---|---|
| Embedded JPEG | RAW file header | ~1600px long edge | < 5ms | Instant display, always first |
| Decoded preview | rawler half-res decode | ~3000px | 50-200ms (0ms if cached) | Replaces embedded JPEG |
| Full resolution | rawler full decode | Native (6000x4000) | 200-500ms | On spacebar zoom to 100% |

### Custom Protocol

```
photosift://image/{image_id}?tier={embedded|preview|full}
photosift://thumb/{image_id}
```

Tauri registers a custom protocol handler in Rust. Frontend loads images via `<img src="...">`. Rust handler checks cache, decodes if needed, returns JPEG bytes. No serialization overhead.

### Prefetch Manager

- Sliding window of **+/-5 images** around current position
- **Direction-aware:** if navigating forward, prioritize N+1..N+5 over N-1..N-5
- **LRU cache:** ~20 decoded preview JPEGs in memory (~60MB total)
- On project open: extract all embedded JPEGs in parallel (rayon)
- On navigation to image N:
  1. Display Tier 1 (embedded JPEG) instantly
  2. Check LRU cache for Tier 2 -> if hit, swap immediately
  3. If miss, queue Tier 2 decode (high priority)
  4. Queue prefetch for surrounding window

### Image Swap Strategy (Zero-Flash)

Two `<img>` elements stacked via absolute positioning:
1. Current image is visible
2. Next image loads behind it (hidden)
3. On `onload` of next image: swap visibility via CSS class toggle
4. Same mechanism for Tier 1 -> Tier 2 upgrade

---

## Loupe View

```
+--------------------------------------------------+
| [Toolbar: Project name | 142/500 | Sort]          |
+--------------------------------------------------+
|                                                    |
|                                                    |
|             Main Image (<img> tag)                 |
|         object-fit: contain, fills viewport        |
|                                                    |
|                                                    |
|                         [EXIF overlay, top-right]  |
|                         f/2.8 - 1/200 - ISO 400   |
+--------------------------------------------------+
| [filmstrip: < . . . . [*] . . . . . . . . . . >]  |
| [rating:    * * * * *       142 / 500 images    ]  |
+--------------------------------------------------+
```

### Zoom

- **Spacebar:** Toggle between fit-to-view and 100% at cursor position
- CSS `transform: scale()` + `transform-origin` at cursor coordinates
- At 100%, request Tier 3 (full-res) via custom protocol
- Pan via mouse drag (CSS transform translation)

### Metadata Overlay

Toggleable with `I` key. Shows in top-right corner:
- Filename
- Capture time
- Focal length, aperture, shutter speed, ISO
- Camera model

---

## Filmstrip

- **Virtualized** via `react-window` (FixedSizeList, horizontal layout)
- Thumbnail height: ~80px, aspect-ratio preserved
- Served via `photosift://thumb/{image_id}` (200px JPEG from SQLite)
- Current image: bright accent border
- Star rating: small colored dots below thumbnail
- Click to navigate
- Auto-scrolls to keep current image centered
- Must handle 5000+ thumbnails without performance issues

---

## Rating System

### Phase 1 Scope: Star Ratings Only

| Key | Action |
|---|---|
| `1`-`5` | Set star rating |
| `0` | Clear star rating |

**Auto-advance:** ON by default. After rating, advance to next image. Toggleable in toolbar.

### Flow

1. Keypress -> Zustand store updates immediately (optimistic UI)
2. `set_rating` Tauri command sent to Rust backend
3. Rust: update SQLite cache + queue XMP sidecar write
4. XMP write is **debounced 100ms** per image (rapid rating changes coalesce)
5. Filmstrip rating indicator updates reactively

### XMP Sidecar

For `DSC_1234.NEF`, creates/updates `DSC_1234.xmp`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description
      xmlns:xmp="http://ns.adobe.com/xap/1.0/"
      xmp:Rating="3">
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
```

**Merge behavior:** If sidecar already exists, parse with `quick-xml`, update only `xmp:Rating`, preserve all other content.

### Undo/Redo

- In-memory stack of `(image_id, field, old_value, new_value)` tuples
- `Ctrl+Z` / `Ctrl+Shift+Z`
- Stack depth: 50 actions
- Lost on app close (acceptable for Phase 1)

---

## Keyboard Shortcuts

All defaults for Phase 1:

| Action | Key |
|---|---|
| Next image | Right Arrow / Down Arrow |
| Previous image | Left Arrow / Up Arrow |
| Star rating 1-5 | `1`, `2`, `3`, `4`, `5` |
| Clear stars | `0` |
| Zoom toggle (100%) | Spacebar |
| Toggle metadata | `I` |
| Show shortcut hints | `?` |
| Undo | `Ctrl+Z` |
| Redo | `Ctrl+Shift+Z` |

Shortcut customization UI is deferred to Phase 4. Defaults are hardcoded in Phase 1.

---

## Data Layer

### Project Management

**Open flow:**
1. User selects folder (native OS dialog or drag-and-drop)
2. Rust scans for `*.nef`, `*.jpg`, `*.jpeg`, `*.tif`, `*.tiff` (case-insensitive)
3. Creates `.photosift/` inside the project folder:
   - `project.json` — metadata, last-viewed index
   - `cache.sqlite` — thumbnails, EXIF, ratings
4. Parallel initial processing (rayon):
   - Extract embedded JPEGs from NEF files
   - Generate 200px thumbnails -> SQLite BLOB
   - Extract EXIF metadata -> SQLite
   - Read existing XMP sidecars -> import ratings
5. Frontend receives incremental updates via Tauri events
6. User can browse immediately (~1-2s after open)

**Re-open:** Detect `.photosift/`, diff file list vs SQLite, process only new/changed files, resume at last position.

### SQLite Schema

```sql
CREATE TABLE images (
    id INTEGER PRIMARY KEY,
    filepath TEXT UNIQUE NOT NULL,
    filename TEXT NOT NULL,
    file_hash TEXT,              -- SHA-256 of first 64KB (change detection)
    file_size INTEGER,
    capture_time TEXT,           -- ISO 8601
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
    thumbnail_blob BLOB,        -- 200px JPEG
    embedded_preview_path TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_images_capture_time ON images(capture_time);
CREATE INDEX idx_images_sort_order ON images(sort_order);
```

---

## Verification Plan

### Manual Testing

1. **Project open:** Select a folder with 500+ NEF files. Verify thumbnails appear incrementally in the filmstrip within 5 seconds.
2. **Navigation speed:** Hold right arrow key and scrub through images. Target: zero perceptible lag for cached images, embedded JPEG appears within one frame for uncached.
3. **Rating round-trip:** Rate an image with `3`, close the app, verify `.xmp` sidecar exists with `xmp:Rating="3"`. Re-open project, verify rating persists.
4. **XMP merge:** Place an existing XMP sidecar (from DxO/Lightroom) next to a NEF file. Rate the image. Verify original XMP content is preserved, only rating field updated.
5. **Zoom:** Press spacebar, verify 100% zoom at cursor position. Pan with mouse drag. Press spacebar again to return to fit view.
6. **Undo:** Rate image `3`, then `Ctrl+Z`, verify rating reverts to previous value.
7. **Large project:** Open a folder with 2000+ images. Verify filmstrip virtualizes correctly (no DOM bloat), navigation stays smooth.

### Automated Testing

- Rust unit tests: decoder (NEF -> JPEG), EXIF extraction, XMP read/write/merge, LRU cache behavior, prefetch queue logic
- Integration test: open a test folder with sample NEF + JPEG files, verify SQLite population
- Frontend: component tests for rating bar, keyboard handler

---

## Out of Scope (Phase 2+)

- AI/ML features (face detection, eye assessment, focus scoring, scene grouping)
- Grid View, Scenes View, Survey Mode
- Color labels and flags
- Filter/sort toolbar
- Export/reject separation workflow
- Customizable keyboard shortcuts UI
- Fujifilm RAF support
- wgpu display rendering surface (wgpu is used for backend compute/demosaic, not webview display)
- Preferences panel
