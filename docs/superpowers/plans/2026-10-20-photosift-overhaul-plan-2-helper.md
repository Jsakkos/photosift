# PhotoSift Overhaul — Plan 2: Claude Helper + Evaluation Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After every import, a Claude helper running on the user's own subscription drafts the shoot's outcomes. Each proposal carries a one-line reason, and near-identical frames come back as "your call" pairs. It learns from the user's accepts and overrides through an editable taste profile. An evaluation harness scores it, read-only, against the user's historical `Export\` folders.

**Architecture:** The server gains an MCP endpoint (`POST /mcp`, stateless JSON-RPC) over the same operations as the HTTP API. It also gains a helper runner that spawns the official `claude -p` CLI with that endpoint attached: once in the background per enriched shoot, and once per message from a chat panel. Claude never writes outcomes. It writes **proposals** (its own table), and the user accepts or overrides them on the review screen. Each accept or override becomes a labelled example fed into the next run. A dev-only binary (`photosift-eval`) indexes past shoots in place into a scratch data root, runs the helper (or a signals-only baseline), and scores the result against the user's exports. The same binary sweeps the burst thresholds.

**Tech Stack:** Plan 1's stack, unchanged (Rust axum/tokio/rusqlite/ort, React 19 + Vite + Vitest + Playwright). New: the `claude` CLI (Claude Code ≥ 2.1.259, the first version with `--permission-prompts`), spawned as a child process. The only new crates are `uuid` (session ids) and `base64` (MCP image content). There is no MCP SDK crate: the protocol subset needed is about 200 lines (Task 2).

**Spec:** `docs/superpowers/specs/2026-10-19-photosift-overhaul-design.md` (§8 helper, §9 harness, §10 review screen, §12 risks). Plan 1 (`docs/superpowers/plans/2026-10-19-photosift-overhaul-plan-1-foundation.md`) is shipped; its SDD ledger rulings R14 (re-enrich) and R16 (burst calibration) are carried here.

## Roadmap position

