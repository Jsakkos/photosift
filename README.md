# PhotoSift

A local-first photo pipeline for Windows. Plug in an SD card (or point at a folder), import into a dated library, let the app group and annotate the shots on-device, then cull each shoot in the browser with one keystroke per photo. Decisions are written to XMP sidecars so Lightroom Classic picks them up on import.

PhotoSift is a small Rust server on `127.0.0.1` plus a React UI you open in a normal browser tab. Nothing leaves your machine.

**Status: Plans 1 and 2 of 4.** Ingest, enrichment, review, the Lightroom handoff, the Claude helper, the MCP endpoint and an evaluation harness are built. Albums and Immich publishing, and people recognition, are planned but not built yet.

## What it does today

- **Import.** Detects a card, scans it, proposes events from capture-time gaps (named event folders, or one "Everyday" folder per month for small days), and copies every file with SHA-256 verification against the source. Duplicates are skipped by content hash. The card is never written to, and files are never moved or renamed after import.
- **Enrich.** In the background, on your GPU when available (CPU otherwise): embedded-preview rendering, perceptual hash, face detection and identity embeddings (YuNet + SFace), sharpness and exposure, near-duplicate bursts, and moments. These annotate the review screen; they never hide or decide anything.
- **Review.** A shoot opens as a summary and highlights, then moments of justified rows (every photo whole, at its own aspect ratio). Set one outcome per photo — album, library, archive or delete? — and tag bloopers. Loupe and burst compare views. Every action saves immediately.
- **Hand off.** Outcomes become star ratings, a red label for delete, and a `PhotoSift|Blooper` keyword in XMP sidecars, merged into any existing sidecar without touching other fields. Deletes go to the Recycle Bin, only after you approve the batch.

- **Helper.** Optionally, Claude looks at a shoot's moments and proposes an outcome and a short reason for every photo, flags coin-flip pairs for you to decide, and answers questions in a chat panel. It runs the official `claude` CLI under your own login (set `helper_transport = "off"` in `settings.toml` to disable it), sees only downsized previews, and **only proposes**: you accept or override each photo, and nothing it does changes a file or a rating. It learns from your overrides through an editable taste profile. The same tools are exposed at `http://127.0.0.1:<port>/mcp`. Runs draw on your Claude plan's usage limits; a ~370-photo shoot is about $0.6 estimated. `photosift-eval` scores picks against past shoots you exported; see `tests/eval/README.md`.

Keyboard: arrows move between photos and moments, `A` album, `L` library, `R` archive, `X` delete?, `U` unreviewed, `B` blooper, `Enter` loupe, `C` compare, `1`-`5` filters, `?` shows the full map.

## Build and run

Prerequisites: Windows 11, Rust (stable), Node 22. An NVIDIA GPU with CUDA 12 / cuDNN 9 is optional.

```powershell
# one-time: download the face models into the dev data root
powershell -File scripts\fetch-models.ps1 -Dev

# server (debug build), http://127.0.0.1:7421 (the release uses 7420)
cargo run --manifest-path server/Cargo.toml

# UI with hot reload, http://localhost:5173 (proxies the API to the server)
npm --prefix web ci
npm --prefix web run dev
```

For a release install:

```powershell
powershell -File scripts\build-release.ps1      # dist\photosift\: exe + web UI + ORT provider DLLs
powershell -File scripts\install-startup.ps1    # start it at logon, hidden
```

The release exe serves the UI itself; open `http://127.0.0.1:7420/` (or run `photosift.exe --open`). For GPU inference the CUDA 12 cuBLAS and cuDNN 9 DLLs must be on `PATH`, or bundled with `build-release.ps1 -CudaDllDir <folder>`; otherwise it falls back to CPU.

## Data locations

| | Release | Debug build |
|---|---|---|
| Data root | `%USERPROFILE%\.photosift\` | `%USERPROFILE%\.photosift-dev\` |

Override with `PHOTOSIFT_HOME`. The root holds `photosift.db` (SQLite), `settings.toml`, `cache\preview\` and `cache\thumb\` (regenerable), `models\`, and `logs\`. The photo library itself lives at `library_root` in `settings.toml` (default release `E:\photos\DSLR`; a debug build defaults to `library\` inside its own data root, and uses port 7421), laid out as `{root}\{YYYY}\{YYYY-MM-DD}_{Slug}\`.

## Tests

```powershell
cargo test --manifest-path server/Cargo.toml
npm --prefix web run test       # unit
npm --prefix web run e2e        # Playwright against a real server and a synthetic card
```

Tests that need real RAW files are `#[ignore]`d and read a local, untracked fixture; see `tests/golden/README.md`. The repository is public, so it contains no photos or personal data: all committed test data is synthetic.

## Layout

```
server/      Rust crate: API, ingest, enrichment, XMP, drive watcher
web/         React + TypeScript UI, Vitest unit tests, Playwright e2e
scripts/     models, release build, startup task, golden fixture
tests/golden/  instructions for the local real-photo fixture
benchmark/   offline Python evaluation tooling (not part of the app)
docs/        design doc and implementation plans
```

## See also

- `docs/superpowers/specs/2026-10-19-photosift-overhaul-design.md` — design of record
- `CLAUDE.md` — notes for AI-assisted development
