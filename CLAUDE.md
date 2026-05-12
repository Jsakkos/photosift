# CLAUDE.md — PhotoSift

## What This Is

PhotoSift is a local-first photo pipeline tool: import RAW photos, cull them through a structured three-pass workflow (triage → select → route), and export XMP sidecars for handoff to Capture One or DxO. Built with Tauri v2 (Rust backend + React/TypeScript frontend).

Read `PhotoSift_Spec.md` for the full design spec. It is the source of truth for architecture, data model, keyboard map, and UX behavior.

## Tech Stack

- **Tauri v2** — app shell, IPC between Rust and React
- **Rust** — all file I/O, image processing, database, and heavy lifting
- **React + TypeScript** — frontend UI
- **SQLite via rusqlite** — single database at `~/.photosift/photosift.db`
- **Tailwind CSS** — styling

### Key Rust Crates

- `kamadak-exif` — EXIF parsing
- `rawloader` (or `libraw-rs` if NEF support is better) — RAW embedded JPEG preview extraction
- `image` — JPEG decode/resize for thumbnail generation
- `img_hash` — perceptual hashing (DCT-based pHash)
- `sha2` — content hashing for deduplication
- `rusqlite` — SQLite with bundled feature

## Project Structure

```
src-tauri/
  src/
    main.rs
    lib.rs
    ingest.rs        # File copy, EXIF, preview extraction, pHash, clustering
    database.rs      # All SQLite operations, typed API
    preview.rs       # Preview cache management, preloading, memory pool
    xmp.rs           # XMP sidecar writing (template-based XML)
    ai.rs            # Post-MVP: ONNX Runtime sharpness/face/scene scoring
src/
  App.tsx
  pages/
    ShootList.tsx    # Home: list of imported shoots with status
    Import.tsx       # Import dialog: folder picker + slug prompt + progress
    Cull.tsx         # Culling shell: hosts triage/select/route views
  components/
    SequentialView/  # Single-image view with group context strip
    GridView/        # Thumbnail grid with flag/destination badges
    ComparisonView/  # 2-up linked zoom/pan for group comparison
    TopBar.tsx       # Shoot name, progress, breakdown bar, view label
    GroupStrip.tsx   # Thumbnail strip of current group members
```

## File Locations

- **Database**: `~/.photosift/photosift.db`
- **Preview cache**: `~/.photosift/cache/{shoot_id}/previews/` (full-res embedded JPEGs)
- **Thumbnail cache**: `~/.photosift/cache/{shoot_id}/thumbs/` (512px longest edge)
- **Imported photos**: land in `DSLR/YYYY/YYYY-MM_Description/RAW/` (user-configured root). As the user completes each pass, `sync_shoot_layout` relocates them into bucket folders: `RAW/rejects/`, `RAW/selects/{0,1,2,3+}/` (unrouted picks, partitioned by star floor — `3+` collapses 3-5), `RAW/edit/` (all nested under `RAW/`), and a top-level `Export/` for `destination='export'` picks (so publishable artifacts sit apart from the working RAW tree). A sibling JPEG, if present, follows its RAW into whichever bucket. `raw_path` in the DB always tracks the current on-disk location. **The path template (`{root}/DSLR/{year}/{year-month}_{slug}` by default) and the five bucket folder names are configurable in Settings — see `src-tauri/src/folder_template.rs` (the `selects/{0,1,2,3+}` star bins are not configurable, just the `selects` parent name). The setting is global: the path template applies to new imports; bucket names are read on every layout sync, so renaming a bucket relocates its files next sync.**
- **XMP sidecars**: every move (re)writes a fresh XMP alongside the RAW at its new location, reflecting the current `(star_rating, flag, destination)`. Stale sidecars at the previous location are cleaned up as part of the move.

## Data Model

Six tables: `shoots`, `photos`, `groups`, `group_members`, `view_cursors`, `undo_log`. Full schema is in the spec. Key points:

- Photos have three orthogonal attributes: `flag` (unreviewed/pick/reject), `destination` (unrouted/edit/export), `star_rating` (0-5, not used during culling).
- Groups are perceptual hash clusters computed at import. Two tiers: near-duplicate (hamming ≤4) and related (hamming 5-12).
- `view_cursors` stores resume position per shoot per view.
- `undo_log` is append-only during a session, pruned on app close.

