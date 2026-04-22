# PhotoSift AI benchmark

A test suite for measuring — and regressing against — the three AI signals
in PhotoSift: **face detection**, **perceptual grouping**, and
**focus/sharpness scoring**. The flow is:

1. **Scan** your RAW library and sample a stratified test set.
2. **Label** ground truth for each image in a spreadsheet.
3. **Import** the labeled shoots in PhotoSift so the AI runs over them.
4. **Run** the harness; it compares PhotoSift's predictions against your
   labels and writes metrics back into the workbook and a markdown report.

## Setup

```bash
pip install openpyxl exifread
python build_workbook.py           # creates photosift_benchmark.xlsx
python sample_library.py --library /path/to/DSLR --target 150
# open photosift_benchmark.xlsx and label
# ...then, after importing the shoots in PhotoSift and letting AI analysis finish:
python run_benchmark.py --db ~/.photosift/photosift.db
```

`benchmark_report.md` and the `Results` sheet in the workbook will refresh on
every harness run.

## Sampling plan

The sampler stratifies across shoot types because the failure modes you
described — missed faces, weird groupings, variable focus scores — each
cluster around different photo characteristics. If the test set is 80%
landscapes, face-detection numbers won't reflect portrait-session
experience.

Strata (auto-classified per shoot from EXIF medians):

- `portraits_studio` — short lens, f/≤2.2, low ISO
- `portraits_available` — short lens, f/≤2.8, mixed ISO
- `events_low_light` — median ISO ≥ 3200
- `landscapes` — f/≥8, focal ≤ 50mm
- `action_bursts` — ≥30% of adjacent frames ≤2s apart
- `wildlife` — median focal ≥ 200mm
- `mixed` — fallback

Default target is 150 images split evenly across present strata. Tune with
`--target`. For meaningful per-slice stats on face detection, aim for ≥20
labeled images in each stratum you care about.

## Labeling protocol

Keep the workbook open in Excel (or LibreOffice) and label blind — don't
peek at PhotoSift's predictions before filling in your own.

### Faces sheet

Fill these columns per row:

- `face_count_true` — integer. Count every face a human would identify
  when looking at a 1500px-wide render. Tiny background faces count. Full
  profile counts. Sunglasses count. Partial occlusion counts if >50% of the
  face is visible.
- `has_profile`, `has_small_face`, `has_occlusion`, `has_sunglasses`,
  `has_low_light`, `has_motion`, `has_backlit` — Y/N flags. Mark every
  condition that applies so the harness can compute per-slice recall. A
  face is "small" if it's under ~2% of frame height.
- `unsure` — Y to exclude this row from metrics but keep it for review.
- `notes` — free text.

The harness writes `face_count_pred`, `max_confidence`, `min_confidence`,
`error_type`, and `abs_error`.

**What the harness measures.** Face-presence recall/precision/F1 and
count-MAE overall; plus recall restricted to rows where each condition
flag is Y. A systematically high `missed` rate on `has_small_face=Y` rows
points straight at the YuNet confidence threshold (currently 0.6 in
`src-tauri/src/ai/face.rs`) — lowering it trades precision for recall.

### Groups sheet

- `cluster_id_true` — an integer you assign. Images you'd expect to
  cluster together get the same integer. Unique shots get a unique integer.
  The numbers are arbitrary but must be consistent within this sheet.
- `expected_link` — `near_dup` (burst / identical frame), `related`
  (reframe, exposure bracket, small recompose), or `unique`.
- `burst_type` — `burst`, `reframe`, `bracket`, `panorama`, `timelapse`,
  `unique`, or `other`. Helps diagnose which kind of similarity the pHash
  is missing.
- `unsure`, `notes`.

The harness computes **Adjusted Rand Index** against your clustering plus
**pair-based precision/recall/F1** (treating each image pair as a binary
"same cluster" classification). It also writes, per image, the **min
hamming distance** to another image in your labeled set — split into
intra-cluster (should be small) and inter-cluster (should be large). If
intra distances overlap with inter distances heavily, the pHash thresholds
(4 / 12) need tuning, not re-clustering.

### Sharpness sheet

- `focus_score_true` — integer 1–10. See the reference at the bottom of
  the sheet. Key anchor: **7 = acceptable, prints fine but edges aren't
  crisp; 10 = tack sharp at 100%**.
