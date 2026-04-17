# PhotoSift Performance Baseline — 2026-04-16

First end-to-end real-shoot profile after the polish & hardening sprint. The
point is a baseline, not a benchmark — numbers vary by SSD/RAM/CPU. Recapture
before and after any perf-sensitive change (AI pipeline, prefetch tuning,
protocol handler refactor).

## Test shoot

- Source: `TODO: path`
- Camera: Nikon D750 NEF
- File count: `TODO: N`
- Total size on disk: `TODO: GB`

## How to capture

1. `cargo tauri dev` from repo root. Use a release-ish dev build (startup
   warmup is cold-path; restart between runs if you want a fair second
   measurement).
2. Open the PhotoSift window. Click **New Import**.
3. Pick the test folder and a slug. Mode: **Copy**.
4. Hit **Start Import**. Start a stopwatch on click.
5. Note the wall-clock time at each phase transition (toast / progress text):
   - Scanning → Processing
   - Processing → Clustering
   - Clustering → Complete
6. Note peak memory from Task Manager → Details → `photosift.exe` (private
   bytes). Take the high-water mark during Processing.
7. After import completes, click into the shoot. Hold `→` for 10 seconds and
   watch for preview stalls.
8. Run `Cmd+E` (XMP export). Time the toast.

## Results

| Metric | Value |
|---|---|
| Scan duration | `TODO` s |
| Process duration | `TODO` s |
| Cluster duration | `TODO` s |
| **Total import** | `TODO` s |
| Throughput | `TODO` photos / s |
| Peak RSS (photosift.exe) | `TODO` MB |
| Preview advance latency (hold →) | `TODO` — any stalls? |
| XMP export (picks only) | `TODO` s for `TODO` sidecars |

## Observations

- `TODO: any stalls, flickers, unexpected memory spikes?`
- `TODO: did the UI ever become unresponsive during import?`
- `TODO: did the Cmd+E export finish without UI freeze?`

## Follow-ups found

- `TODO: anything that surprised you — file a GitHub issue or add to FutureWork.md.`

## Methodology notes

- Numbers are from a **debug** build via `cargo tauri dev`. Release build
  numbers will be faster; rebaseline separately if that matters.
- Single cold run — first-pass, no file-system warm cache. Subsequent runs of
  the same import will fail the dedup check (content hash), not re-copy.
