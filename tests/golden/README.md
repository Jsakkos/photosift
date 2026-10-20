# Golden card fixture

A real-camera regression check that lives **outside git**: the repository is
public, so no photos, previews or real file names are ever committed. Only
`manifest.example.txt` (placeholder paths) is tracked.

## Build the fake card

1. Copy `manifest.example.txt` to `manifest.txt` (gitignored) and replace the
   placeholders with absolute paths to your own RAFs/NEFs. Lines starting with
   `#` are comments. A same-stem `.JPG` next to a RAW is copied with it.
2. Run:

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\make-golden-fixture.ps1
   ```

   This writes `<repo>\.photosift-golden\card\DCIM\100_FUJI\*.RAF|*.JPG`,
   `...\DCIM\100NIKON\*.NEF` and `expected.toml` (gitignored). Set
   `PHOTOSIFT_GOLDEN_DIR` (or pass `-Out`) to build it elsewhere. Re-running
   rebuilds the card from scratch; your originals are only read.

`expected.toml` holds `photos` (RAW count, computed), `cameras` and `events`
(fixed by the manifest design: two camera bodies; three proposed events with
zero camera offsets). If you build a different mix, edit those two numbers.

## Run the checks

```powershell
$env:PHOTOSIFT_GOLDEN_DIR = "$PWD\.photosift-golden\card"
cargo test --manifest-path server/Cargo.toml --test golden --test media -- --ignored
```

- `media::golden_raf_and_nef` - embedded JPEG extraction and EXIF on every RAW.
- `golden::golden_card_scans_and_imports` - scans the card, imports it into a
  temp library, then checks every photo has a preview and pHash, every scanned
  item has an EXIF capture time, and the portrait shots (see
  `PORTRAIT_STEMS`) have previews taller than wide. Adjust that list in
  `server/tests/golden.rs` if your set differs.

Both tests are `#[ignore]` and fail loudly when `PHOTOSIFT_GOLDEN_DIR` is unset.