- `category_true` — one of: `tack`, `good`, `acceptable_soft`,
  `motion_blur`, `oof` (out of focus), `shake`. Pick the closest
  description of the failure mode, not just "blurry".
- `subject_type` — what the focus is supposed to be on.
- `unsure`, `notes`.

The harness computes **Spearman ρ** between your 1–10 and PhotoSift's raw
sharpness (Laplacian variance over the whole preview), **MAE on the 1–10
scale** after percentile-mapping the raw sharpness into 1–10, and
**category accuracy** via a rule-based bucketing of the percentile.

## Why the variability you're seeing

PhotoSift's whole-image sharpness is `laplacian_variance()` over the
grayscale preview (see `src-tauri/src/ai/sharpness.rs`), normalized
against a fixed calibration scale and then bucketed per-shoot into
percentiles at display time. Two related sources of apparent variance:

1. **Absolute Laplacian variance is scene-dependent.** A detailed
   landscape has much higher raw variance than a clean studio portrait
   even if both are tack-sharp. The UI uses per-shoot percentiles (see
   `sharpness_percentiles_for_shoot`) to normalize, but if you're
   comparing raw scores across shoots, expect swings.
2. **Eye sharpness uses the same metric on a tiny crop.** A 40×40 pixel
   eye patch has far fewer edges to average over, so per-eye sharpness is
   noisier than whole-image sharpness. The `face_mean_eye_sharp` column
   the harness writes is useful for confirming this: the Spearman ρ vs
   your 1–10 score should be materially worse than whole-image ρ.

The harness also surfaces the **mock eye-open signal** explicitly. The
current `MockEyeProvider` alternates 0/1 (documented in
`docs/phase2-ai-qa.md` and `src/ai/mod.rs`), so `eyes_open_count` in
the DB is not meaningful. Any metric that trusts it will look like noise —
because it is noise.

## Harness output

`benchmark_report.md` is a short markdown summary. The `Results` sheet
in the workbook has every metric as a row with `n`. `_RunMeta` captures
the run context (DB path, timestamps, thresholds).

Per-row predictions are written back onto the Faces / Groups / Sharpness
sheets too — filter and sort there to find the specific images that
PhotoSift got wrong. The `Faces.error_type` column in particular is the
fastest way to find "images PhotoSift missed faces in" for a triage session.

## Suggested cadence

Treat this as a regression suite:

1. Freeze the labeled workbook under version control (commit the xlsx).
2. After any change to face confidence thresholds, pHash cutoffs, or the
   sharpness kernel, re-run `run_benchmark.py` and diff the `Results`
   sheet / report.
3. Before merging a new model (e.g. once the ONNX eye classifier lands),
   gate on non-regression of the existing metrics plus a minimum bar on
   whichever metrics the new model is supposed to improve.

## Proposed acceptance thresholds (starting point)

These are reasonable bars to ship against, not hard requirements. Tune
once you have a first run's numbers to anchor against.

| Feature | Metric | Bar |
|---|---|---|
| Faces | Presence recall (overall) | ≥ 0.95 |
| Faces | Recall on `has_small_face=Y` | ≥ 0.70 |
| Faces | Recall on `has_profile=Y` | ≥ 0.75 |
| Faces | Count MAE | ≤ 0.5 |
| Groups | ARI | ≥ 0.75 |
| Groups | Pair F1 | ≥ 0.80 |
| Sharpness | Spearman ρ (whole-image) | ≥ 0.60 |
| Sharpness | Category accuracy | ≥ 0.55 |

Sharpness bars are deliberately softer: human scoring of focus has real
inter-rater disagreement, and compressing a 10-point scale into a 6-way
category is lossy.

## File layout

```
benchmark/
├── build_workbook.py            # one-shot workbook builder
├── sample_library.py            # scan + stratify + pre-populate
├── run_benchmark.py             # DB join + metrics + report
├── _smoke_test.py               # end-to-end sanity check against a synthetic DB
├── photosift_benchmark.xlsx     # the labeling + results workbook
├── benchmark_report.md          # regenerated each run
└── README.md                    # you are here
```

Run `PHOTOSIFT_BENCH_FIXTURE=/tmp/smoke python _smoke_test.py` before trusting
a real run — it builds a tiny synthetic DB with known-outcome predictions
and asserts the harness recovers them. The workbook and DB it produces live
under `$PHOTOSIFT_BENCH_FIXTURE` and can be thrown away.
