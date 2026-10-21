# Stack bench — Rust/axum vs Python/FastAPI (2026-10-19)

Throwaway spike. Same feature slice in both: ingest (read, SHA-256, embedded-JPEG
extract, EXIF, decode, 2560px preview + 400px thumb, pHash, SQLite), enrich
(YuNet detect + SFace embeddings via ONNX Runtime 1.24, sharpness, exposure),
serve (previews/thumbs over HTTP to a shared plain-JS viewer).

**Data:** `<library>/2026/2026-10_<party-shoot>` — 777 files,
22 GB, 405 photos (372 X100VI RAF+JPG, 33 D750 NEF). Read-only.
**Machine:** Ryzen 7 9700X (16 threads), 32 GB RAM, RTX 3090, source on SATA SSD.
**Parity check:** identical pHashes, identical face counts per photo, embeddings
cosine ≥ 0.99, sharpness within ~10% (different resample filters).

## Results (two alternating runs each)

| Measure | Rust | Python | Ratio |
|---|---|---|---|
| Ingest 405 photos / 22 GB (wall) | 63.8 / 65.5 s | 71.4 / 96.6 s | 1.1–1.5× |
| ↳ CPU work summed (decode+resize+encode) | ~45 s | ~85 s | 1.9× |
| ↳ peak RAM during ingest | 0.8–1.2 GB | 2.1–3.1 GB | 2.7× |
| Enrich CPU (668 faces) | 5.1 / 5.3 s | 9.0 / 9.5 s | 1.8× |
| Enrich CUDA | 2.1 / 2.1 s | 4.0 / 4.1 s | 1.9× |
| Model load (CUDA, one-time) | 1.6 s | 1.5 s | ≈ |
| Startup to first HTTP 200 | 0.09–0.35 s | 1.7–2.2 s | ~8× |
| Idle RAM / RAM with models loaded | 12 MB / 880 MB | 97 MB / 1000 MB | |
| HTTP preview GET p50 / p99 (no browser) | 0.9 / 17–20 ms | 2.6 / 15–16 ms | |
| 160 concurrent preview GETs | 0.32–0.34 s | 0.43–0.44 s | 1.3× |
| **Browser: → to painted, no cache** p50/p99 | **24.9 / 58 ms** | **25.0 / 42 ms** | ≈ |
| **Browser: → to painted, prefetch 3** p50/p99 | **13.7 / 18 ms** | **12.7 / 18 ms** | ≈ |
| Browser: held arrow (33 ms repeat), frames painted | 100/100 | 99–100/100 | ≈ |
| Browser: 405-thumb grid fully loaded | 0.41 s | 0.74 s | 1.8× |
| Code (non-blank lines) | 622 | 309 | 2× |
| Build/runtime fix rounds | 2 compile errors, 0 runtime bugs | 1 silent data bug (numpy float32 stored as BLOB) + 3 CUDA/DLL env rounds | |
| Deployable footprint | 27 MB exe (+ORT/CUDA DLLs) | 554 MB venv (+CUDA DLLs) | |

## Reading it

- **What the user feels is identical.** Arrow-key latency is dominated by the
  browser decoding a 2560px JPEG (~13 ms prefetched, ~25 ms cold), not by the server.
- **Ingest is disk-bound in both** (~340 MB/s off a SATA SSD); from an SD card it
  will be even more so. Python does ~2× the CPU work for the same output.
- **Rust is ~2× faster at enrichment and uses ~⅓ the memory**, starts instantly.
- **Python is half the code**, but its CUDA setup was the fiddliest part of the spike,
  and its one real bug was silent (wrong SQLite type) where Rust's compiler caught
  both of its errors before anything ran.

Not covered: EXIF orientation (both show portraits sideways), cold-cache control,
SD-card sources.
