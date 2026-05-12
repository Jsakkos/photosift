# CLAUDE.md — PhotoSift

## What This Is

PhotoSift is a local-first photo pipeline tool: import RAW photos, cull them through a structured three-pass workflow (triage → select → route), enrich with on-device AI (faces, eyes, sharpness) and optional cloud LLM Curator, then hand off to Capture One / DxO via XMP sidecars or publish direct to Immich. Tauri v2 (Rust backend + React/TypeScript frontend).

Other docs:
- `PhotoSift_Spec.md` — design-of-record (architecture, data model, keyboard map). Source of truth for *intent*.
- `PhotoSift_FutureWork.md` — open items + shipped log.
- `README.md` — user/contributor-facing overview.

When the spec and the code disagree, the code wins — flag the divergence rather than implement to the spec.

## Tech Stack

| Layer | Choice |
|---|---|
| Shell | Tauri v2 |
| Backend | Rust — SQLite (rusqlite, bundled), `rawler` for NEF, `kamadak-exif`, `image`, `img_hash` style pHash (custom), `sha2`, `ort` (ONNX Runtime, CUDA-capable), `quick-xml`, `reqwest` (rustls), `keyring` |
| Frontend | React 19 + TypeScript strict, Zustand, Tailwind v4, react-window |
| Tests | Vitest (frontend), `cargo test --lib` (Rust), WebdriverIO + tauri-driver (e2e) |
| Dev MCP | `tauri-plugin-mcp` on `127.0.0.1:4000` in debug builds for Claude-driven UI verification |

## Repo Layout

### `src-tauri/src/` (Rust backend)

Top-level modules:
- `lib.rs` — Tauri builder, AI provider bootstrap, `invoke_handler` for **56 commands**
- `state.rs` — `AppState` (AI/curator progress atomics, XMP write queue)
- `layout.rs` — bucket-folder mapper; moves RAWs + sibling JPEGs as `(flag, destination, star_rating)` changes
- `folder_template.rs` — configurable path template + bucket names

Module directories:
- `ai/` — on-device AI: YuNet face detection, eye state (`eye_onnx`), mouth state (`mouth_onnx`), cat heuristic, sharpness (Laplacian variance), mock fallbacks, background `worker`
- `curator/` — cloud LLM aesthetic judgment: providers (Anthropic, Gemini, local OpenAI-compatible) behind `CuratorProvider` trait, cost estimation, prompts, worker
- `commands/` — thin Tauri command wrappers by domain (one file per domain: `shoots`, `import`, `scan`, `drives`, `image`, `rating`, `culling`, `settings`, `export`, `ai`, `layout`, `curator`)
- `db/` — `schema.rs` is the authoritative table list (idempotent additive migrations) + typed CRUD
- `ingest/` — copy, EXIF, embedded JPEG preview, thumbnail, pHash, clustering, pairing, walker, progress
- `metadata/` — XMP read/write + write-coalescing `xmp_queue`, EXIF parsing, orientation
- `pipeline/` — preview cache, prefetch manager, decoder tiers, embedded-preview extraction, `photosift://` protocol handler
- `drives/` — removable drive detection for SD-card import

### `src/` (React frontend)

- `App.tsx`, `main.tsx`, `vite-env.d.ts`
- `pages/` — `ShootListPage`, `CullPage`, `PrimitivesPage`
- `components/` — 27 top-level + subdirs:
  - `chrome/` — AppShell, TabBar
  - `primitives/` — Photo, Stars, Kbd, ExifChip, ColorLabel, ScoreBar, LogoB, etc.
  - `triage/` — TriageShell + 4-column rails (incl. FacesRail)
  - `select/` — SelectShell + rating-peer / star-grouped rails + DetailRail
  - `route/` — RouteShell
  - `import/` — import flow components
  - top-level views: LoupeView, ComparisonView, GridView, HeatmapOverlay, MetadataOverlay, ImportDialog, SettingsDialog, ShortcutsOverlay, Toolbar, ViewSelector, FirstRunModal, FolderLayoutEditor, FaceChip, FaceThumb, EyeStatusBadge, Ai{Eye,Smile,Species,Sharpness,Pick}*, CuratorChip, FlagFlash, ProgressBar, Toast, DriveDetectedToast, GroupStack, EmptyViewState
