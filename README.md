# PhotoSift

A local-first photo culling pipeline. Import RAW photos from an SD card or folder, cut them down through a three-pass keyboard-driven workflow, and hand off the survivors to Capture One / DxO for editing or to Immich for publishing. Built with Tauri (Rust + React).

## Workflow

```
  Ingest  →  Triage  →  Select  →  Route  →  Export
```

- **Triage** — `P` keep · `X` reject. Perceptual-hash bursts collapse to a single cover so 12 near-identical shots reject in one keystroke.
- **Select** — `1`–`5` star rate, `[` `]` narrow the pass floor. Multi-pass star workflow: ≥1 survives Pass 1, ≥5 is top shelf.
- **Route** — `E` send to the edit folder (Capture One / DxO import), `D` publish direct (Immich ingest).
- **Export** — XMP sidecars (Lightroom-compatible `xmp:Rating` plus a custom `photosift:destination` field) written alongside the RAW; publish-direct copies the embedded JPEG preview into the configured Immich landing folder. `xmp:Label` is deliberately not written — see `PhotoSift_Spec.md` for why.

## Shoot folder layout

Imports land under `~/Pictures/DSLR/YYYY/YYYY-MM_slug/RAW/`. As you finish each pass, PhotoSift auto-reorganises so the filesystem reflects your decisions:

```
DSLR/2026/2026-04_greece/
  RAW/       still-unreviewed
  rejects/   flag = reject
  selects/   kept, not yet routed
  edit/      routed → Capture One / DxO
  export/    routed → Immich / publish direct
```

The move is idempotent: flip a flag and the file follows on the next pass-complete transition. Pixel data, EXIF, and metadata are never mutated — only the layout changes.

## Quick start

Prereqs: Rust (latest stable), Node 20+, and the platform [Tauri prerequisites](https://tauri.app/start/prerequisites/) (WebView2 on Windows; Xcode CLTs on macOS; webkit2gtk/build-essential on Linux).

```bash
npm install
npm run tauri dev
```

To produce an installer:

```bash
npm run tauri build
```

## Keyboard

| Key | Action |
|---|---|
| `P` / `X` | Triage: keep / reject |
| `1`–`5` | Select: star rating |
| `0` | Clear rating |
| `[` / `]` | Select: lower / raise pass floor |
| `E` / `D` | Route: edit / publish direct |
| `Tab` | 2-up compare |
| `G` | Toggle grid view |
| `Space` | Advance |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo (session-scoped) |
| `T` / `F` | Toggle all-strip / faces rail |
| `H` | Toggle heatmap overlay |

## Data locations

- **Database**: `~/.photosift/photosift.db` (SQLite)
- **Preview cache**: `~/.photosift/cache/{shoot_id}/previews/` — full-res embedded JPEGs
- **Thumbnail cache**: `~/.photosift/cache/{shoot_id}/thumbs/` — 512 px longest edge
- **AI models** (optional): drop `eye_state.onnx`, `mouth_state.onnx`, `cat_detector.onnx` into `~/.photosift/models/` to swap the mocks for real classifiers. YuNet face detection is bundled.
- **Curator** (optional, cloud): aesthetic / compositional culling via Anthropic, Gemini, or a local OpenAI-compatible endpoint. BYO API key — stored in the OS keychain, set under Settings → Curator. Suggestions are stored in `curator_judgments` and surfaced as overlays; nothing is auto-applied.

## Tech stack

| Layer | Choice |
|---|---|
| Shell | Tauri v2 |
| Backend | Rust (SQLite via rusqlite, image crate, img_hash, ORT for ONNX) |
| Frontend | React 19 + TypeScript strict, Zustand, Tailwind v4 |
| Virtualisation | react-window |
| Tests | Vitest (frontend), `cargo test` (Rust) |

## Project layout

```
src-tauri/src/
  lib.rs            Tauri command registry + app setup
  ingest/           RAW copy, EXIF, preview extraction, pHash, clustering
  db/schema.rs      SQLite schema + typed API (idempotent additive migrations)
  pipeline/         Preview cache, prefetch manager, decode tiers
  metadata/         XMP sidecar read/write, write-coalescing queue
  ai/               ONNX face/eye/mouth/cat providers with mock fallbacks
  layout.rs         Auto-reorganize sync: flag/destination → bucket folder
  commands/         Tauri command handlers (thin wrappers over the above)
src/
  pages/            ShootListPage, CullPage
  components/
    chrome/         TabBar, AppShell
    primitives/     Photo, Stars, Kbd, ScoreBar, ExifChip, ColorLabel, LogoB
    triage/         TriageShell + 4-column rails
    select/         SelectShell + star-grouped / rating-peer rails
    route/          RouteShell
    LoupeView, ComparisonView, GridView, MetadataOverlay, HeatmapOverlay, ...
  stores/           Zustand: projectStore, settingsStore, aiStore
  hooks/            useKeyboardNav, useImageLoader, ...
```

## Non-destructive guarantee

File bytes, EXIF, and metadata are never mutated. Decisions are stored as SQLite rows and XMP sidecars. The shoot folder's *layout* is managed by PhotoSift — files move between `RAW/`, `rejects/`, `selects/`, `edit/`, and `export/` to mirror the cull state — but the moves are reversible and photo-ID-keyed caches mean no previews are invalidated. See `src-tauri/src/layout.rs`.

## Tests

```bash
# Frontend (Vitest)
npm run test:run

# Rust (cargo)
cd src-tauri && cargo test --lib
```

## See also

- `PhotoSift_Spec.md` — full design spec (architecture, data model, keyboard map, UX)
- `PhotoSift_FutureWork.md` — roadmap beyond the shipped MVP
- `CLAUDE.md` — notes for AI-assisted development
