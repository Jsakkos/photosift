# CLAUDE.md — PhotoSift

## What This Is

PhotoSift is a local-first photo pipeline: a Rust server on `127.0.0.1` plus a React UI in a normal browser tab. It ingests an SD card or folder (copy + SHA-256 verify, embedded previews, pHash), enriches on-device (faces, sharpness, bursts, moments), and lets the user cull each shoot with one outcome per photo. Outcomes are written to XMP sidecars for Lightroom Classic; deletes go to the Recycle Bin only after explicit approval.

**Status: Plans 1 (foundation) and 2 (Claude helper, MCP endpoint, taste profile, evaluation harness) are built; the helper meets its agreed quality bar (design doc §9).** Plans 3-4 (albums + Immich, people) are not built yet; nothing in the code references them.

**The helper proposes, never decides.** Claude (via the user's own `claude` CLI login) proposes an outcome and a short reason per photo; proposals are suggestions the user accepts or overrides. A helper run the app spawns cannot set an outcome: besides its CLI deny list, its MCP URL carries a per-process token and the server refuses `set_outcome` and `start_run` to it (and `set_taste_profile` to background and eval runs). Nothing the helper does touches a photo's file or XMP. External MCP clients (plain `/mcp`, e.g. Claude Desktop) may call `set_outcome`, like the HTTP API; nothing deletes without the in-app approval.

Docs:
- `docs/superpowers/specs/2026-10-19-photosift-overhaul-design.md` — design of record (intent, constraints, architecture).
- `docs/superpowers/plans/2026-10-19-photosift-overhaul-plan-1-foundation.md` — the plan that produced this code.
- `PhotoSift_Spec.md` — pointer to the design doc (the old Tauri spec was removed).
- `README.md` — user/contributor overview.
- Older `docs/superpowers/` specs/plans and `docs/*.md` describe the removed Tauri prototype; history only.

When the design doc and the code disagree, the code wins — flag the divergence rather than implement to the doc.

## Public repo

**The GitHub repo is public. No photos, previews, face crops, embeddings, XMP files copied from the library, real library paths, or personal names go into git.** Real-photo fixtures live outside git (`PHOTOSIFT_GOLDEN_DIR`, `tests/golden/manifest.txt` and `.photosift-golden/` are gitignored). Test data in git is synthetic.

## Tech Stack

| Layer | Choice |
|---|---|
| Server | Rust, axum + tokio, SQLite (`rusqlite`, bundled), `ort =2.0.0-rc.12` (ONNX Runtime, `cuda` feature), `kamadak-exif`, `quick-xml`, `zune-jpeg` + `fast_image_resize` + `jpeg-encoder`, `trash` (Recycle Bin), `sysinfo` (drive watcher) |
| Models | YuNet (face detect) + SFace (identity), fetched by `scripts/fetch-models.ps1`, never committed |
| Web | React 19 + TypeScript strict, React Router 7, Vite 6, no state library |
| Tests | `cargo test` (server), Vitest (web unit), Playwright against a real server (web e2e) |
| Platform | Windows 11 only; binds `127.0.0.1` only, default port `7420` (release) / `7421` (debug) |

## Repo Layout

### `server/` (crate `photosift`, binary `photosift.exe`)

- `main.rs` — startup: single-instance check (bind = lock; second launch just opens the browser with `--open`), config, DB, card watcher, router. `--no-browser` suppresses opening a tab.
- `lib.rs` — module list + `build_state`
- `config.rs` — `Settings` (`settings.toml`), data-root and web-dist resolution
- `instance.rs` — single-instance handshake, log redirect when detached (GUI subsystem), browser opening
- `api/` — axum router + handlers (`scan`, `import`, `shoots`, `photos`, `media`, `trash`, `ws`) and `guard.rs` (Host/Origin check)
- `db/` — `schema.sql` (authoritative tables), `photos.rs` CRUD, `review.rs` review-screen queries
- `ingest/` — `scan` (card/folder → CardScan), `events` (split into events, pure), `naming` (slug + folder path, pure), `copy` (copy + verify), `job` (import orchestration + progress; `ImportMode::Copy` or `IndexInPlace`), `in_place` (open a library folder as a shoot without copying: library-root check, `Export\` excluded, photos only; shared with the eval harness)
- `media/` — `embedded` (largest embedded JPEG), `exif`, `render` (orient → preview/thumb JPEGs), `phash`
- `enrich/` — `faces` (YuNet with tiling), `identity` (SFace), `quality` (sharpness, exposure), `moments` (moments + bursts + signal score, pure), `job` (background worker)
- `outcome.rs` — `Outcome` enum + XMP field mapping; `xmp.rs` merge-write sidecar; `trash.rs` approved delete
- `drives.rs` — removable-drive watcher (injectable lister); `events.rs` — `ServerEvent` + WebSocket broadcast hub; `logging.rs` — `plog!` timestamped log macro
- `helper/` — helper runs and chat turns: `prompt.md` (system prompt; `PROMPT_VERSION` is a hash of it, so edits are tracked per run), `prompt.rs`, `transport.rs` (spawns the `claude` CLI with isolation flags, a run timeout and cancel), `kill_tree.rs` (Windows Job Object: killing a child, or PhotoSift exiting, kills its whole process tree, e.g. the npm `.cmd` shim's node), `chat.rs`, taste profile and learning from past decisions
- `mcp/` — the `/mcp` endpoint (JSON-RPC over POST) and its tool definitions
- `eval/` — evaluation harness library: scores picks against a shoot's `Export\` folder
- `bin/make-synthetic-card.rs` — writes a fake card of synthetic JPEGs for tests; `bin/fake-claude.rs` — scripted stand-in for the `claude` CLI (tests, e2e); `bin/eval.rs` — `photosift-eval`
- `testsupport.rs` — synthetic JPEG/EXIF builders shared by tests and the bin; `tests/` — integration tests (`common/mod.rs` boots a test server on a temp data root)

### `web/`

- `src/main.tsx`, `App.tsx`, `styles.css`
- `src/api/` — `client.ts` (typed fetch), `types.ts` (mirror of server JSON), `ws.ts` (reconnecting WebSocket)
- `src/pages/` — `ShootsPage`, `ImportPage`, `ReviewPage`
- `src/review/` — `Summary`, `Highlights`, `MomentSection`, `JustifiedRow`, `PhotoTile`, `Loupe`, `Compare`, `Dialogs`; `keyboard.ts` (**the only key map; `?` renders it**), `model.ts`, `useReview.ts`, `prefetch.ts`
- `src/lib/` — pure helpers (`justify` row packing, `cards`, `importDraft`); `src/components/CardBanner.tsx`
- `tests/e2e/` — Playwright specs; `fixtures.ts` starts a real `photosift.exe` on a free port with a temp `PHOTOSIFT_HOME` per test; `global-setup.ts` builds server + web once

### `scripts/` (PowerShell)

`fetch-models.ps1`, `make-golden-fixture.ps1`, `build-release.ps1`, `install-startup.ps1` (Task Scheduler entry, run at logon, hidden), `screenshot.ps1` (native desktop screenshot fallback).

Other top-level: `tests/golden/` (fixture instructions + `manifest.example.txt`), `benchmark/` (offline Python eval tooling, never runs in the app), `spikes/` (stack-benchmark history).

## HTTP / WebSocket Surface

Routes in `server/src/api/mod.rs`. JSON bodies, errors are `{"error": "..."}`. Mirror every response type in `web/src/api/types.ts`.

| Route | Purpose |
|---|---|
| `GET /api/health`, `/api/status` | liveness + version; AI backend (`cuda`/`cpu`), embeddings availability, enrichment queue |
| `GET /api/cards`, `/api/settings` | inserted cards the watcher announced; read-only settings (library root) |
| `POST /api/scan`, `GET /api/scan/{id}`, `POST /api/scan/{id}/events`, `POST /api/slug` | scan a card/folder, re-group into events, slugify a name |
| `POST /api/import`, `GET /api/import/current`, `GET /api/import/{job}` | start / observe the single import job |
| `POST /api/import/in-place` | open a folder strictly inside `library_root` (e.g. a shoot Lightroom imported) as one shoot, nothing copied: body `{folder}`; `{status:"existing", shoot_id}` when it is a shoot already (opened before or imported into), else `{status:"indexing", job_id, photos}` as an import job (409 while one runs); 400 outside the library, for the root itself, or with no photos |
| `GET /api/shoots`, `GET /api/shoots/{id}/review`, `POST /api/shoots/{id}/trash` | shoot list, review data, approved delete (body `{photo_ids}`: only those pending deletes are trashed) |
| `POST /api/shoots/{id}/helper/run` | start a helper run for a shoot (202 `{run_id}`; 409 while one is open; 503 when unavailable) |
| `POST /api/shoots/{id}/helper/cancel` | stop the shoot's helper run: kills the child's process tree and fails the run "stopped by you", or drops a waiting run (204; 404 when there is none) |
| `GET` / `POST /api/shoots/{id}/chat` | chat history / send a chat turn about the shoot |
| `GET` / `PUT /api/taste` | the taste profile the helper is told |
| `GET /api/taste/decisions` | recent overrides of proposals (decisions that differ from the proposal), newest first; `?limit=` (50, at most 200) |
| `POST /api/photos/{id}/decision-note` | attach a note to the user's decision |
| `POST /api/photos/{id}/outcome` | set outcome (+ blooper) and rewrite the sidecar (409 for a trashed photo) |
| `GET /media/preview/{id}`, `/media/thumb/{id}`, `/media/scan/{scan}/{idx}` | JPEGs served from the cache |
| `POST /mcp` | stateless MCP (Streamable HTTP, JSON replies) over the read operations plus `set_outcome`; tool table in `server/src/mcp/tools.rs`; `GET` is 405. `?client=helper&kind=<background\|chat\|eval>&token=<t>` marks the app's own helper child: a wrong token is 403, and its denied tools are refused (`isError`) and left out of `tools/list` |
| `GET /ws` | `ServerEvent` stream: import/enrichment progress (`import_*`, `enrich_*`), `card_detected` / `card_removed` / `card_scan_failed`, `proposals_changed`, `helper_status` (run queued/running/done/failed/interrupted, with the error), `chat_message`, `taste_changed` |

MCP tools (`mcp/tools.rs`): `list_shoots`, `get_shoot_overview`, `get_moment`, `get_photos`, `get_faces`, `get_taste_profile`, `set_outcome`, `start_run`, `propose`, `propose_your_call`, `finish_run`, `set_taste_profile`. The helper's own child process is given only the propose-side tools, and the server enforces that (see `/mcp`).

Anything else under `/` serves `web/dist` with an SPA fallback to `index.html`.

## Data Model and Domain Rules

- Outcomes: `unreviewed | album | library | archive | delete`, plus an independent `blooper` boolean. One outcome per photo.
- XMP mapping (`outcome.rs`): album ★4, library ★3, archive ★2, delete ★1 + `xmp:Label="Red"`, unreviewed writes nothing. Blooper adds the hierarchical keyword `PhotoSift|Blooper`. An existing ★5 is never lowered. Only these fields are written; everything else in an existing sidecar (including Lightroom `crs:*`) is preserved.
- **Files are never moved or renamed after ingest.** Original EXIF is never rewritten. The card is never written to.
- Every copied file is SHA-256-verified against the source before an import reports success. Global dedupe by content hash.
- In-place shoots (`POST /api/import/in-place`): rows point at the files where they are, the shoot's folder is the opened folder (any name), and sidecars are written beside those files like any shoot's. Only folders inside `library_root` qualify.
- Library folders: named event `{root}\{YYYY}\{YYYY-MM-DD}_{Slug}\`, everyday `{root}\{YYYY}\{YYYY-MM}_Everyday\`. Slug is ASCII, words joined by `-`, no spaces/apostrophes ("Baby's First Meal" → `Babys-First-Meal`).
- Delete = Recycle Bin via the `trash` crate, only after explicit batch approval. Never permanent.
- Previews: 2560 px long edge JPEG q85; thumbnails 400 px q80; EXIF orientation applied; source is the largest embedded JPEG (RAF/NEF) or the JPEG itself.
- AI enriches, never hides or auto-decides: signals annotate, outcomes are always the user's call.

## UI Rules

- Every photo is rendered whole at its own aspect ratio (fixed-height justified rows), never cropped.
- Every action persists immediately; no save button.
- Keyboard-first. `web/src/review/keyboard.ts` `BINDINGS` is the single binding source; the `?` overlay renders from it. Keys: arrows navigate moments/photos, `A` album · `L` library · `R` archive · `X` delete? · `U` unreviewed · `B` blooper · `Enter` loupe · `C` compare · `1`-`5` filters · `Y` accept the helper's suggestion · `Shift+Y` accept every suggestion in the moment · `H` helper chat · `?` key map · `Esc` back.

## Data Roots and Environment

| Item | Value |
|---|---|
| Data root | release `%USERPROFILE%\.photosift\`, debug `%USERPROFILE%\.photosift-dev\` (a `cfg!(debug_assertions)` branch in `config::data_root`) |
| Contents | `photosift.db`, `settings.toml`, `cache\preview\`, `cache\thumb\`, `models\`, `logs\photosift.log` (release/detached) |
| `PHOTOSIFT_HOME` | overrides the data root; empty string = unset. Tests use a temp dir. |
| `PHOTOSIFT_WEB_DIST` | built UI directory; default `<exe dir>\web`. Missing dir ⇒ API-only. |
| `PHOTOSIFT_GOLDEN_DIR` | fake-card dir for the `#[ignore]` golden tests |
| `PHOTOSIFT_MODELS_DIR` | models dir for the golden enrichment test; default `<data root>\models` |
| `PHOTOSIFT_CLAUDE_BIN` | path to the `claude` executable; default is `claude.exe` / `claude.cmd` on `PATH`. Tests point it at `fake-claude`. |
| `PHOTOSIFT_EVAL_DIR` | eval harness scratch dir (scratch data root + `results.jsonl`); default `<repo>\.photosift-eval\` (gitignored) |
| Helper settings | `settings.toml`: `helper_enabled` (true; gates only the automatic run after enrichment), `helper_transport` (`cli`; or `cli-api-key`, `off`), `helper_model` (`sonnet`), `helper_max_turns` (120), `helper_max_budget_usd` (3.0), `helper_frames_per_burst` (3), `helper_run_timeout_minutes` (20; wall-clock limit of one run or chat turn, after which its process tree is killed and the run fails) |
| Library root | `settings.toml` `library_root`; default release `E:\photos\DSLR`, debug `<dev data root>\library` |
| Port | `settings.toml` `port`; default release `7420`, debug `7421` |

Debug and release keep separate state (data root, port, library), so a running release binary is unaffected by development. Defaults only: an existing `settings.toml` keeps whatever it says, so an old dev `settings.toml` that still has `7420` / the real library must be edited or deleted.

## Commands

```powershell
# One-time: download YuNet + SFace into the debug data root (omit -Dev for release)
powershell -File scripts\fetch-models.ps1 -Dev

# Run the server (debug build, data root ~\.photosift-dev, http://127.0.0.1:7421)
cargo run --manifest-path server/Cargo.toml
# Without PHOTOSIFT_WEB_DIST / a web\ dir beside the exe the server is API-only;
# either point it at web/dist after `npm --prefix web run build`, or use the dev server:
npm --prefix web ci            # first time
npm --prefix web run dev       # Vite on http://localhost:5173, proxies /api /media /ws to :7421

# Tests and checks
cargo test --manifest-path server/Cargo.toml
cargo fmt --manifest-path server/Cargo.toml --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
npm --prefix web run test       # Vitest
npm --prefix web run build      # tsc --noEmit + vite build
npm --prefix web run e2e        # Playwright; global setup builds server + web

# Release folder (exe + web + ORT provider DLLs) and logon task
powershell -File scripts\build-release.ps1 [-CudaDllDir <dir>]
powershell -File scripts\install-startup.ps1
```

### Evaluation harness (real shoots, outside git)

`photosift-eval` scores picks against what the user actually exported from past shoots; read-only against the library. Setup, columns and the cost warning are in `tests/eval/README.md`.

```powershell
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- run --max-shoots 3 --label baseline-signals            # signals only, no Claude
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- run --helper --max-shoots 3 --label helper-v1       # real helper runs (uses the Claude login)
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- report --label baseline-signals --label helper-v1
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- sweep      # burst threshold sweep
```

**Claude plan usage.** Helper runs and dev sessions share one Pro-plan allowance (the 5-hour limit has stopped a run). A ~370-photo helper run is about $0.6 estimated (client-side). Never run `--helper` evals or real `claude` calls casually; tests and e2e use `fake-claude`. The helper runs the official CLI under the user's own login: never store or proxy tokens, never `--bare`.

### Golden fixture (real photos, outside git)

`tests/golden/README.md` has the full steps. Short version: copy `tests/golden/manifest.example.txt` to `tests/golden/manifest.txt` (gitignored) with paths to your own RAF/NEF files, run `scripts\make-golden-fixture.ps1` (writes `.photosift-golden\card\`), then:

```powershell
$env:PHOTOSIFT_GOLDEN_DIR = "$PWD\.photosift-golden\card"
cargo test --manifest-path server/Cargo.toml -- --ignored
```

They panic when `PHOTOSIFT_GOLDEN_DIR` is unset. CI cannot run them. Run them after changes to ingest, EXIF/embedded-preview extraction, rendering, or enrichment.

## Verification Loop — MANDATORY before reporting done

Compiling and unit tests passing is not done. Layered, in order:

1. `cargo test`, clippy, fmt, `npm --prefix web run test`, `npm --prefix web run build`.
2. For UI changes: **verify with Playwright or the in-app browser against a running server and read the screenshots yourself** — do not ask the user "does this look right?". The e2e suite (`npm --prefix web run e2e`) starts a real server per test with a synthetic card (`make-synthetic-card`) and a temp data root; extend it for regressions, and use a browser pane/Playwright MCP against `cargo run` + `npm --prefix web run dev` for ad-hoc checks (console, network and DOM are all available).
3. For backend/filesystem behavior: integration tests under `server/tests/`, or a CLI repro. Use the golden fixture for anything that depends on real RAW files.

Only summarize what you actually observed. Don't say "works" on the strength of the diff alone.

## Gotchas & Conventions

- **Host/Origin guard.** `api/guard.rs` rejects any `Host` other than `127.0.0.1:<port>` / `localhost:<port>` (DNS rebinding) and any foreign `Origin` on non-GET and WebSocket requests. Debug builds also trust Vite's `http://localhost:5173`. The Vite proxy therefore **must** set `changeOrigin: true` (it does in `web/vite.config.ts`); a new proxied path needs it too.
- **CUDA.** `ort` is built with `cuda`. ORT ships `onnxruntime_providers_cuda.dll` / `_shared.dll` beside the exe (copied by `build-release.ps1`), but not cuBLAS/cuDNN: those must be on `PATH`, or bundled with `build-release.ps1 -CudaDllDir <dir>` (the pip `nvidia-cublas-cu12` / `nvidia-cudnn-cu12` wheels are a source). If CUDA init fails the server logs it and falls back to CPU; never fatal. Models absent ⇒ enrichment reports unavailable and the app still runs.
- **No silent helper spend.** Only an enrichment pass that enriched *new* photos queues the automatic helper run. A start that merely re-detects faces (`photos.faces_model` stale or NULL, e.g. every Plan-1 photo once models load) never does, and a photo whose preview the models cannot read is stamped `<stamp>:no-preview` so it is not re-queued every start. Tests exercise these paths without the ONNX files through `Overrides::fake_faces`.
- **Dev profile** sets `opt-level = 3` for `ort`, `ort-sys`, `fast_image_resize` and image/JPEG crates; ONNX inference is >1 s/image without it. Don't lower without measuring.
- **Port 7420 (release) / 7421 (debug)** is the single-instance lock. A second `photosift.exe` on the same port finds the first via `/api/health` and exits. Debug uses its own port and library so it coexists with the logon-started release. Orphaned dev servers hold their port, and Vite pins `5173` (`strictPort`). Kill both before ending a session.
- **Release exe is a GUI-subsystem build**: no console; output goes to `<data root>\logs\photosift.log`.
- **Logging.** Use `plog!("tag", ...)` (not `eprintln!`) for server diagnostics: lines carry a local timestamp and module tag, and the unattended release's log file is the debugging surface.
- **fake-claude in e2e.** The Playwright fixture's `claudeScript` option (a JSON script, see the header of `server/src/bin/fake-claude.rs`) points `PHOTOSIFT_CLAUDE_BIN` at `fake-claude` with `helper_transport = 'cli'`; without it the fixture writes `helper_transport = 'off'`, so e2e never starts a real `claude`. `global-setup.ts` builds all bins.
- **Helper wording.** Photos are never cited by id in summaries, reasons or chat (the user cannot see ids); plain text, no markdown. Ids belong only in tool calls.
- **Types** are mirrored by hand: change a response in `server/src/api/*`, update `web/src/api/types.ts` in the same commit.
- Conventional commits (`feat:`, `fix:`, `refactor!:` ...).
- Rust: `rustfmt` and `clippy -D warnings` clean; `anyhow` at handler boundaries, `thiserror` inside modules. TypeScript: strict, no `any`, functional components with hooks.
- No comments explaining *what* well-named code says. Comments explain *why* — a hidden constraint, invariant, or workaround.