- `stores/` — Zustand: `projectStore`, `settingsStore`, `aiStore`, `shootListStore`, `importIntentStore` (each with extensive Vitest coverage in `__tests__/`)
- `hooks/` — `useKeyboardNav` (one binding source of truth), `useImageLoader`, `useDriveDetection`
- `lib/` — pure utilities: `bracket`, `curatorApi`, `dateBrowser`, `faceVerdict`, `folderTemplate`, `groupRanking`, `importApi`, `loupeZoom`
- `types/`, `styles/`, `test/`

## Tauri Command Surface

56 commands in `src-tauri/src/lib.rs` `invoke_handler` block, grouped by domain:

| Domain | Count | Notes |
|---|---|---|
| Shoots | 3 | list/get/delete |
| Import | 2 | start/cancel (streams `ImportProgress`) |
| Scan | 2 | folder walk, thumbnail extraction |
| Drives | 1 | removable-drive listing |
| Image | 2 | list, metadata |
| Rating | 1 | star rating |
| Culling | 9 | flag, destination, bulk-flag, undo, view cursor, group CRUD |
| Settings | 3 | get, update, recluster (after pHash threshold change) |
| Export | 1 | publish-direct (Immich JPEG copy) |
| AI | 6 | status, cancel, reanalyze, faces-for-photo, heatmap, sharpness percentiles |
| Layout | 5 | sync-if-eligible, visit tracking, max-floor bump, bucket path, open in OS |
| Curator | 17 | per-provider keys (Anthropic/Curator/Local), test connection, start/cancel/resume/clear, status, judgments, summary, agreement, cost estimate, accept/override |

When adding a command, register it in `lib.rs` and mirror the type in TypeScript. Commands return typed structs, not raw JSON.

## Data Model

**10 tables** in `src-tauri/src/db/schema.rs` — authoritative:

`shoots` · `photos` · `groups` · `group_members` · `view_cursors` · `undo_log` · `settings` · `file_moves` · `curator_judgments` · `faces`

Photos carry three orthogonal attributes:
- `flag` — `unreviewed` / `pick` / `reject`
- `destination` — `unrouted` / `edit` / `publish_direct` (a.k.a. export)
- `star_rating` — 0-5, primary signal in the Select pass

`settings` is a single-row key/value table; reads cached in `settingsStore`. `file_moves` is an audit trail for layout-sync operations. `curator_judgments` and `faces` are append-only enrichment tables keyed on `photo_id`.

Two pHash tiers for clustering: near-duplicate (hamming ≤4, collapsed by default) and related (5-12, loose group).

## Architecture Rules

- **Every culling action persists immediately.** No save button, no unsaved state. Closing mid-cull is a non-event.
- **Keyboard-first.** Every action has a keystroke. `useKeyboardNav.ts` is the single source of binding truth; `ShortcutsOverlay.tsx` mirrors it manually — keep in sync.
- **Non-destructive data, layout-managing.** Pixel data, EXIF, and metadata are never mutated. The *layout* under a shoot folder IS managed by PhotoSift: `sync_shoot_layout` moves RAWs + sibling JPEGs into buckets — `RAW/`, `RAW/rejects/`, `RAW/selects/{0,1,2,3+}/`, `RAW/edit/`, top-level `Export/` — to mirror `(flag, destination, star_rating)`. Moves are idempotent and reversible. See `src-tauri/src/layout.rs`.
- **XMP sidecars rewrite on every move** at the new location, with stale sidecars at the old location cleaned up. The sidecar carries `xmp:Rating` + `photosift:destination`. **`xmp:Label` is deliberately never written** — Capture One and DxO interpret it as a user color tag the user did not choose. User-set `xmp:Label`s in pre-existing sidecars are passed through untouched. See `src-tauri/src/metadata/xmp.rs:107`.
- **Preview hot path.** `pipeline/prefetch.rs` preloads N+1..N+5 forward and N-1..N-3 backward as decoded pixel buffers. Target: zero perceptible load time on advance. Most performance-critical code path in the app.
- **Typed IPC.** Tauri commands return typed structs; shared types defined in Rust and mirrored in `src/types/`.
- **Two import paths.** `commands::scan::scan_folder` (pre-import preview) and `commands::import::start_import` both walk source files. Any per-file feature must be added to **both**.

## AI Subsystems