| Plan | Delivers | Status |
|---|---|---|
| 1 — Foundation | import, enrichment, review, XMP, delete | shipped |
| **2 — Helper + evaluation** (this doc) | usage spike, MCP endpoint, `claude -p` runner, proposals + reasons + your-call pairs, face-crop expression judgments, taste profile + labelled overrides, chat panel, re-enrich (R14), eval harness, burst calibration (R16) | — |
| 3 — Albums + Immich | album membership, `Export\` JPEGs, Immich sync, "Review album" | after 2 |
| 4 — People | face naming, `People\|<name>` | after 3 |

"Usable" (spec §2) = Plan 1 + Plan 2, **and** the harness shows helper picks good enough that the user trusts the draft (Task 10 sets the bar from the baseline).

**Out of scope (deliberately):** helper-proposed event names on the import screen (spec §5.3; that needs a synchronous Claude call while the user waits at the card, so it waits for Plan 3, which adds album naming). Also out: album membership (Plan 3), the optional local ranking model (spec §8 learning item 3; revisit only if the harness shows Claude can't pre-sort cheaply), and local eye/smile ONNX classifiers (see Global Constraints).

## Global Constraints

- **The GitHub repo is public.** Never commit photos, previews, face crops, embeddings, sidecars copied from the library, real library paths, shoot folder names, personal names, chat transcripts, taste-profile text, or eval results. Eval inputs and outputs live outside git (`PHOTOSIFT_EVAL_DIR`, default `<repo>\.photosift-eval\`, gitignored). Spike results committed to git contain numbers only.
- Windows 11 only. The server binds `127.0.0.1` only. Ports: release `7420`, debug `7421`. Every new route sits behind `api/guard.rs` (Host/Origin check), including `/mcp`.
- **Claude transport:** PhotoSift spawns only the official `claude` executable on the user's machine, under the user's own login. It never reads, stores, copies or forwards Claude credentials or OAuth tokens. It never passes `--bare` (bare mode ignores the subscription login). The transport sits behind one trait so it can be swapped (Task 4), as spec §8 Fallback requires.
- **The helper proposes; the user decides.** Helper tools write the `proposals`, `your_call_pairs` and `helper_runs` tables only. The runner denies the MCP `set_outcome` tool. No XMP or trash action ever originates from Claude. "AI enriches, never hides or auto-decides."
- **Budget principle (spec §8):** Claude sees downsized images only. Thumbnails are 400 px. Previews are capped at 1024 px long edge, at most 8 per call. Face crops are 256 px, at most 24 per call. Claude looks only at shortlists: per burst, the top 3 frames by `signal_score` unless it asks for more. Every run passes `--max-budget-usd` and `--max-turns` (values set by Task 0).
- **Expression = Claude's read of face crops.** No eye/smile ONNX models ship (spec §12 "Expression models": none trustworthy exist; the prototype's were mocks). The prompt states "expression beats sharpness" (spec §6). The harness measures whether face crops pay for their tokens.
- Outcomes stay `unreviewed | album | library | archive | delete` + `blooper`. A proposal's outcome is one of `album | library | archive | delete` (never `unreviewed`). The XMP mapping is unchanged.
- Files are never moved or renamed; original EXIF is never rewritten; the card is never written to. **The eval harness never writes into a library folder** (no copies, no sidecars, no trash). It indexes in place and writes only to its scratch data root.
- Schema changes are new migration steps keyed on `PRAGMA user_version` (v2, v3, …). `schema.sql` (v1) is never edited.
- Every server response change is mirrored in `web/src/api/types.ts` in the same commit. New key bindings go in `web/src/review/keyboard.ts` `BINDINGS` (the `?` overlay renders from it).
- Logging via `plog!("helper", …)` / `plog!("mcp", …)` / `plog!("eval", …)`. Never log image bytes, prompts containing the taste profile, or chat text at info level.
- Rust: rustfmt + clippy `-D warnings`; `anyhow` at handler boundaries, `thiserror` inside modules. TS strict, no `any`. Conventional commits. Verification loop per `CLAUDE.md` (Playwright or the in-app browser for UI, read the screenshots yourself).

## Review Focus

1. **Claude is missing, logged out, or hits a usage limit mid-run.** The run ends `failed` with a plain reason ("Claude usage limit reached — try again after it resets"). Proposals already written stay. The review screen works exactly as in Plan 1 and offers "Retry helper". → Task 4 tests `cli_missing_reports_unavailable`, `rate_limit_result_fails_run_keeps_proposals`; Task 5 e2e `helper_failure_is_visible_and_retryable`.
2. **The user decides photos while the background run is still proposing.** A user outcome is never overwritten. A later proposal for an already-decided photo is stored but shown only as a suggestion, and no XMP is written by the helper. → Task 3 test `proposal_never_changes_outcome`; Task 5 e2e `deciding_during_run_keeps_user_choice`.
3. **Claude sends bad tool input:** an id from another shoot, a trashed photo, outcome `"pick"`, a 300-character reason, or a call against a run that has already finished (an orphaned `claude` child after a server restart). Each comes back as an MCP tool error (`isError: true`) naming the problem, and none of it is written. → Task 3 tests `propose_rejects_foreign_trashed_and_invalid`, `writes_to_finished_run_rejected`; Task 4 test `startup_marks_running_runs_interrupted`.
4. **A shoot grows after it was curated** (the rolling `Everyday` folder gets a second import). Enrichment of the new photos queues a new run whose prompt names the new photo ids. Earlier proposals and every user decision survive. → Task 4 test `second_import_queues_run_for_new_photos`.
5. **A historical shoot's exports don't map cleanly to originals.** This covers renamed exports, exports with no EXIF, a shoot with a `RAW\` subfolder, and prototype leftovers (`selects\`, `rejects\`). Unmatched exports are counted and listed by file name in the shoot's report, never silently dropped. `Export\` is never indexed as originals. → Task 8 tests `export_matching_by_stem_then_exif`, `unmatched_exports_reported`, `export_dir_never_indexed`.

---

## File Structure

```
server/src/
  db/migrations.rs            NEW  migration steps v2.. (v1 stays schema.sql); db::migrate calls it
  db/helper.rs                NEW  helper_runs, proposals, your_call_pairs, decisions, taste, chat CRUD
  enrich/job.rs               MOD  faces_model stamp + re-enrich queue (R14)
  media/crops.rs              NEW  face_crop(), downsized_preview() for MCP images
  mcp/mod.rs                  NEW  POST /mcp JSON-RPC: initialize, tools/list, tools/call, ping
  mcp/tools.rs                NEW  tool table: name, description, input schema, handler
  helper/mod.rs               NEW  HelperService (queue, status), RunKind, RunStatus
  helper/transport.rs         NEW  trait Transport; ClaudeCli; command-line builder
  helper/stream.rs            NEW  parse claude stream-json lines → RunEvent (pure)
  helper/prompt.rs            NEW  system prompt assembly (template + taste + examples)
  helper/prompt.md            NEW  the system prompt template (include_str!)
  helper/learning.rs          NEW  record_decision(), recent_examples()
  api/helper.rs               NEW  /api/shoots/{id}/helper/*, /api/taste, /api/photos/{id}/decision-note
  api/{mod,photos,shoots}.rs  MOD  routes; decision recording; review gains proposals + helper block
  db/review.rs                MOD  ReviewPhoto.proposal, ShootReview.helper
  events.rs                   MOD  HelperStatus, ProposalsChanged, ChatMessage events
  config.rs                   MOD  helper settings + PHOTOSIFT_CLAUDE_BIN
  ingest/scan.rs, ingest/job.rs  MOD  ScanOptions{exclude_dirs}, ImportMode::IndexInPlace, Settings.read_only guard
  xmp.rs, trash.rs            MOD  refuse when Settings.read_only
  eval/{mod,exports,score,sweep}.rs  NEW  harness library (exports matching, scoring, burst sweep)
  bin/eval.rs                 NEW  photosift-eval CLI
  bin/fake-claude.rs          NEW  test double for the claude CLI (scripted MCP calls + stream-json)
server/tests/
  migrate.rs, reenrich.rs, mcp.rs, helper_data.rs, helper_runner.rs, eval.rs   NEW
web/src/
  api/{types,client}.ts       MOD  proposals, helper block, chat, taste
  review/{PhotoTile,Summary,MomentSection,keyboard,model,useReview}.tsx/ts  MOD
  review/YourCallPair.tsx     NEW  pair with tradeoff line
  review/HelperPanel.tsx      NEW  chat drawer
  review/OverrideWhy.tsx      NEW  criterion chips after an override
  pages/TastePage.tsx         NEW  /taste: profile editor + recent decisions
web/tests/e2e/
  helper.spec.ts, chat.spec.ts, taste.spec.ts   NEW (fake-claude via PHOTOSIFT_CLAUDE_BIN)
spikes/helper-usage/
  run.ps1, RESULTS.md         NEW  (Task 0; RESULTS has numbers only)
```

---

### Task 0: Subscription-route and usage spike (gate)

Produces a decision, not product code. Everything after this task depends on its answers. If the gate fails, Tasks 4-6 swap the transport (see Step 6) and the rest of the plan stands.

**Files:**
- Create: `spikes/helper-usage/run.ps1`, `spikes/helper-usage/RESULTS.md`
- Modify: `.gitignore` (`.photosift-eval/`, `spikes/helper-usage/out/`)

**Interfaces:**
- Produces, recorded in `RESULTS.md` and consumed by Tasks 4 and 8:
  - `transport`: `cli-subscription` | `cli-api-key` | `manual-desktop`, which becomes the Task 4 setting `helper_transport` = `"cli"` | `"cli-api-key"` | `"off"`
  - `helper_model` default, `helper_max_turns`, `helper_max_budget_usd`
  - the verified flag list (Step 4)
  - the per-shoot token and time numbers.

Context as of 2026-10-20, for the executor to re-verify rather than trust:
- `code.claude.com/docs/en/headless` says `--bare` "doesn't use your subscription login". It also says `--output-format json`/`stream-json` results carry `total_cost_usd`, `usage` and `session_id` (client-side estimates).
- `support.claude.com/en/articles/15036540` has a 2026-06-15 note saying the announced Agent SDK / `claude -p` billing change was paused. It also has a 2026-10-07 update saying Max and Team plans include monthly API credits that cover the Agent SDK and `claude -p`.
- Third-party coverage disagrees with itself.

- [ ] **Step 1: Docs and terms check.** Read the current versions of: the headless page, the CLI reference, the support article above (and the "Monthly API credits for Max and Team plans" article it links), and Anthropic's consumer terms / usage policy sections on automated access. In `RESULTS.md`, record the date, the URLs, and a one-paragraph paraphrase of each (no long quotes). Answer explicitly: (a) does `claude -p` on the user's plan draw on plan limits, a separate credit, or both; (b) is a local app spawning the official CLI under the user's login permitted; (c) what happens at exhaustion. If (b) is "no" or unclear, stop and ask the user before Step 3.
- [ ] **Step 2: Auth check.** Run `claude --version` (must be ≥ 2.1.259) and `claude auth status`. Record `authMethod` (expect `claude.ai`).
- [ ] **Step 3: Write `run.ps1 -Source <folder of camera JPGs> -Model <alias> [-Crops]`.**
  - The script makes 400 px thumbnails into `spikes/helper-usage/out/thumbs/` (System.Drawing; orientation can be ignored here).
  - It groups them into moments by a 90 s EXIF-time gap (DateTimeOriginal via `System.Drawing.Imaging` property 0x9003).
  - It runs `claude -p` with the Step 4 flags plus `--tools Read --add-dir out` and a prompt. The prompt asks for the album picks with one-line reasons and your-call pairs, as `--json-schema` structured output.
  - With `-Crops`, it also writes 1024 px versions of the 30 photos Claude shortlists and runs a second pass.
  - It appends one JSON line per run to `out/runs.jsonl`, from the `result` message: `{model, photos, num_turns, duration_ms, total_cost_usd, usage}`.
- [ ] **Step 4: Verify the isolation flags** on a 10-photo run with `--output-format stream-json --verbose`. Flags: `--strict-mcp-config`, `--tools ""` (or `Read` for the spike), `--setting-sources local`, `--disable-slash-commands`, `--permission-mode dontAsk`, `--permission-prompts none`, `--system-prompt-file`, `--max-turns`, `--max-budget-usd`, `--session-id`, `--resume`.
  - Run it from an empty working directory.
  - In the `system/init` event, confirm: no plugins, no MCP servers other than those passed, the tool list is as restricted, and auth still works without `--bare`.
  - Record which user-level context still leaks in (for example `~/.claude/CLAUDE.md`), and whether `--setting-sources local` suppresses user hooks.
  - Record the exact final flag list.
- [ ] **Step 5: Measure one full shoot.** Use the Fuji JPGs of the ~400-photo 2026-10 party shoot (outside git) with `sonnet`, then with `opus`, each with and without `-Crops`.
  - Before and after each run, the user reads their plan's usage meter in claude.ai Settings → Usage (ask them; you cannot see it), and you record the delta.
  - In `RESULTS.md` (numbers only, no file or folder names), record per run: tokens in/out/cache, `total_cost_usd`, wall time, turns, and the meter delta.
- [ ] **Step 6: Decide and record** the gate in `RESULTS.md`:
  - **Pass:** one 400-photo run (with crops) uses ≤ 10% of the user's weekly/monthly allowance, or ≤ 25% of the separate credit. → `transport = cli-subscription`. Set `helper_model` to the cheaper model unless the user prefers the other after seeing both outputs side by side. Set `helper_max_budget_usd` = 2× the measured `total_cost_usd`, rounded up to $0.50. Set `helper_max_turns` = 2× the measured turns.
  - **Fail on cost:** the user chooses `cli-api-key` (same CLI with `ANTHROPIC_API_KEY` in the child's environment from a user-set env var; PhotoSift never stores it) or `manual-desktop` (no runner; the user points Claude Desktop/Code at `/mcp`, and Task 5's review integration still works).
  - **Fail on terms:** `manual-desktop` only.
- [ ] **Step 7: Commit** `docs(spike): claude -p subscription usage and isolation flags` (`run.ps1`, `RESULTS.md`, `.gitignore`; never `out/`).

### Task 1: Migration steps + re-enrich pass (Ruling R14)

**Files:**
- Create: `server/src/db/migrations.rs`, `server/tests/migrate.rs`, `server/tests/reenrich.rs`
- Modify: `server/src/db/mod.rs` (`migrate` delegates steps ≥ 2), `server/src/enrich/job.rs`, `server/src/db/photos.rs`

**Interfaces:**
- Produces:
  - `db::migrations::STEPS: &[(i64, &str)]`. Step 2 adds the column `photos.faces_model TEXT`. `faces_model` is NULL when the photo was enriched without a detector; otherwise it is the model stamp.
  - `enrich::FACES_MODEL: &str = "yunet-2023mar+sface-2021dec"` (identity half omitted, i.e. `"yunet-2023mar"`, when the embedder is missing).
  - `photos::shoots_needing_faces(pool, stamp: &str) -> anyhow::Result<Vec<i64>>`: shoots with a live photo where `enriched_at IS NOT NULL AND (faces_model IS NULL OR faces_model != stamp)`.
  - `photos::save_faces_only(pool, photo_id, faces: &[NewFace], stamp: &str)`: replaces that photo's faces, recomputes `face_sharpness`, sets `faces_model`, and leaves the other quality columns alone.
  - `EnrichWorker::start` enqueues those shoots when models load. `run_shoot` re-detects only those photos, then calls `moments::assign` (scores change when faces appear).
- [ ] **Step 1: Write failing tests.**
  - `migrate.rs::v1_db_upgrades_in_place`: create a v1 DB with `SCHEMA_V1` only and one photo, open it with `db::open` → `user_version == 2` (the current max), the photo survives, `faces_model` is NULL.
  - `migrate.rs::migrate_is_idempotent_across_versions`: open three times → no error.
  - `reenrich.rs::faces_added_after_models_arrive` (models from `PHOTOSIFT_MODELS_DIR`, `#[ignore]` like Plan 1's model tests): import with an empty models dir → 0 faces and `faces_model` NULL. Respawn the server with the models → after `EnrichDone`, the faces are > 0, `faces_model == FACES_MODEL`, `signal_score` changed, and `sharpness` is unchanged.
  - `reenrich.rs::no_reenrich_without_models`: an empty models dir → no shoot is queued twice (`/api/status` `queue == 0` after startup settles).
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test migrate --test reenrich` → FAIL.
- [ ] **Step 3: Implement.** Each step runs in its own `BEGIN…COMMIT` with `PRAGMA user_version = N` inside it. The normal enrichment path also writes `faces_model`.
- [ ] **Step 4: Run** → PASS; run `--ignored reenrich` locally with models present.
- [ ] **Step 5: Commit** `feat(enrich): stamp face models and re-detect photos enriched without them`

### Task 2: MCP endpoint (read operations)

**Files:**
- Create: `server/src/mcp/{mod,tools}.rs`, `server/src/media/crops.rs`, `server/tests/mcp.rs`
- Modify: `server/src/api/mod.rs` (route `/mcp` at top level, inside the guard), `server/Cargo.toml` (`base64 = "0.22"`)

**Interfaces:**
- Consumes: `db::review::{list_shoots, shoot_review}`, `photos::{get_photo, list_faces}`, cache paths, `outcome::set_and_mirror`.
- Produces:
  - **Protocol** (Streamable HTTP, stateless subset, `protocolVersion "2025-06-18"`):
    - `POST /mcp` takes one JSON-RPC 2.0 message and answers `application/json`.
    - `initialize` → `{protocolVersion, capabilities:{tools:{}}, serverInfo:{name:"photosift", version}}`.
    - `notifications/*` → 202 with an empty body.
    - `ping` → `{}`; `tools/list` → `{tools:[{name, description, inputSchema}]}`; `tools/call` → `{content:[…], isError}`.
    - `GET /mcp` → 405 (no server-initiated stream).
    - An unknown method → JSON-RPC error `-32601`.
  - Tool errors (bad input) return `isError: true` with a text explanation. Only malformed JSON-RPC is a protocol error.
  - `mcp::tools::Tool { name: &'static str, description: &'static str, schema: serde_json::Value, call: fn(&AppState, serde_json::Value) -> ToolResult }`.
  - `ToolResult { content: Vec<Content>, is_error: bool }`, where `Content::Text(String) | Content::Image { jpeg: Vec<u8> }` (serialized as `{"type":"image","data":<base64>,"mimeType":"image/jpeg"}`).
  - Read tools (all take ids, all reject ids of trashed photos):

    | Tool | Input | Returns |
    |---|---|---|
    | `list_shoots` | `{}` | text table: id, name, dates, counts, enriched |
    | `get_shoot_overview` | `{shoot_id}` | text: meta, cameras, counts, moments (idx, time range, burst count, photo count); Task 3 appends the current proposal counts |
    | `get_moment` | `{shoot_id, moment_idx, frames_per_burst?: 1-6 = 3}` | per burst: a text line per frame (`id, time, faces, face_sharpness, sharpness, clip_hi, clip_lo, signal_score, outcome`; Task 3 appends `proposal`) + the thumbnail image of the top `frames_per_burst` frames, each preceded by a text label `photo <id>` |
    | `get_photos` | `{photo_ids: [≤8]}` | per id: label + preview downsized to 1024 px long edge (q80) |
    | `get_faces` | `{photo_ids: [≤12]}` | per face (≤24 total, largest first): label `photo <id> face <n> (sharpness s)` + 256 px crop |
    | `get_taste_profile` | `{}` | the profile text (empty string until Task 3 adds the table: return `""`) |
    | `set_outcome` | `{photo_id, outcome, blooper?}` | same semantics as `POST /api/photos/{id}/outcome` (for user-driven Claude Desktop/Code; the runner denies it) |
  - `media::crops::face_crop(preview: &Rgb, face: &FaceRow, edge: u32) -> Result<Vec<u8>, MediaError>`: pads the box by 40% per side, clamps to the frame, makes a square, resizes to `edge`, and encodes q85.
  - `media::crops::downsized(preview_path: &Path, edge: u32) -> Result<Vec<u8>, MediaError>`.
- [ ] **Step 1: Write failing tests** (`server/tests/mcp.rs`, synthetic shoot via the existing test helpers):
  - `initialize_and_list_tools`: `initialize` → `protocolVersion == "2025-06-18"`; `tools/list` names exactly the 7 tools above, and every `inputSchema` has `"type":"object"`.
  - `notification_gets_202`: `notifications/initialized` → 202 with an empty body.
  - `get_moment_returns_labelled_thumbnails`: on a 6-photo, 1-burst shoot, `frames_per_burst: 3` → exactly 3 image blocks, each image preceded by a text block containing `photo <id>`, and the ids are the burst's top 3 by `signal_score`.
  - `get_photos_caps_and_downsizes`: 9 ids → `isError: true`; 2 ids → 2 images whose decoded long edge is 1024 (or less when the preview is smaller).
  - `face_crop_square_and_clamped`: unit test on a synthetic `Rgb` with a box at the corner → 256×256 output, no panic.
  - `trashed_or_unknown_ids_are_tool_errors`: `get_photos` with a trashed id → `isError: true` with a message naming the id; the HTTP status is still 200.
  - `foreign_origin_rejected`: `POST /mcp` with `Origin: http://evil.example` → 403 (the guard).
  - `set_outcome_via_mcp_writes_xmp`: same effect as the HTTP route (★4 sidecar for album).
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test mcp` → FAIL.
- [ ] **Step 3: Implement.** Handlers run in `spawn_blocking`. Image tools read cache JPEGs; no decoding happens for thumbnails.
- [ ] **Step 4: Run** → PASS. Then point a real `claude` at it once: `claude -p "list my shoots" --strict-mcp-config --mcp-config '{"mcpServers":{"photosift":{"type":"http","url":"http://127.0.0.1:7421/mcp"}}}' --allowedTools "mcp__photosift__list_shoots"` against `cargo run`. The result must contain the dev shoot list. If Claude Code needs something the stateless subset lacks (for example `Mcp-Session-Id`), add exactly that, with a test.
- [ ] **Step 5: Commit** `feat(mcp): stateless MCP endpoint over the read operations`

### Task 3: Helper data — runs, proposals, your-call pairs, taste, decisions

**Files:**
- Create: `server/src/db/helper.rs`, `server/src/helper/learning.rs`, `server/tests/helper_data.rs`
- Modify: `server/src/db/migrations.rs` (step 3), `server/src/mcp/tools.rs` (write tools), `server/src/api/photos.rs` (record decisions), `server/src/db/review.rs`, `server/src/events.rs`, `web/src/api/types.ts`

**Interfaces:**
- Produces:
  - **Schema step 3:**
    - `helper_runs(id INTEGER PK, shoot_id INT NOT NULL REFERENCES shoots, kind TEXT CHECK(kind IN ('background','chat','external','eval')), status TEXT CHECK(status IN ('queued','running','done','failed','interrupted')), session_id TEXT, model TEXT, prompt_version TEXT, started_at TEXT, finished_at TEXT, summary TEXT, error TEXT, cost_usd REAL, input_tokens INT, output_tokens INT, cache_read_tokens INT, num_turns INT)`
    - `proposals(photo_id INTEGER PRIMARY KEY REFERENCES photos ON DELETE CASCADE, run_id INT NOT NULL REFERENCES helper_runs, outcome TEXT NOT NULL CHECK(outcome IN ('album','library','archive','delete')), blooper INT NOT NULL DEFAULT 0, reason TEXT NOT NULL, created_at TEXT NOT NULL)`. This is the latest proposal per photo; a newer run overwrites it.
    - `your_call_pairs(id INTEGER PK, run_id INT NOT NULL, shoot_id INT NOT NULL, photo_a INT NOT NULL, photo_b INT NOT NULL, tradeoff TEXT NOT NULL, UNIQUE(photo_a, photo_b))`
    - `decisions(photo_id INT NOT NULL, run_id INT NOT NULL, proposed TEXT NOT NULL, proposed_reason TEXT NOT NULL, chosen TEXT NOT NULL, criterion TEXT CHECK(criterion IN ('expression','who','moment','sharpness','duplicate','composition','other')), note TEXT, decided_at TEXT NOT NULL, PRIMARY KEY(photo_id, run_id))`
    - `taste(id INTEGER PRIMARY KEY CHECK(id = 1), text TEXT NOT NULL, updated_at TEXT NOT NULL)`
    - `chat_messages(id INTEGER PK, shoot_id INT NOT NULL, role TEXT CHECK(role IN ('user','helper')), text TEXT NOT NULL, at TEXT NOT NULL)`
  - **`db::helper` functions:**
    - `create_run(pool, shoot_id, kind, model, prompt_version) -> i64` (status `queued`)
    - `set_run_status(pool, run_id, status, error: Option<&str>)`
    - `finish_run(pool, run_id, usage: &RunUsage)`, where `RunUsage { cost_usd: f64, input_tokens: i64, output_tokens: i64, cache_read_tokens: i64, num_turns: i64, session_id: String }`
    - `active_run(pool, run_id) -> anyhow::Result<RunRow>` (errors unless the status is `running`)
    - `upsert_proposals(pool, run_id, &[NewProposal])`, where `NewProposal { photo_id: i64, outcome: Outcome, blooper: bool, reason: String }`
    - `add_your_call(pool, run_id, a, b, tradeoff)`
    - `get_taste(pool) -> String`, `set_taste(pool, &str)`
    - `latest_run(pool, shoot_id) -> Option<RunRow>`
    - `shoot_proposals(pool, shoot_id) -> HashMap<i64, ProposalRow>`
    - `your_calls(pool, shoot_id) -> Vec<YourCallRow>`
  - **MCP write tools** (each takes `run_id`; each calls `active_run` first):
    - `start_run {shoot_id}` → `run_id`. Kind `external`, status `running`. Used by Claude Desktop/Code; the runner creates its own runs.
    - `propose {run_id, items: [{photo_id, outcome, reason, blooper?}]}` (≤ 50 items per call).
    - `propose_your_call {run_id, photo_a, photo_b, tradeoff}` (both photos in the same shoot, a ≠ b).
    - `finish_run {run_id, summary}` (summary ≤ 600 chars; sets status `done`).
    - `set_taste_profile {text}` (≤ 4000 chars).
  - **Validation:** every photo is in the run's shoot and not trashed; `outcome ∈ {album, library, archive, delete}`; `reason` is 1-140 chars after trimming. On any invalid item the whole call is rejected with a per-item error list, and nothing is written.
  - `get_taste_profile` now reads the table.
  - `helper::learning::record_decision(pool, photo_id, chosen: Outcome)`. If a proposal exists, it upserts `decisions(photo_id, proposal.run_id)` with `chosen`; setting `unreviewed` deletes that row. Called from the outcome HTTP handler and from MCP `set_outcome` after success.
  - `learning::set_criterion(pool, photo_id, criterion, note: Option<String>)`: applies to the latest decision row.
  - `learning::recent_examples(pool, overrides: usize, accepts: usize) -> Vec<Example>`, where `Example { proposed, proposed_reason, chosen, criterion, note }`. Newest first. An override is a row with `chosen != proposed`.
  - `POST /api/photos/{id}/decision-note {criterion, note?}` → 204, or 404 when no decision exists.
  - **Review data:**
    - `ReviewPhoto.proposal: Option<{outcome, blooper, reason, criterion: Option<String> /* from this proposal's decision row, so the override chips survive a reload */}>`
    - `ShootReview.helper: { status: "none"|"queued"|"running"|"done"|"failed"|"interrupted"|"unavailable", run_id: Option<i64>, summary: Option<String>, error: Option<String>, suggested: Counts /* proposals by outcome, unreviewed photos only */, your_calls: [{id, a, b, tradeoff}] }`
    - `"unavailable"` comes from `HelperService` (Task 4); until then it is `"none"`.
  - **Events:** `ServerEvent::ProposalsChanged { shoot_id }` (debounced by the caller: one per tool call) and `ServerEvent::HelperStatus { shoot_id, run_id, status, error: Option<String> }`.
- [ ] **Step 1: Write failing tests** (`server/tests/helper_data.rs`):
  - `proposal_never_changes_outcome` (Review Focus 2): set album on photo 1 via HTTP, then propose `delete` for it via MCP → the outcome is still album, the review's `proposal.outcome == "delete"`, the sidecar mtime is unchanged, and `suggested.delete == 0` (photo 1 isn't unreviewed).
  - `propose_rejects_foreign_trashed_and_invalid` (Review Focus 3): one call each with an id from another shoot, a trashed id, outcome `"pick"`, and a 300-char reason → `isError: true`, the text names each bad item, and `proposals` is empty.
  - `writes_to_finished_run_rejected` (Review Focus 3): after `finish_run`, `propose` → `isError: true` "run <id> is not running".
  - `newer_run_overwrites_proposal`: run 1 proposes album, run 2 proposes library → the review shows library with run 2's reason.
  - `decision_recorded_for_accept_and_override`: proposal album; the user sets album → a decision row with `chosen = proposed`; the user then sets library → the same row now has `chosen = library`; `recent_examples(10, 10)` returns it as an override; setting `unreviewed` removes the row.
  - `decision_note_sets_criterion`: POST the note → the latest row has `criterion = "expression"`.
  - `your_call_pair_round_trip`: propose a pair → the review's `helper.your_calls` contains it, and a reversed duplicate is rejected.
  - `taste_round_trip_and_cap`: set 4001 chars → error; set "prefers candid" → `get_taste_profile` returns it.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test helper_data` → FAIL.