## Architecture Rules

- **Every culling action persists immediately to SQLite.** No save button, no unsaved state. The user can close the app at any time and resume later.
- **Keyboard-first.** Every action has a keystroke. Mouse/trackpad is supported but not required.
- **Non-destructive data, layout-managing.** Pixel data, EXIF, and metadata are never mutated. The *layout* under a shoot folder IS managed by PhotoSift: as the user finishes triage/select/route, `sync_shoot_layout` moves RAWs (and their sibling JPEGs, if any) into buckets that reflect `(flag, destination, star_rating)` — `RAW/`, `RAW/rejects/`, `RAW/selects/{0,1,2,3+}/` (unrouted picks binned by star floor), `RAW/edit/` nested under `RAW/`, plus a top-level `Export/` for `destination='export'` — and writes a fresh XMP sidecar at each new location. The sidecar carries `xmp:Rating` + `photosift:destination` only — no `xmp:Label`, because Capture One and DxO interpret that as a color tag the user never chose. Moves are idempotent and reversible — flip the metadata, re-run sync, the file and its sidecar follow. See `src-tauri/src/layout.rs` for the bucket map and trigger gating.
- **Preview hot path.** The Rust preview module preloads N+1..N+5 forward and N-1..N-3 backward as decoded pixel buffers. Target is zero perceptible load time on advance. This is the most performance-critical code path.
- **IPC is typed.** Tauri commands return typed structs, not raw JSON. Define shared types in `src-tauri/src/` and mirror them in TypeScript.

## Three-Pass Culling Model

Each "pass" is a view preset (filter + available actions), not a separate mode:

| View | Filter | Groups | Primary Keys |
|---|---|---|---|
| Triage | `flag = unreviewed` | Collapsed | `P` pick, `X` reject, `Space` next |
| Select | `flag != reject` | Expanded + comparison | `P` pick (auto-rejects group), `Tab` 2-up |
| Route | `flag = pick` | None | Mouse-driven: select photos, pick Capture One / Export from the Route dropdown. No per-view hotkeys; `Ctrl+E` still exports XMP sidecars. |

## Common Tasks

```bash
# Dev
cargo tauri dev

# Build
cargo tauri build

# Run SQLite queries for debugging
sqlite3 ~/.photosift/photosift.db

# Test import with real files
# Point at a folder of .NEF files from a Nikon D750
```

## Testing Notes

- **Always verify your own work before reporting it done or asking the user to test.** Use a layered approach, in order: (1) `cargo test` + `npm run test:run` + `npx tsc --noEmit` for any code change; (2) for UI changes, drive the running app via `tauri-mcp` (`cargo tauri dev` — or run `cargo run --no-default-features` from `src-tauri` if vite is already up on :1420 — then use the `mcp__tauri-mcp__*` tools to navigate, exercise the feature, and screenshot the key states), or fall back to native screenshots if the bridge won't connect; (3) for backend/filesystem behavior, exercise it via tests or a CLI repro. Only after that should you summarize what you actually observed. Don't say "this works" or "done" on the strength of the diff alone, and don't push verification onto the user when you can run it yourself.
- Always test import with real D750 NEF files. The embedded JPEG preview extraction and pHash computation depend on the specific RAW format.
- Perceptual hash grouping thresholds (≤4 near-duplicate, 5-12 related) may need tuning with real-world bursts. Make thresholds configurable constants, not magic numbers.
- Preview preloading should be tested with shoots of 200+ images to verify memory behavior.
- Keyboard handling must work when the image preview has focus. Watch for focus stealing.

## Style & Code Conventions

- Rust: standard `rustfmt`, `clippy` clean. Error handling with `anyhow` in commands, `thiserror` for module-specific errors.
- TypeScript: strict mode, no `any`. Components are functional with hooks.
- Commits: conventional commits (`feat:`, `fix:`, `refactor:`).
- No AI-generated comments explaining obvious code. Comments explain *why*, not *what*.