### On-device AI (`src-tauri/src/ai/`)

Runs at import as optional enrichment; suggest-only (never auto-applies a verdict).

- **Face detection** — YuNet (bundled), via `face::YuNetProvider`. Returns boxes + landmarks.
- **Eye state, mouth state, cat detection** — drop-in ONNX classifiers. Place `eye_state.onnx`, `mouth_state.onnx`, `cat_detector.onnx` into `~/.photosift/models/` to enable; falls back to mocks (`ai/mock.rs`) if absent or if ORT init fails. Contracts in `reference_eye_mouth_onnx_integration.md` (user memory).
- **Sharpness** — Laplacian variance via `ai/sharpness.rs`; per-shoot percentiles computed for relative scoring (a 60th-percentile shot in a shoot of soft glass is still the sharpest available).
- **CUDA** — `ort` is compiled with the `cuda` feature. ORT ships the provider DLL but not cuBLAS/cuDNN; extract those from `pip nvidia-cublas-cu12` / `pip nvidia-cudnn-cu12` wheels into `target/debug/` if face inference is ~2s instead of ~500ms. See `reference_cuda_dll_setup.md`.
- **Worker** — `ai/worker.rs` runs in the background with cancel/progress atomics on `AppState`. Graceful fallback on init failure: app boots, AI panel shows "unavailable", no crash.

### Curator (`src-tauri/src/curator/`)

Optional cloud LLM aesthetic / compositional culling. **Suggestion-only — never auto-applies, every judgment is reviewable.**

- **Providers** — Anthropic (Claude), Gemini, local OpenAI-compatible (e.g. self-hosted llama.cpp / Ollama). Behind a `CuratorProvider` trait so adding a provider means implementing one trait.
- **Keys** — per-provider, stored in OS keychain via `keyring` crate (service `photosift`). Native backends opt-in via `apple-native` / `windows-native` / `linux-native-sync-persistent` features — critical: without these, `keyring 3.x` defaults to a mock backend that silently drops writes.
- **Judgments** — persisted to `curator_judgments` with `provider` column for cross-provider audit. Agreement stats (`get_curator_agreement_stats`) compare curator verdicts against the user's eventual cull decisions.
- **Cost estimation** before running (`estimate_curator_cost_cents`); cancel/resume mid-run.

## Three-Pass Culling Model

Each "pass" is a view preset (filter + available actions), not a separate mode. Per-view filter gates configurable in Settings (`select_requires_pick`, `route_min_star`).

| View | Filter | Groups | Primary Keys |
|---|---|---|---|
| Triage | `flag = unreviewed` | Collapsed | `P` pick · `X` reject · `Space` next · `Enter` expand group |
| Select | `flag = pick AND star_rating >= floor` | Expanded | `1`-`5` rate · `[` `]` floor down/up · `Tab` 2-up · `X` reject |
| Route | `flag = pick AND star_rating >= route_min_star` (default 3) | None | `E` edit · `D` publish direct · `U` reset |

First time a user enters each view, `FirstRunModal` shows a one-time explainer gated by `settings.onboarded_{triage,select,route}`. "Replay tour" in `?` re-arms all three. Keep modal copy in sync with the bindings in `useKeyboardNav.ts` and the table in `ShortcutsOverlay.tsx`.

## Testing

```bash
# Frontend (Vitest) — stores + lib are well-covered, components are not
npm run test:run
npm run test:coverage

# Rust unit tests
cd src-tauri && cargo test --lib

# End-to-end (WebdriverIO + tauri-driver) — runs against a built debug binary
npm run test:e2e:build   # produces target/debug/photosift.exe
npm run test:e2e         # runs wdio.conf.mjs
```

E2E rig is documented in user memory `reference_tauri_driver.md`. Use it for regression coverage of hot paths; use the dev MCP bridge (below) for ad-hoc verification.

## Development Cycle — MANDATORY before PR handoff

**Code changes are not done when they compile and the unit tests pass.** Before opening a PR for the user to review, the change must be exercised end-to-end against the running app. Use at least one of:

- **`tauri-plugin-mcp` (preferred for ad-hoc UI verification)** — runs on `127.0.0.1:4000` in debug builds (`npm run tauri:dev`). Provides programmatic screenshots, DOM inspection, JS evaluation, click/type. Walk through the affected screens, capture screenshots, **read the screenshots yourself** — do not ask the user "does this look right?". See user memory `reference_tauri_mcp.md`.
- **`tauri-driver` / WebdriverIO (preferred for repeatable / regression coverage)** — extend or run the relevant `tests/e2e/*.spec.ts` when the change is in a hot path that other features depend on.