- [ ] **Step 3: Implement**, mirroring the new response fields and events in `web/src/api/types.ts` (the web build must stay green: `npm --prefix web run build`).
- [ ] **Step 4: Run** → PASS (also `cargo test` overall, since `ReviewPhoto` changed shape).
- [ ] **Step 5: Commit** `feat(helper): proposals, your-call pairs, taste profile and decision log`

### Task 4: Helper runner — transport, background queue, prompt

**Files:**
- Create: `server/src/helper/{mod,transport,stream,prompt}.rs`, `server/src/helper/prompt.md`, `server/src/bin/fake-claude.rs`, `server/tests/helper_runner.rs`
- Modify: `server/src/config.rs`, `server/src/lib.rs` (`AppState.helper`), `server/src/enrich/job.rs` (call `helper.enqueue(shoot_id)` after `EnrichDone`), `server/src/api/{mod,helper}.rs`, `server/src/main.rs`, `server/Cargo.toml` (`uuid = { version = "1", features = ["v4"] }`)

**Interfaces:**
- Consumes: Task 0's flag list, model, budget and turns; Task 3's `db::helper`; Task 2's `/mcp`.
- Produces:
  - **Settings** (serde defaults; existing files keep their values): `helper_enabled: bool = true`, `helper_transport: String = "cli"` (`"cli" | "cli-api-key" | "off"`), `helper_model: String` (from Task 0), `helper_max_turns: u32` (Task 0), `helper_max_budget_usd: f64` (Task 0), `helper_frames_per_burst: u32 = 3`. `config::claude_bin() -> Option<PathBuf>` checks `PHOTOSIFT_CLAUDE_BIN` (empty = unset), else searches `PATH` for `claude.exe`, then `claude.cmd`.
  - `helper::transport::Transport` (trait, `Send + Sync`):
    - `fn availability(&self) -> Availability { Ready { auth: String } | Unavailable { reason: String } }`
    - `fn run(&self, req: &RunRequest, on_event: &mut dyn FnMut(RunEvent)) -> anyhow::Result<RunOutcome>`
  - `RunRequest { run_id: i64, system_prompt_path: PathBuf, prompt: String, model: String, max_turns: u32, max_budget_usd: f64, session: SessionMode /* New(Uuid) | Resume(String) */, deny_tools: Vec<&'static str>, cwd: PathBuf }`.
  - `RunEvent` (from `stream.rs`): `Init { tools: Vec<String>, mcp_ok: bool }`, `AssistantText(String)`, `Retry { attempt: u32, error: String }`, `Result(RunResult)`. `RunResult { is_error: bool, subtype: String, result: String, usage: RunUsage }`.
  - `RunOutcome = RunResult`.
  - `stream::parse_line(line: &str) -> Option<RunEvent>` is pure; unknown event types → `None`.
  - **`ClaudeCli` impl:**
    - Availability = `claude auth status` exit 0 and `authMethod != "none"`, cached at startup and refreshed on "Retry".
    - Command line = Task 0's verified list. Pinned shape: `-p <prompt> --output-format stream-json --verbose --strict-mcp-config --mcp-config {"mcpServers":{"photosift":{"type":"http","url":"http://127.0.0.1:<port>/mcp"}}} --tools "" --allowedTools "mcp__photosift__*" --disallowedTools <deny_tools joined> --permission-mode dontAsk --permission-prompts none --setting-sources local --disable-slash-commands --system-prompt-file <path> --model <m> --max-turns <n> --max-budget-usd <x>` plus `--session-id <uuid>` or `--resume <id>`.
    - `cwd` = `<data root>\helper\` (created empty).
    - On Windows, `.cmd` shims run through `cmd /C`, and the child gets `CREATE_NO_WINDOW` (0x08000000) so the GUI-subsystem release never flashes a console.
    - stdout is read line by line into `parse_line`; stderr goes to `plog!("helper", …)` at debug.
    - Kill the child on drop.
    - `cli-api-key` = the same command with `ANTHROPIC_API_KEY` passed through from PhotoSift's own environment. If that variable is unset, the transport is `Unavailable`.
  - `helper::HelperService` (cheap to clone; stored as `AppState.helper`):
    - `start(db: DbPool, events: EventHub, data_root: &Path, port: u16, settings: Arc<Settings>, transport: Arc<dyn Transport>) -> Self` and `enqueue(shoot_id)`. `port` builds the `/mcp` URL. `lib::build_state` picks the transport from `helper_transport`: `"off"` → an `Off` transport that is always `Unavailable { reason: "helper turned off in settings" }`. There is one worker thread (like `EnrichWorker`), and only one run at a time. Kind `background`.
    - `status() -> HelperStatusInfo { available: bool, reason: Option<String>, transport: String, queue: usize }`.
    - On startup it marks runs in `running`/`queued` as `interrupted`.
    - It skips a shoot whose live, unreviewed photos all have proposals from a `done` run newer than their `enriched_at`.
  - **Background deny list:** `mcp__photosift__set_outcome`, `mcp__photosift__set_taste_profile`, `mcp__photosift__start_run`.
  - `helper::prompt::build(pool, shoot_id, run_id, new_photo_ids: &[i64]) -> (String /*system*/, String /*user*/)`.
    - The system prompt = `prompt.md` with `{{TASTE}}` replaced by the taste text (or "No notes yet.") and `{{EXAMPLES}}` replaced by `recent_examples(40, 10)`, rendered one line each: `proposed <o> ("<reason>") → chose <o>[; why: <criterion>[ — <note>]]`.
    - The user prompt names `shoot_id` and `run_id`, and when they are non-empty, the `new_photo_ids`.
    - `PROMPT_VERSION: &str` = the first 12 hex chars of the SHA-256 of `prompt.md`, stored on the run.
  - **`prompt.md` must state** (the executor writes the prose):
    - who the photos are for (family album; emotional impact beats technique)
    - outcome meanings (spec §7 table)
    - the expected album fraction (~1 in 8 for a party; a few for a 10-photo import)
    - the workflow:
      1. `get_shoot_overview`
      2. `get_moment` per moment
      3. `get_faces` when the top frames of a burst are close
      4. `propose` for **every** live unreviewed photo (bursts' lower frames → archive unless distinct)
      5. `propose_your_call` for near-identical frames where it would be a coin-flip, stating the tradeoff
      6. `finish_run` with a summary of 2-4 sentences
    - "expression beats sharpness"; "keep bloopers that carry feeling: library + blooper"; reasons ≤ 140 chars, concrete, and no names of people
  - **HTTP:**
    - `POST /api/shoots/{id}/helper/run` → 202 `{run_id}` (enqueue now; 409 if one is queued or running for that shoot; 503 with the reason if unavailable).
    - `GET /api/status` gains `helper: HelperStatusInfo`.
  - Every state change emits `HelperStatus`.
  - **`fake-claude` bin:**
    - It reads `FAKE_CLAUDE_SCRIPT` (a JSON file path) with `{ "auth": "claude.ai"|"none", "calls": [{tool, args}], "result": {is_error, subtype, result}, "exit": i32, "delay_ms": u64 }`.
    - It parses `--mcp-config`, `--session-id`/`--resume` and the prompt (it finds `run_id <n>` in the prompt and substitutes `$RUN` in `args`), performs each MCP call via HTTP, and prints `system/init`, `assistant` and `result` stream-json lines.
    - It answers `auth status` with `{"authMethod": …}` and the matching exit code.
    - It appends its argv to `FAKE_CLAUDE_ARGS_LOG` when that variable is set.
- [ ] **Step 1: Write failing tests** (`server/tests/helper_runner.rs`, with `PHOTOSIFT_CLAUDE_BIN` pointing at the built `fake-claude`):
  - `parse_line_cases`: unit tests on recorded lines (`system/init`, `assistant` text, `system/api_retry` with `error: "rate_limit"`, a `result` with `usage` and `total_cost_usd`) → the expected `RunEvent`s. Garbage → `None`.
  - `background_run_after_enrichment`: the script proposes album for 2 photos and calls `finish_run` → after import + `EnrichDone`, a `HelperStatus done` arrives. The review shows 2 proposals and the summary. The run row has `cost_usd`, `num_turns` and `prompt_version` set.
  - `argv_is_isolated_and_denies_writes`: in the args log, `--strict-mcp-config`, `--tools ""`, `--permission-mode dontAsk`, `--setting-sources local`, `--max-budget-usd` and `--disallowedTools` (containing `mcp__photosift__set_outcome`) are present, and `--bare` is absent.
  - `cli_missing_reports_unavailable` (Review Focus 1): `PHOTOSIFT_CLAUDE_BIN` → a nonexistent path → `/api/status.helper.available == false` with a reason, the import and enrichment still finish, and the review's `helper.status == "unavailable"`.
  - `logged_out_reports_unavailable`: `auth: "none"` → unavailable, reason contains "claude auth login".
  - `rate_limit_result_fails_run_keeps_proposals` (Review Focus 1): the script proposes 1 photo, then the result has `is_error: true` and an `api_retry`/`rate_limit` line was emitted → status `failed`, the error mentions the usage limit, and the 1 proposal remains.
  - `startup_marks_running_runs_interrupted` (Review Focus 3): insert a `running` run and respawn the server → `interrupted`. The fake's MCP `propose` with that `run_id` → tool error.
  - `second_import_queues_run_for_new_photos` (Review Focus 4): import 3 photos into an Everyday shoot (run 1 done), then import 2 more on another day of the same month → run 2's prompt (args log) lists exactly the 2 new ids, and run 1's proposals for the first 3 still stand.
  - `manual_run_conflicts_while_running`: `POST …/helper/run` twice quickly (the fake has `delay_ms: 2000`) → the second gets 409.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test helper_runner` → FAIL.
