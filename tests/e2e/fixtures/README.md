# E2E screenshot fixtures

Deterministic input data for the screenshot-CI suite (issue #20).

## What's here

| Path | What | When to regenerate |
|---|---|---|
| `generate.mjs` | Sharp-based generator for `img/*.jpg` | Edit the script, then run `npm run fixtures:generate`. Commit the output. |
| `img/sample_NN.jpg` | 12 deterministic preview JPEGs (1600×1067, ~25KB ea) | Only when `generate.mjs` changes — output is byte-stable for the same sharp version. |
| `e2e.css` | Injected into every screenshot session — kills subpixel AA, caret blinking, animations | Edit when you find a new source of nondeterminism. |

## How the fixtures are consumed

1. Test runner sets two env vars before launching the binary:
   - `PHOTOSIFT_HOME=<runner.temp>\photosift-ci` — redirects the app's state directory.
   - `PHOTOSIFT_E2E_FIXTURES=<repo>/tests/e2e/fixtures/img` — tells the testing seeder where to find the source JPEGs.
2. A spec calls `seedFixture({ shoots: [...] })` (helpers/screenshot-helpers.mjs).
3. That invokes the debug-only `seed_test_fixtures` Tauri command (src-tauri/src/commands/testing.rs).
4. The seeder truncates the DB, inserts the requested rows, and copies each photo's preview JPEG from `PHOTOSIFT_E2E_FIXTURES/<fixture>.jpg` to the cache path the `photosift://` protocol handler reads.

So fixture content is repo-relative, deterministic, and tiny. No real RAW files needed — the UI doesn't decode RAWs at runtime.

## Why not generate JPEGs on the fly per test?

Two reasons: (1) Sharp's first-call startup is ~200ms which adds up across 50+ shots, and (2) baseline PNGs in `tests/e2e/__snapshots__/` reference the *pixel content* of these fixtures, so any drift in the generator would silently rebuild every baseline. Committed JPEGs make the dependency explicit.

## Adding a new fixture image

1. Bump the loop count in `generate.mjs` and add a hue if needed.
2. `npm run fixtures:generate`.
3. Commit `img/sample_NN.jpg`.
4. Reference `"sample_NN.jpg"` in any `SeedPhoto.fixture` field from a spec.
