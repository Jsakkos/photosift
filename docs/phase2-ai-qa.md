# Phase 2 AI — Manual QA Checklist

Complete this after a full rebuild (`cargo tauri dev`) on a Windows box with an
NVIDIA GPU. For CPU-only machines, the same checklist applies with one known
difference: `providerStatus = "cpu"`.

## Current shipping state (2026-04-16)

- **Face detection**: **real** — YuNet 2023mar with full anchor decoder + NMS. CUDA→CPU fallback wired. Bundled ~230 KB model, extracted to `~/.photosift/models/` on first run. Live inference smoke test exists (`#[ignore]`-gated — needs `ORT_DYLIB_PATH` env var on Windows).
- **Eye state (open/closed)**: mock provider — alternates 0/1 deterministically. Not representative.
- **Whole-image sharpness**: **real** (Laplacian variance). The sort, filter, and heatmap surfaces all exercise real signal.
- **Heatmap overlay**: real Laplacian tile grid, on-demand via `H`.
- **AI-pick group recommendation**: derivation logic works, but scores (sharpness × 1+eyes_open) depend on the mock eye provider — treat as placeholder ranking until eye classifier is real.

## Prereqs

- Windows 10/11 with recent NVIDIA driver.
- Real D750 NEF shoot ≥500 photos.
- Built from the `phase2-ai` branch (or merged to main).

## 0. Smoke

- [ ] `cargo tauri dev` starts; no AI-related panics in the log.
- [ ] Import-complete event fires and AI queue populates (`log::info!("Enqueued N photos...")`).
- [ ] Toolbar progress indicator appears during import if `enable_ai_on_import` is on.
- [ ] Settings dialog shows the AI section (3 controls + Re-analyze button) on a shoot page.

## 1. Import + background analysis

- [ ] Import a ≥500-photo shoot with "Enable AI analysis on import" checked.
- [ ] Import finishes in usual time (no regression vs Phase 1 baseline).
- [ ] Progress indicator counts up, hides when queue drains.
- [ ] `failedCount` stays at 0 on a clean shoot. If not, open the log and note which photos failed.
- [ ] Full ≥500-photo shoot completes analysis in under 10 min wall clock on CUDA. CPU-only may take longer — document actual.

## 2. Loupe — AI panel (`F` key)

YuNet face detection is live; eye open/closed remains mock.

- [ ] Press `F` on a portrait → panel shows face count + sharpness chip.
- [ ] Photo with 1 face → one face row with thumb + eye indicators + sharpness numbers.
- [ ] Photo with 2+ faces → up to 3 inline rows, "+N more" pill.
- [ ] Photo with 0 faces → panel hidden unless `F` is pressed to force-show.
- [ ] Press `F` again → panel hides.
- [ ] Open a photo with `aiAnalyzedAt === null` (mid-analysis) → panel stays hidden even with `F` pressed.
- [ ] When `providerStatus === "disabled"` → panel never renders (verify by deleting `~/.photosift/models/yunet.onnx`, restarting).
- [ ] Eye open/closed indicators show (currently alternating mock values — not semantically meaningful until eye classifier lands).

## 3. Loupe — heatmap overlay (`H` key)

- [ ] Press `H` on a sharp photo → green-dominant tint overlay.
- [ ] Press `H` on an out-of-focus photo → red-dominant tint.
- [ ] Press `H` on a mixed-sharpness photo → visibly graded overlay (green on face, red on background or vice versa).
- [ ] Press `H` again → overlay hides.
- [ ] Overlay is purely visual (no mouse capture / pointer-events leak).
- [ ] Navigate to a new photo while heatmap is on → new heatmap fetches + draws within 1 s.
- [ ] Navigate through ~25 photos with heatmap on → oldest cached heatmap evicted (cache capped at 20).

## 4. Groups — AI-pick badge + `Shift+A`

- [ ] Open a near-dup burst with ≥2 analyzed members → `★ AI` badge appears on one thumb in both filmstrip and grid.
- [ ] `Shift+A` while focused in that group → cover changes to the AI pick, re-renders immediately, badge disappears (because the AI pick is now the cover).
- [ ] Only one badge per group.
- [ ] Group with 0 or 1 analyzed members → no badge.

## 5. Sort + filter