- [ ] **Step 3: Implement.** Write the system prompt to `<data root>\helper\runs\<run_id>\system.md`. Delete run directories older than 30 days at startup.
- [ ] **Step 4: Run** → PASS. Then run once against the real `claude` on the golden shoot in the dev data root (`cargo run`, models present). Read the proposals and reasons yourself, record the run's tokens and cost in the commit body, and confirm a `HelperStatus done`.
- [ ] **Step 5: Commit** `feat(helper): claude -p runner with isolated flags, background queue and status`

### Task 5: Review screen — reasons, suggestions, accept, your-call pairs, overrides

**Files:**
- Create: `web/src/review/YourCallPair.tsx`, `web/src/review/OverrideWhy.tsx`, `web/tests/e2e/helper.spec.ts`
- Modify: `web/src/review/{PhotoTile,Summary,MomentSection,keyboard,model,useReview}.ts(x)`, `web/src/pages/ReviewPage.tsx`, `web/src/api/client.ts`, `web/src/review/review.css`, `web/tests/e2e/fixtures.ts` (option `claudeScript`: writes the script, sets `PHOTOSIFT_CLAUDE_BIN` to the built `fake-claude.exe`, sets `FAKE_CLAUDE_SCRIPT`)

**Interfaces:**
- Consumes: `ShootReview.helper`, `ReviewPhoto.proposal`, `ProposalsChanged`, `HelperStatus`, `POST /api/shoots/{id}/helper/run`, `POST /api/photos/{id}/decision-note`.
- Produces:
  - `client.ts`: `runHelper(shootId)`, `setDecisionNote(photoId, criterion, note?)`.
  - `model.ts`: `acceptable(review, scope: {photoId} | {momentIdx} | "all") -> {photoId, outcome, blooper}[]`. It returns unreviewed photos with a proposal, in navigation order.
  - New `BINDINGS`:
    - `Y` "Accept the helper's suggestion"
    - `Shift+Y` (key `"Y"`) "Accept every suggestion in this moment"
    - `H` "Helper chat" (wired in Task 6)
    - `resolveKey` must tell `y` from `Y`: keep the case for exactly the `Y` binding, everything else stays case-insensitive.