**Rule:** A PR is handed to the user only after the change has been exercised in the running app via one of these (or a written explicit reason — e.g. "pure backend refactor, full `cargo test --lib` coverage of the changed path"). Include "verified via tauri-mcp: [screens]" or "verified via tauri-driver: [test path]" in the PR description.

Unit tests prove correctness; in-app verification proves the feature actually works for the user.

## Commands

```powershell
# Dev (kills the Vite + Tauri pair on Ctrl+C). Use `tauri:dev` — it loads
# `tauri.dev.conf.json` so the running window is identifiable as "PhotoSift —
# DEV" and uses a distinct identifier (separate WebView2 storage from prod).
npm run tauri:dev

# Production build
npm run tauri build

# Probe the database — prod uses .photosift, debug builds use .photosift-dev
sqlite3 ~/.photosift/photosift.db
sqlite3 ~/.photosift-dev/photosift.db

# Test with real D750 NEFs — point at a folder, expect ~6016x4016 embedded previews
```

## Build Isolation (dev vs. prod)

Debug builds keep their own state so a compiled prod binary can keep running uninterrupted while you develop:

- **Data root.** Debug = `~/.photosift-dev/`, release = `~/.photosift/`. One `cfg!(debug_assertions)` branch in `db/schema.rs::photosift_home()` propagates through DB, cache, and `ensure_models_on_disk()`.
- **Keychain.** Debug uses service `photosift-dev`, release `photosift`. `curator/mod.rs` exposes the right `KEYRING_SERVICE` via parallel `#[cfg]` consts. Re-enter API keys once when first running dev.
- **Tauri identifier + window title.** `tauri.dev.conf.json` overrides `productName` → `PhotoSift (dev)`, `identifier` → `com.photosift.app.dev`, and window title → `PhotoSift — DEV`. Loaded only by `npm run tauri:dev` (`tauri dev --config …`). The distinct identifier also gives the WebView2 instance a separate `userDataFolder`, isolating `localStorage` UI flags.
- **Escape hatch.** `PHOTOSIFT_HOME=<path>` env var overrides the data root. Use it sparingly to debug against prod data — writes mutate prod state.
- **E2E.** `npm run test:e2e:build` produces a debug binary, so e2e also writes to `~/.photosift-dev/` and uses the dev keyring. No risk of e2e clobbering prod data.

## Gotchas & Conventions

- **Port 1420 is pinned by Tauri** — no auto-increment. Orphaned `vite` / `tauri dev` processes hold it and produce "port already in use" failures plus stale-UI confusion. Always kill before ending a session.
- **Dev profile overrides crank `opt-level=3`** for `ort`, `ort-sys`, `image`, `zune-jpeg`, `rawler`, `jpeg-decoder`, `jpeg-encoder` and `opt-level=2` for `photosift` itself. Without these, AI inference takes >1s/image in dev. **Don't lower them** without measuring.
- **`xmp:Label` is intentionally never written.** Tests assert its absence; user-set labels in pre-existing sidecars pass through untouched. See `metadata/xmp.rs:107`.
- **`keyring 3.x`** defaults to a mock backend without `*-native` features → `set_password()` silently succeeds but `get_password()` returns `NoEntry`. The Cargo manifest enables native backends for all three desktop OSes; don't remove them.
- **Two import paths** — any per-file feature added to `commands::import::start_import` must also be added to `commands::scan::scan_folder` and vice versa.
- **Tauri MCP plugin is debug-only** — `#[cfg(debug_assertions)]` gate in `lib.rs`. Release builds don't ship the bridge.
- **Conventional commits** — `feat:`, `fix:`, `refactor:`, etc.
- **Rust**: `rustfmt`, `clippy` clean. `anyhow` in command handlers, `thiserror` in module-internal error types.
- **TypeScript**: strict, no `any`. Functional components with hooks.
- **No comments explaining *what* well-named code already says.** Comments explain *why* — a hidden constraint, an invariant, a workaround. The CLAUDE.md you are reading is the place for context, not inline comments.
