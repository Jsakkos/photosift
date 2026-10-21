# Evaluation harness

`photosift-eval` scores picks for past shoots against what you actually
exported. Each shoot's `Export\` folder is the answer key: an export is mapped
back to its original by file stem (ignoring Lightroom's `-Edit`, `-2` and
` (2)` suffixes), then by EXIF `DateTimeOriginal` + camera. Exports that match
neither way are listed by name in the shoot's row, never dropped.

## It never writes into your library

The harness indexes originals **in place**: no copies, no sidecars, no Recycle
Bin, no new folders. `Export\` is never indexed as originals. The app state it
runs on has `read_only` set, so XMP writes and trash refuse outright.
Everything it writes goes under the eval dir:

| Path | What |
|---|---|
| `<eval dir>\root\` | scratch data root: `photosift.db`, `settings.toml`, `cache\` previews, `models\` |
| `<eval dir>\results.jsonl` | one JSON line per scored shoot |

The eval dir is `PHOTOSIFT_EVAL_DIR`, default `<repo>\.photosift-eval\`
(gitignored). The scratch database is kept between invocations, so a shoot is
indexed and enriched once; later runs reuse it. Delete `root\` to start over.
The scratch `settings.toml` always has `helper_enabled = false` (runs are only
started explicitly, one per shoot) and a library root inside `root\`.

## Setup

1. List your shoots: copy `tests/eval/shoots.example.txt` to
   `tests/eval/shoots.txt` (gitignored) and put one shoot folder per line.
   **Only shoots that have an `Export\` folder**; others are skipped.
2. Models (optional but recommended: faces feed the signals): copy
   `face_detection_yunet_2023mar.onnx` and `face_recognition_sface_2021dec.onnx`
   into `<eval dir>\root\models\`, e.g. from `%USERPROFILE%\.photosift-dev\models`
   after `scripts\fetch-models.ps1 -Dev`.

## Run

```powershell
# Baseline: signals only, no Claude. Keeps the top frame of the |truth| best bursts.
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- run --max-shoots 3 --label baseline-signals

# The helper: one real helper run per shoot.
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- run --helper --max-shoots 3 --label helper-v1

# Means per label, side by side.
cargo run --release --manifest-path server/Cargo.toml --bin photosift-eval -- report --label baseline-signals --label helper-v1
```

`run` takes `--shoots <file>` (default `tests/eval/shoots.txt`), `--helper`,
`--max-shoots N` and `--label TEXT` (default `baseline` / `helper`).

**Cost warning:** every `--helper` shoot is one full helper run on your Claude
login (the `helper_model`, `helper_max_turns` and `helper_max_budget_usd` of the
scratch `settings.toml` apply). Start with `--max-shoots 3` and read the token
and cost columns before running more.

## Reading the table

Shoots are named by `shoot` = the first 8 hex chars of the SHA-256 of the
folder path, never by folder name.

| Column | Meaning |
|---|---|
| `truth` | originals with at least one matched export |
| `unm` | exports that matched no original (listed under the table) |
| `keep` | picks: album + library proposals (helper) or the baseline's frames |
| `exact` | picks that are truth photos |
| `near` | other picks paired one-to-one with a missed truth photo in the same burst |
| `missed`, `false` | truth photos not covered; picks neither exact nor near |
| `prec`, `rec`, `rec~` | exact / keep, exact / truth, (exact + near) / truth |
| `tokens`, `cost` | the helper run's input + output tokens and reported cost |

`results.jsonl` also carries the album-only score, the run's status, error,
cache-read tokens, `prompt_version`, `model` and a hash of the scratch settings.
`report` counts a shoot once per label, by its latest row.