- Layout (spec §10):
  - **Summary** shows `helper.summary` above the chips. Next to it is a status line: "Helper is reading the shoot…" (queued/running), "Suggested: 42 album · 60 library · 250 archive · 3 delete?" (from `suggested`), or the error text plus a **Retry helper** button (failed/interrupted/unavailable; disabled with the reason when unavailable).
  - An **Accept all suggestions (N)** button opens a confirm dialog naming the per-outcome counts, then applies them one `setOutcome` at a time with progress, stopping on the first error.
  - **PhotoTile** fills the existing `data-slot="reason"` with `proposal.reason`, on large, highlight and strip tiles (on strip tiles as a `title` tooltip only, to keep strips compact).
  - An unreviewed photo with a proposal shows an outlined badge `suggest: Album` (`data-suggested="album"`). Once decided, the solid badge shows; if it differs from the proposal, the reason is struck through and `OverrideWhy` appears.
  - **OverrideWhy** shows chips "expression · who's in it · moment · sharpness · duplicate · composition · other". A click sends `setDecisionNote`; "other" opens a one-line input (Enter saves). It is never modal and never steals keyboard focus.
  - **YourCallPair** renders inside its moment, above the burst rows. It shows both photos whole at their own aspect, side by side, with the tradeoff line under them and `data-your-call-id`. Arrow keys move between the two. `A` on one sets album on it **and** archive on the other only if the other is still unreviewed (the cheap swap, spec §1 lesson). Accepting the suggestion with `Y` uses the per-photo proposal.