- [ ] `Alt+S` cycles: none → sharpness → faces → none.
- [ ] `sortByAi === "sharpness"` orders visible photos by descending sharpness. Null-analyzed photos sort to the end.
- [ ] `sortByAi === "faces"` orders by descending face count. Null-analyzed photos sort to the end.
- [ ] In Settings → hide-soft threshold = 50 → photos with sharpness < 50 disappear from Select and Route views.
- [ ] Photos still analyzing (aiAnalyzedAt = null) are NOT filtered — confirm they stay visible as AI catches up.
- [ ] Threshold = 0 → no photos filtered regardless of sharpness.

## 6. Cancel

- [ ] Start an import with a large shoot, click "cancel" in the Toolbar indicator mid-run.
- [ ] Indicator hides; progress stops advancing.
- [ ] Already-analyzed photos keep their AI data (partial results preserved).
- [ ] Start a new import → prior cancel flag is reset, new batch processes normally.

## 7. Re-analyze

- [ ] Open Settings → Re-analyze button on a finished shoot.
- [ ] Confirm prompt → queue populates; all photos' `aiAnalyzedAt` clears.
- [ ] After completion, `faces` table rows replaced (no duplicates). Verify with:
```bash
sqlite3 ~/.photosift/photosift.db "SELECT photo_id, COUNT(*) FROM faces GROUP BY photo_id HAVING COUNT(*) > 1;"
```
Expected: empty result.

## 8. Provider status + graceful fallback

- [ ] Rename `~/.photosift/models/yunet.onnx` → restart app → `providerStatus` becomes `"disabled"` (check with `invoke("get_ai_status")` in devtools or observe UI hiding AI surfaces).
- [ ] UI affordances gone: no AI panel, no heatmap, no AI-pick badge, no progress indicator.
- [ ] Culling still works normally — flags, ratings, undo, export all functional.
- [ ] Relaunch app with the model renamed back → AI surfaces return.
- [ ] Force CPU (can test by deleting CUDA DLLs or setting env var) → `providerStatus === "cpu"`, AI still works, just slower.

## 9. Failure recovery

- [ ] Force a preview file corruption — `echo garbage > ~/.photosift/cache/<shoot>/previews/42.jpg` — re-analyze the shoot → that one photo reports failed, others complete.
- [ ] `failedCount` increments visibly in the Toolbar indicator.
- [ ] App does not crash. Log shows the per-photo error.

## 10. Screenshots (for docs + future regression diffs)

Save under `docs/phase2-ai-screenshots/`:

- [ ] `loupe-panel.png` — AI panel open on a 1-face portrait.
- [ ] `loupe-heatmap.png` — heatmap overlay active on a sharp portrait.
- [ ] `loupe-both.png` — `F` + `H` active simultaneously.
- [ ] `grid-aipick.png` — grid view with `★ AI` badge visible on one member of a near-dup group.
- [ ] `settings-ai.png` — Settings dialog showing the AI section with sliders + Re-analyze button.
- [ ] `toolbar-progress.png` — Toolbar during active analysis showing "Analyzing 47/412" + cancel.

## Known gaps (for follow-up work, not blockers for this checklist)

- **Eye classifier** — `MockEyeProvider` alternates deterministically. Once a real eye open/closed model is sourced, swap `OnnxEyeStateProvider` in `lib.rs`.
- **Fixture-based face tests** — the live smoke test is `#[ignore]`-gated (runs with `cargo test -- --ignored`). A proper fixture pair (portrait with known face + landscape) asserting detection count would add regression coverage for the decoder — not blocking.
- **Cross-platform inference** — Windows + CUDA/CPU only. macOS (CoreML) out of scope.
- **ORT dylib on test hosts** — `load-dynamic` requires `ORT_DYLIB_PATH` or the onnxruntime DLL on PATH for live inference. Production builds work because Tauri bundles the DLL; `cargo test -- --ignored` from a fresh checkout may need `ORT_DYLIB_PATH` set.

## Exit criteria

- All items in sections 0–7 checked on a real ≥500-photo shoot.
- Section 8 verified at least once per environment (CUDA, CPU, disabled).
- Section 10 screenshots committed.
- Known gaps opened as separate plan items / follow-up tasks.