- [ ] **Step 1: Write failing tests.**
  - Vitest `model.test.ts`: `acceptable` skips decided photos and photos without proposals, and orders by navigation order. `resolveKey({key:"Y"})` → accept-moment, `{key:"y"}` → accept-one.
  - Playwright `helper.spec.ts` (synthetic 12-photo shoot, fake-claude script proposing album ×3 / archive ×8 / delete ×1 plus one your-call pair):
    - (a) After enrichment the summary shows the fake's summary text and "Suggested: 3 album"; each large tile shows its reason text.
    - (b) Focus a suggested photo, press `Y` → its badge becomes solid Album, the Album chip count increments, and the sidecar has ★4.
    - (c) Press `L` on a photo suggested as album → OverrideWhy chips appear; click "expression" → the `decision-note` request returns 204 and the chip shows `aria-pressed="true"` after a reload (Task 7's e2e covers the listing).
    - (d) `Shift+Y` accepts the rest of the moment.
    - (e) The your-call pair shows both frames uncropped (rendered aspect within 2% of natural) and the tradeoff text; `A` on the left makes left album and right archive.
    - (f) `helper_failure_is_visible_and_retryable` (Review Focus 1): the script result is `is_error` with a rate-limit retry → the error text is visible, all Plan 1 keys still work, and clicking Retry (after swapping the script to a success one) leads to the "Suggested" line appearing.
    - (g) `deciding_during_run_keeps_user_choice` (Review Focus 2): the script has `delay_ms` 1500 before proposing delete for photo 1; press `A` on photo 1 during the delay → after the run, photo 1 is still Album with the suggestion visible as struck-through text.
- [ ] **Step 2: Run** `npm --prefix web run test` and `npm --prefix web run e2e -- helper.spec.ts` → FAIL.
- [ ] **Step 3: Implement.** Refresh the review on `ProposalsChanged` (debounce 300 ms) and update the status from `HelperStatus`.
- [ ] **Step 4: Run** → PASS. Then, against `cargo run` + `npm --prefix web run dev` with the real helper on the golden shoot, screenshot the review screen yourself. Check that reasons are readable, nothing is cropped, the your-call pair sits inside its moment, and the console is clean.
- [ ] **Step 5: Commit** `feat(web): helper reasons, suggestions, accept keys, your-call pairs and override reasons`

### Task 6: Chat panel

**Files:**
- Create: `web/src/review/HelperPanel.tsx`, `web/tests/e2e/chat.spec.ts`
- Modify: `server/src/api/helper.rs`, `server/src/helper/mod.rs`, `server/src/events.rs`, `server/tests/helper_runner.rs`, `web/src/pages/ReviewPage.tsx`, `web/src/api/{client,types}.ts`

**Interfaces:**
- Produces:
  - `POST /api/shoots/{id}/chat {text}` → 202 (409 while a chat turn for that shoot is running; 503 if unavailable). It stores the user message and runs a `chat`-kind turn on a separate chat thread (it does not queue behind background runs).
    - The first message creates `--session-id <uuid>`, stored on the shoot's latest chat run. Later messages `--resume` it and create a new `chat` run row per turn.
    - The chat deny list is `mcp__photosift__set_outcome`, `mcp__photosift__start_run`. `set_taste_profile` is allowed, because the user asked in their own words.
  - `GET /api/shoots/{id}/chat` → `[{id, role, text, at}]`.
  - `ServerEvent::ChatMessage { shoot_id, role, text }`, one per assistant text block. On a failed turn: `ChatMessage { role: "helper", text: "Couldn't reach Claude: <reason>" }`.
  - The chat system prompt is the same `prompt.md` plus a short section saying: answer the user's request; use `propose`/`propose_your_call` to change suggestions; never claim to have set outcomes.
  - **HelperPanel:** a right-side drawer toggled by `H` and by a "Helper" button in the summary. It holds the message list and a textarea (Enter sends, Shift+Enter adds a newline). While the textarea has focus, review keys are inactive (the existing `isTypingTarget` already covers this). Esc first blurs the textarea, then closes the drawer.
- [ ] **Step 1: Write failing tests.**
  - Rust `chat_resumes_same_session`: two messages → the args log shows `--session-id X` and then `--resume X`; `GET …/chat` returns 4 messages in order.
  - Rust `chat_can_update_taste_background_cannot`: chat args do not deny `set_taste_profile`; background args do.
  - Playwright `chat.spec.ts`: press `H`, type "make the third one album", Enter → the fake's reply appears, and its scripted `propose` changes photo 3's suggestion badge. Pressing `A` while typing inserts "a" into the textarea and does not change any outcome.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS. Do one real exchange against the golden shoot ("which photo has the best smile in moment 2?") and read the answer and console yourself.
- [ ] **Step 5: Commit** `feat(helper): chat panel over the same claude session`

### Task 7: Taste profile page

**Files:**
- Create: `web/src/pages/TastePage.tsx`, `web/tests/e2e/taste.spec.ts`
- Modify: `server/src/api/helper.rs`, `web/src/App.tsx` (route `/taste`, link from the Shoots page header), `web/src/api/{client,types}.ts`

**Interfaces:**
- Produces:
  - `GET /api/taste` → `{text, updated_at}`.
  - `PUT /api/taste {text}` → same shape (400 over 4000 chars).
  - `GET /api/taste/decisions?limit=50` → `[{photo_id, shoot_id, proposed, proposed_reason, chosen, criterion, note, decided_at}]`, newest first.
  - **TastePage:**
    - A textarea that autosaves 800 ms after the last keystroke, with a "Saved" indicator.
    - Below it, "Where you disagreed": recent overrides with thumbnail, "suggested X because '…' → you chose Y", and the criterion.
    - A one-line explainer: "The helper reads these notes and your last 40 disagreements before every run."
- [ ] **Step 1: Write failing tests.** Rust (in `helper_data.rs`): `taste_http_round_trip`, `decisions_listing_newest_first`. Playwright `taste.spec.ts`: type a note, reload → it persists. After an override on the review screen, the taste page lists it with its criterion.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(web): editable taste profile and recent disagreements`

### Task 8: Evaluation harness (read-only, against historical `Export\`)

**Files:**
- Create: `server/src/eval/{mod,exports,score}.rs`, `server/src/bin/eval.rs`, `server/tests/eval.rs`, `tests/eval/README.md`, `tests/eval/shoots.example.txt`
- Modify: `server/src/ingest/scan.rs` (`ScanOptions`), `server/src/ingest/job.rs` (`ImportMode`), `server/src/config.rs` (`read_only`), `server/src/xmp.rs`, `server/src/trash.rs`, `.gitignore` (`tests/eval/shoots.txt`, `.photosift-eval/`)

**Interfaces:**
- Produces:
  - `Settings.read_only: bool` (default false; never written to `settings.toml`, i.e. `#[serde(skip)]`). When it is true, `xmp::apply` returns `XmpError::ReadOnly`, `trash` refuses with an error, and `outcome::set_and_mirror` skips XMP and reports `xmp_error: "read-only"`.
  - `scan::ScanOptions { exclude_dirs: &'static [&'static str] }`; `scan::scan_with_options(source, db, settings, &ScanOptions)`. Names are compared case-insensitively at any depth. `eval::EXCLUDE_DIRS = ["Export"]`. `scan` keeps today's behaviour (no excludes).
  - `ingest::job::ImportMode::{Copy, IndexInPlace}`. `IndexInPlace` records rows that point at the source paths (`raw_path`/`jpeg_path` = originals), renders previews into the scratch cache, writes `files` rows, never creates a folder, and never copies. It is reachable only from the eval binary (not over HTTP).
  - `eval::exports::match_exports(export_dir: &Path, photos: &[PhotoKey]) -> ExportMatch { matched: Vec<(PathBuf, i64)>, unmatched: Vec<String> }`, where `PhotoKey { id: i64, stem: String, capture_time: Option<NaiveDateTime>, camera: Option<String> }`.
    - Rule 1: an export stem equal to the original stem, case-insensitive, ignoring a Lightroom suffix `-\d+` / ` \(\d+\)` / `-Edit`.
    - Rule 2: EXIF `DateTimeOriginal` + camera equal to exactly one photo.
    - Otherwise the export is unmatched.
  - `eval::score::score(keep: &HashSet<i64>, truth: &HashSet<i64>, burst_of: &HashMap<i64, (i64, i64)>) -> Score { exact, near, missed, false_picks, precision_strict, recall_strict, recall_lenient }`. `near` = a one-to-one match between a remaining pick and a remaining truth photo in the same (moment, burst). Lenient recall = (exact + near) / |truth|.
  - `eval::baseline_keep(review: &ShootReview, k: usize) -> HashSet<i64>`: the top frame of the `k` bursts with the highest top `signal_score`.
  - **Binary** `photosift-eval run --shoots <file> [--helper] [--max-shoots N] [--label TEXT]`:
    - `<file>` lists one shoot folder per line (default `tests/eval/shoots.txt`, gitignored).
    - The binary builds the normal `AppState` (`lib::build_state`) with `read_only = true` and serves the router in-process on a free `127.0.0.1` port, so the helper's `/mcp` calls work exactly as in the app.
    - Per shoot, with data root `<eval dir>\root\` and `read_only = true`: index in place (exclude `Export`), enrich, then either the baseline (k = |truth|) or (with `--helper`) one background helper run through the real `HelperService`, waiting for it to finish. Keep = proposals with outcome album or library; album-only is scored separately.
    - It prints a table (shoot hash = first 8 hex chars of the SHA-256 of the folder path, photo count, |truth|, unmatched, exact, near, missed, false, recalls, tokens, cost).
    - It appends a JSON line per shoot to `<eval dir>\results.jsonl` with `{label, prompt_version, model, settings_hash, shoot_hash, …}`.
    - The scratch DB is kept between invocations so re-runs skip indexing.
  - `photosift-eval report [--label A --label B]`: aggregates the means per label from `results.jsonl` side by side.
- [ ] **Step 1: Write failing tests** (`server/tests/eval.rs`, synthetic folders):
  - `export_dir_never_indexed` (Review Focus 5): a folder with 4 JPEGs + `Export\` holding 2 JPEGs (copies of 2 originals) → the index has 4 photos.
  - `export_matching_by_stem_then_exif` (Review Focus 5): `DSCF0001-Edit.jpg` matches by stem; `family-01.jpg` with the same DateTimeOriginal + camera as `DSCF0003` matches by EXIF.
  - `unmatched_exports_reported` (Review Focus 5): an export with no EXIF and an unknown stem → `unmatched == ["family-02.jpg"]`, and it appears in the report row.
  - `index_in_place_writes_nothing_to_library`: snapshot the folder's file list + sizes + mtimes before and after a full `run` (baseline) → identical; no `.xmp` is created; the scratch root has previews.
  - `read_only_refuses_xmp_and_trash`: with `read_only`, setting an outcome → `xmp_error == "read-only"` and no sidecar; trash → error, the file is still present.
  - `score_counts`: truth {1,2,3}, keep {1,4,9}, where 4 shares a burst with 2 and 9 is unrelated → exact 1, near 1, missed 1, false 1, recall_lenient 2/3.
  - `raw_subfolder_found`: originals under `RAW\` + exports at top-level `Export\` → matched.
- [ ] **Step 2: Run** `cargo test --manifest-path server/Cargo.toml --test eval` → FAIL.
- [ ] **Step 3: Implement.** Document in `tests/eval/README.md`: making `shoots.txt` (only shoots that have an `Export\`), `PHOTOSIFT_EVAL_DIR`, models, and the cost warning (each `--helper` shoot is one helper run; start with `--max-shoots 3`).
- [ ] **Step 4: Run** → PASS. Then run the baseline locally on 3 real shoots and read the table yourself.
- [ ] **Step 5: Commit** `feat(eval): read-only harness scoring helper picks against past exports`

### Task 9: Burst/moment calibration (Ruling R16)

**Files:**
- Create: `server/src/eval/sweep.rs`
- Modify: `server/src/bin/eval.rs` (subcommand), `server/tests/eval.rs`, `server/src/config.rs` (defaults, only if Step 5 decides so)

**Interfaces:**
- Consumes: the indexed eval roots and truth sets from Task 8; `moments::split(photos, &SplitParams)`.
- Produces:
  - `eval::sweep::evaluate(inputs: &[ShootInputs], params: &SplitParams) -> SweepRow { bursts_per_100: f64, truth_collisions: f64, near_dupe_coverage: f64 }`.
    - `truth_collisions` = the share of truth photos sharing a burst with another truth photo (bursts too coarse: the user kept two frames of what we called one burst).
    - `near_dupe_coverage` = the share of truth photos whose burst has ≥ 2 frames (the burst grouping is doing work).
    - `ShootInputs { photos: Vec<MomentInput>, truth: HashSet<usize> }`.
  - `photosift-eval sweep` evaluates the grid `burst_phash_max ∈ {8,10,12,14,16,18,20,24}` × `burst_window_seconds ∈ {5,10,20}` × `moment_gap_seconds ∈ {60,90,180}` over every indexed shoot. It prints the table and recommends the setting with the fewest bursts per 100 photos subject to `truth_collisions ≤` the current default's value + 0.02. It never edits settings.
- [ ] **Step 1: Write failing tests:**
  - `sweep_collision_counts`: 4 frames 1 s apart, pHash distances 0/6/16 from the first, truth = frames 0 and 3 → at `phash_max` 12 the collisions are 0, at 20 they are 1.0 (both truth photos in one burst).
  - `sweep_is_pure`: same inputs → same rows (no DB access inside `evaluate`).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** → PASS. Then run the sweep over every shoot listed in `shoots.txt`, and record the recommended setting and the current default's row in the task notes (not git).
- [ ] **Step 5: Decide with the user.** Show both rows. On their yes, change the debug and release defaults in `Settings::defaults_for` with a failing-then-passing assertion in `server/tests/health.rs` `settings_defaults_written`, and tell them their existing `settings.toml` keeps old values until edited. Update Plan 1's golden test expectation for DSCF1765/1766 only if the new threshold groups them. Commit `feat(eval): burst threshold sweep` (+ `fix(enrich): calibrated burst defaults` if changed).

### Task 10: Baseline, quality bar, docs

**Files:**
- Modify: `CLAUDE.md` (Plan 2 status, new routes/tools table, helper settings, `PHOTOSIFT_CLAUDE_BIN`, `PHOTOSIFT_EVAL_DIR`, eval commands, fake-claude in e2e, "helper proposes, never decides"), `README.md` (helper section), `docs/superpowers/specs/2026-10-19-photosift-overhaul-design.md` (§9: the agreed target numbers; §12: the subscription-route row updated with Task 0's findings), `.github/workflows/ci.yml` (build `fake-claude` before e2e)

- [ ] **Step 1:** Run `photosift-eval run --label baseline-signals` over all of `shoots.txt`, then `--helper --label helper-v1` over the same shoots, as far as Task 0's budget allows. If the budget can't cover all of them, use the 10 most recent shoots; say so in the notes.
- [ ] **Step 2:** Show the user the `report` comparison and propose a bar, e.g. "lenient recall ≥ 0.7 with ≤ 2× |truth| keeps". This is the spec's "good enough that the user trusts the draft" (§9); the user picks the numbers. Record the agreed bar (numbers only) in spec §9.
- [ ] **Step 3:** If helper-v1 misses the bar, iterate on `prompt.md` and `helper_frames_per_burst` only (each iteration re-scored with a new `--label`; prompt_version changes automatically) until it passes or the user stops. Record the iteration count and the final numbers in the commit body.
- [ ] **Step 4:** Update the docs listed above. Run the full verification loop: `cargo test`, clippy, fmt, `npm --prefix web run test`, `npm --prefix web run build`, `npm --prefix web run e2e` → all pass.
- [ ] **Step 5: Commit** `docs: Plan 2 helper, MCP and evaluation harness` (+ `feat(helper): tune prompt to quality bar` commits from Step 3).
