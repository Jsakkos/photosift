# PhotoSift Overhaul — Design

*2026-10-19 · status: draft for review · replaces the Tauri prototype's premise (three-pass cull → Capture One/DxO)*

Statements marked **(user)** were decided by the user in the design interview. Statements marked **(call)** are the designer's calls, open to correction. Everything else is context or evidence.

---

## 1. Intent

PhotoSift exists so that a busy parent with an infant can go from a full SD card to a shared family album **in one short sitting instead of days or weeks**. **(user)**

The photos are family life and events with friends and family, not artistic work. The camera is often lent to relatives, so photos are rarely technically perfect; what matters is **emotional impact** — expression, who is in it, the moment. A blurry frame of the baby mid-sneeze can be worth more than a sharp, empty one. **(user)**

The app does the heavy lifting: ingest, enrichment with local computer-vision models, and a Claude-powered helper that proposes the album, explains its choices, and learns the user's taste from their corrections. The user reviews and decides; nothing is hidden or deleted behind their back. **(user)**

### Who and how often

- One user, on their own Windows desktop. Scoped to them "at present". **(user)**
- Imports vary from ~10 photos (a handful of the baby on a Tuesday) to ~400 (a party), occasionally more. Cards often hold a mix of days/moments since the last import; occasionally exactly one event. **(user)**
- Cameras: Fujifilm X100VI (primary; shoots RAF+JPG pairs) and Nikon D750 (NEF). Two cameras on one day is possible but not common; it must work when it happens. **(user)** On the 2026-10-18 party the D750 shot at 12:47 and 16:07 and the Fuji 15:48–17:03; the 16:07 D750 burst interleaves with the Fuji's, so both clocks look right (an earlier "3 h off" reading was wrong).

### Prior evidence that shaped this design

- **Baby 5-months experiment (Sept 2026)** — an ad-hoc Claude session picked a Top 10 from 103 frames using 16-up contact sheets (a past shoot's `Top10\` folder). Against the user's 9 actual exports: 2 exact matches, 4 cases where they chose an adjacent frame (1491 vs 1492: "nearly identical, I just liked it more"; 1573 vs 1572: "better smile, felt more natural" — and a *different pose*, so a different moment), 3 picks missed. **Lessons:** Claude identifies good moments from contact sheets; frame choice within a burst needs face-level detail and local signals; near-identical frames are a coin-flip the UI should make cheap to swap; a pose change makes a new moment even seconds apart. **(user, via the discussion)**
- **Stack spike (2026-10-19)** — `spikes/stack-bench/RESULTS.md`. Rust/axum vs Python/FastAPI on the 405-photo party shoot. User-perceived speed identical (→ key to painted preview ≈13 ms prefetched, ≈25 ms cold — dominated by browser JPEG decode). Rust ~2× faster at enrichment (2.1 s CUDA for 405 photos / 668 faces), ~⅓ the memory, instant startup; ingest disk-bound in both (~64 s for 22 GB off SATA SSD). Python half the code but fiddlier CUDA setup and a silent data bug.

---

## 2. Goals, non-goals, anti-goals

### Goals

1. **Card → album in one sitting.** For a 400-photo event: import + enrichment finish unattended; the helper's draft album is ready when the user sits down; they confirm/swaps for a few minutes and sends it.
2. **Never miss the sentimental shots.** Bloopers and technically flawed but meaningful photos are surfaced as their own category, not buried with junk.
3. **Show the work.** Every suggestion carries a short reason. The review screen shows how the helper got from 400 to 50, not just the result. **(user: "A and B show the work to get there, which is essential")**
4. **Learn their taste.** Corrections feed back so the helper gets better over time, and its idea of their taste is readable and editable.
5. **Consistent, standardized filing.** Every import lands in one consistent folder scheme.
6. **Seamless Lightroom Classic handoff.** The whole shoot goes to Lightroom with PhotoSift's decisions already attached. **(user)**
7. **Albums in Immich without manual steps,** including ongoing albums that grow across many shoots (e.g. the baby's album). **(user)**
8. **Use the user's Claude subscription, not metered API billing,** for the curation. **(user)**

### Non-goals (this version)

- Other users, other OSes, other editors (Capture One/DxO support is dropped), other photo servers. **(user: scoped to them)**
- A desktop wrapper. A browser tab on `localhost` is fine "for now". **(user)**
- Phone photos (Immich already handles the phone). **(call)**
- AI scoring of video. MOV files are copied/archived with their event and shown in the timeline, but not scored. **(call)**
- Re-organizing or re-culling the existing 2009–2026 library. Existing folders stay where they are; they are read only for the evaluation harness (§9). **(call)**
- Managing Immich sharing permissions. The user creates/shares albums in Immich; PhotoSift only adds photos to albums (and may create an album they have named, which they then shares himself). **(call)**
- Google Photos integration. **(call — Google restricted third-party library access; revisit if wanted)**
- A Lightroom plugin. **(call — hold unless the manual "Read Metadata from Files" step proves annoying)**

### Anti-goals — failure even if it technically works **(user)**

1. **It becomes a maintenance hobby.** If the user spends more time debugging/programming PhotoSift than using it, it has failed.
2. **It doesn't work very well** — chiefly, the helper's picks aren't good enough to trust, so they redo the work by hand.

Consequences for the build **(call)**:
- Small surface: one Rust process, one browser UI, one SQLite file. Every feature must earn its maintenance cost.
- It cannot lose photos: copies are hash-verified before an import reports success; re-running anything is idempotent; nothing is deleted without explicit approval.
- Claude can verify its own work (§10): real-browser e2e tests and a golden-shoot fixture from real photos.
- The helper is **measured** against the user's historical choices before it is trusted (§9).

**Definition of usable:** both a fast import + smooth review *and* good helper picks. The user explicitly wants both before calling it usable — "we should work until that's the case." **(user)** Internal milestones are fine; "done" is both.

---

## 3. Constraints

- **Machine:** Windows 11, Ryzen 7 9700X (16 threads), 32 GB RAM, RTX 3090 (CUDA). Photo library on `E:` (SATA SSD). CUDA/cuDNN runtime DLLs are not shipped by ONNX Runtime; current dev setup copies them from pip `nvidia-*-cu12` wheels (see user memory `reference_cuda_dll_setup.md`). ONNX Runtime 1.24 / `ort =2.0.0-rc.12` is what the prototype and spike use.
- **Library root:** `E:\photos\DSLR\` today. Existing structure: `{year}\{YYYY-MM}_{Event-Name}\` (~150 folders since 2009), older shoots with a `RAW\` subfolder, most 2026 shoots flat; `Export\` subfolder per shoot is what Immich ingests. Some 2026 folders contain leftovers from the prototype (`selects\`, `rejects\`, `edit\`, `.photosift\`).
- **Existing automations to keep working:** a scheduled task copies the library to the NAS for backup, and another imports from `Export\` folders into Immich. **(user)** Task Scheduler shows `\Photo backup` and `\Photo Library Sync to NAS`; the builder should read their actions (source paths, how Immich import is invoked) before relying on them. PhotoSift must not break either; it should *use* the `Export\` → Immich path rather than replace it.
- **Lightroom writes XMP automatically.** Sidecars in 2026 shoots contain Lightroom develop data (`crs:ProcessVersion` etc., e.g. 70 sidecars in a 2026 baby shoot), so "Automatically write changes into XMP" is effectively on. PhotoSift and Lightroom therefore share sidecar files: PhotoSift must read-merge-write only the fields it owns and never clobber Lightroom's.
- **Lightroom Classic** is the catalog of record for the whole shoot. **(user)** It tracks files by path: anything that renames/moves files outside Lightroom breaks the catalog. Lightroom reads XMP on import (or on manual "Metadata → Read Metadata from Files").
- **Claude:** the user's own subscription via their own `claude` CLI login on this machine. Not the metered API. **(user)**
- **Immich:** self-hosted; API key required for album automation. Server version not yet confirmed — Immich v3 changed the API (see §12).

---

## 4. Architecture (decided)

**Rust local server + React UI in a normal browser tab.** **(user, after the spike)**

```
                 ┌──────────────────────────── photosift.exe (Rust, one process) ─────────────────────────────┐
 SD card ──────► │ drive watcher → ingest (copy+verify, EXIF, previews, pHash) → enrichment worker (ONNX, CUDA)│
                 │                                   │                                                        │
                 │                              SQLite (one file)                                             │
                 │        ┌──────────────────────────┼───────────────────────────────┐                       │
                 │   HTTP/JSON API + WebSocket   MCP endpoint                 helper runner                   │
                 └────────┼──────────────────────────┼──────────────────────────────┼─────────────────────────┘
                          │                          │                              │ spawns `claude -p`
                  browser tab (React)        Claude Desktop/Code (optional)   (background + chat), which
                                                                              calls back into the MCP endpoint
       XMP sidecars ──► Lightroom Classic          Export\ JPEGs ──► existing task ──► Immich ◄── Immich API (albums)
```

- **One Rust process** started at Windows login (background, no window) so it can notice SD cards and run enrichment unattended. **(call)** Serves the UI on `127.0.0.1` only.
- **Browser UI** (React + TypeScript). Live updates (import progress, enrichment progress, "new card detected") over a WebSocket. **(call)** The prior React codebase is not carried over; the UI is rebuilt for the new workflow. **(call)**
- **One operation surface, three clients.** The HTTP API and the MCP endpoint expose the same operations (list moments/photos, get preview/face crop, read signals, set outcome, add to album, read/update taste profile). The UI, the Claude helper, and the tests all go through it, so they can never disagree about what is possible. **(call)**
- **Reuse from the prototype** (`src-tauri/src/`): ingest walker/copy/hash, embedded-preview extraction, EXIF, pHash/clustering, XMP read/write (minus the bucket logic), YuNet face detection, sharpness, keyring not needed. **Drop:** Tauri shell + command layer, `layout.rs` bucket mover, `folder_template.rs` buckets, three-pass model and its settings, the API-key curator providers (Anthropic/Gemini/local), `xmp:Label`-never-write rule (reversed — see §7). **(call)**
- **Python** only for offline experiments (the existing `benchmark/` folder pattern), never at runtime. **(call)**
- **Repo:** same repository, new direction; git history keeps the prototype. **(call)**

---

## 5. Ingest

### Flow
1. **Card detected** → the open tab shows a banner: "SD card: 412 photos, 3 days. Import?" (if no tab is open, the import waits). **(call — auto-import without confirmation is a later option)**
2. **Split into events.** Group the card's photos by capture date and time gaps; propose events (e.g. "Sat Oct 18, 12:47–5:10 pm — 405 photos; Tue Oct 14 — 12; Wed Oct 15 — 6"). The user can merge/split before confirming. **(user agreed to the idea)**
3. **Name.** Larger events get a name; the helper proposes one from content ("Birthday party at the park") and the user accepts/edits in one keystroke. Small moments default into the month's rolling **everyday** folder instead of needing a name. **(user: already does this as `Misc-<baby>` folders; the rolling folder matches their habit)** Threshold for "small" left to builder, tunable.
4. **Multi-camera clock offset.** When an import contains more than one camera body, the import screen shows each camera's time range and lets the user set a per-camera offset (minutes/hours) before confirming; corrected capture times are used everywhere. Original EXIF is never rewritten. No automatic offset detection in this version — the party shoot showed both clocks were fine, so automation isn't justified yet. **(call)**
5. **Copy + verify.** Copy to the destination, SHA-256 verify each copy, skip files already imported anywhere (global dedupe by content hash — the prototype already does this). Report success only after verification. Never touch the card's contents. **(call)**
6. **Previews.** Generate a screen-sized preview (spike used 2560 px long edge, JPEG q85) and a grid thumbnail (spike: 400 px) per photo, from the embedded JPEG (RAF embeds 4416×2944; D750 NEF embeds 6016×4016). Apply EXIF orientation (the spike did not; portraits rendered sideways). **(call)**
7. **Enrichment** runs automatically after import (§6), then the helper runs (§8).

### Folder scheme (going forward only) **(call, user invited improvements)**
```
E:\photos\DSLR\
  2026\
    2026-10-18_Birthday-Party\      ← named event: day-level date
      DSCF1606.RAF  DSCF1606.JPG  DSC_4742.NEF  DSCF1700.MOV …   (flat, all cameras)
      Export\                                  ← JPEGs for Immich (album + library outcomes)
    2026-10_Everyday\                          ← rolling month folder for small moments
      Export\
```
- `_` only between date and name; `-` inside the name; ASCII only; no spaces/apostrophes ("Baby's First Meal" → `Babys-First-Meal`).
- The root stays `E:\photos\DSLR\` for new shoots, so the NAS-backup and Immich tasks keep their source paths. (The name is slightly wrong for the X100VI; renaming is a later, separate chore.) **(call)**
- **Existing folders are never renamed or moved.** Tidying them, if ever, happens inside Lightroom's Folders panel so the catalog follows.
- **No file moves after ingest, ever.** Decisions live in the database and XMP, not in folder location. (This deliberately reverses the prototype's bucket folders.)

---

## 6. Enrichment (local, on-device)

Runs on the GPU after import; suggest-only. Spike timing for 405 photos: ~2 s on CUDA (detect + embed + sharpness + exposure).

Signals per photo / per face **(call, guided by the user's priorities: expression and people over technique)**:
- **Faces:** YuNet detection + 5 landmarks. **Known recall gap:** the prototype letterboxes the whole frame to 640×640, which loses small faces in group shots; the builder should test tiling or a larger input on real party photos.
- **Face identity embeddings:** SFace (OpenCV zoo, Apache-2.0, 128-d) — validated in the spike (same people across frames match at cosine ≈0.93–0.95). Used for "same person" grouping and People (§11).
- **Expression:** eyes open/closed, smile/laugh. The prototype's eye/mouth classifiers were drop-in ONNX files that aren't shipped; the builder must source or train working ones, or rely on Claude's read of face crops. Expression ranks above sharpness. **(user: "better smile, felt more natural" decided their pick)**
- **Technical:** sharpness (global + on faces), exposure (mean luminance, clipped highlights/shadows), motion blur. Used as **tie-breakers within a burst and to catch true junk**, never as the main score. **(user)**
- **Grouping:** near-duplicate bursts (pHash within a burst) inside **moments** (time gaps + visual change; a pose change starts a new moment even seconds apart). **(user)** Structure: card → events → moments → near-duplicate groups.

---

## 7. Outcomes, metadata, and the Lightroom handoff

### One outcome per photo **(user agreed)**
| Outcome | Meaning | On disk + Lightroom + NAS | Immich library (The user + partner) | Shared album |
|---|---|---|---|---|
| **Album** | Keepers (~50 of 400 for a party) | ✓ | ✓ | ✓ |
| **Library** | Sentimental, bloopers, nice-but-not-album | ✓ | ✓ | — |
| **Archive** | Everything else worth keeping | ✓ | — | — |
| **Delete?** | Truly unusable; deleted only after explicit approval | ✓ until approved | — | — |

- "The keepers would all go in the album. The bloopers would end up in my immich feed, but maybe not in an album." **(user)**
- Plus **tags** independent of outcome: `blooper` (so a "Bloopers 2026" album is one click later), `People/<name>` (§11), and album membership (§8).
- Replaces the prototype's flag/destination/star triple and the Triage→Select→Route passes with one decision per photo. **(call)**

### XMP sidecars → Lightroom Classic
- Cull in PhotoSift first, then import the folder into Lightroom, so every photo arrives with decisions attached. If Lightroom already has the folder, a change in PhotoSift needs **Metadata → Read Metadata from Files** in Lightroom; the UI should say so when relevant. **(call, user wants it "as seamless as possible")**
- Consistent mapping (sketch, **call** — exact fields to be verified against Lightroom Classic by the builder):
  - Album ★★★★ · Library ★★★ · Archive ★★ · Delete? ★ + red label (★★★★★ left for the user's own favourites).
  - Lightroom Classic ≥13.2 reads/writes Pick/Reject via `xmpDM:pick` (−1/0/1) per community reports — consider mapping Delete? to Reject and Album to Pick if verified.
  - Keywords: `PhotoSift|Album|<album name>`, `PhotoSift|Blooper`, `People|<name>` (hierarchical keywords so Lightroom smart collections can target them).
  - **`xmp:Label` is now allowed** (reverses the prototype rule, which existed for Capture One/DxO). Existing XMP fields written by the camera or Lightroom (e.g. Fuji's `crs:` defaults, `aux:` fields) are preserved.
- Never write develop settings.
- For Fuji RAF+JPG pairs the sidecar is `<name>.xmp` beside the RAF (as Lightroom already does, e.g. `DSCF1486.xmp`); builder to confirm how Lightroom treats the paired JPG.

### Deleting **(call)**
- Delete? photos are only removed after the user approves them in a batch, and removal means **Windows Recycle Bin** (recoverable), never permanent deletion.
- Recommended timing is before the Lightroom import (the normal order). If Lightroom already has the folder, PhotoSift marks them (Reject / red label) and tells the user to use Lightroom's "Delete Rejected Photos", so the catalog doesn't end up with missing files.

---

## 8. Albums and the Claude helper

### Albums **(user)**
- Two kinds: **event albums** (one shoot, finished once curated — e.g. a birthday party) and **ongoing albums** that grow across many shoots (the baby album; possibly more baby albums later).
- Audiences: The user + partner see everything in Immich; friends/family see shared event albums and the baby album(s).
- A photo can be in zero, one, or several albums. For small imports the helper's main question is "does this belong in an album, and which?" ("these 3 look like they belong in the baby album").
- Album = Immich album + `PhotoSift|Album|<name>` keyword in XMP.

### Best-version-available output **(call; user: Fuji JPEGs are "frequently good enough, but it's hard to say")**
- For each Album/Library photo, the version sent to Immich is: **Lightroom edit** (if a matching file appears in `Export\`) → **camera JPEG** (Fuji pair) → **NEF embedded preview** (D750, no edit).
- So an album can go out the same evening without opening Lightroom. If the user later edits some in Lightroom and exports to `Export\` as they do today, PhotoSift matches the export back to its original (filename + capture time) and swaps it into the album. Lightroom becomes optional per photo.
- When an edited version replaces a camera JPEG, the old Immich asset must be removed/replaced so the library doesn't hold both. PhotoSift-written JPEGs must not collide with Lightroom's export filenames in `Export\` (naming left to builder).
- Delivery: PhotoSift writes JPEGs into the shoot's `Export\` folder (the existing scheduled task imports them into Immich), then uses the Immich API to place the resulting assets into albums. Whether to upload directly via the API instead of relying on the scheduled task is left to the builder; the existing task must keep working either way.

### The helper (Claude, via the user's subscription)
**Behaviour (user):** works in the background after each import, and is available for direct conversation. It highlights the best photos accounting for visually similar ones, flags photos it isn't sure about ("your call"), helps curate the final album, and learns from the decisions made together.

**Mechanism (call):**
- PhotoSift spawns the official `claude` CLI headless (`claude -p`, the user's own login) with PhotoSift's MCP endpoint attached. Claude calls PhotoSift tools to read moments/signals, fetch downsized previews and **face crops**, and write proposals (outcome + album + one-line reason per photo + "your call" items).
- A chat panel in the UI talks to the same CLI session for direct requests ("show me the best of the grandparents with the baby", "swap in something funnier here"). Because the tools are a standard MCP server, Claude Desktop/Code can also connect to them.
- **Division of labour** (from the 5-months experiment): local signals pre-rank frames within each burst; Claude chooses **moments** from contact-sheet-style views and judges **frames** from face crops when signals are close; truly near-identical frames become "your call" pairs with the tradeoff stated ("left has the bigger laugh, right is sharper").
- **Budget:** subscription usage limits apply. Claude sees only downsized images and only shortlists (one or a few candidates per burst), never all 400 at full size. Per-shoot usage must be measured early (§12).
- **Learning:** Claude has no memory between runs, so PhotoSift owns it: (1) a **taste profile** — plain-language notes ("prefers candid over posed", "always keep grandparents with the baby", "expression beats sharpness") that the user can read and edit; (2) **labelled examples** of past accept/override decisions with the reason the helper gave, so an override records *which criterion* they disagreed with; (3) optionally a small local ranking model trained on their decisions (face/scene embeddings → keep probability) to pre-sort before Claude sees anything. All three feed every run.
- **Fallback:** the helper sits behind one interface so the transport can change (e.g. to an API key, or to "The user drives Claude Desktop against the MCP tools") if the subscription route turns out to be limited or disallowed. **(call)**

---

## 9. Measuring the helper — evaluation harness **(call; directly serves anti-goal 2)**

The user's existing library is an answer key: 34 past shoots have an `Export\` folder that records what they actually chose from the RAWs/camera JPEGs (e.g. the baby's 5-month shoot: 9 exports from 103 frames).

- A dev-only harness runs the full pipeline (enrichment + helper) **read-only** against selected past shoots and scores the helper's Album picks against the actual exports: exact-frame matches, same-moment matches (adjacent frame in the same burst counts as a near-hit), misses, and false picks.
- Every change to prompts, signals, or ranking is re-scored before it ships. Results are tracked over time.
- The harness never writes into the library (no XMP, no exports); it reads originals and writes to its own scratch space.
- Target numbers are left to the builder to propose after a first baseline; the bar is "good enough that the user trusts the draft".
- **Bar (agreed 2026-10-22):** with no taste notes or past decisions (a cold start), the helper's album + library picks catch at least 75% of the user's exports, counting an adjacent frame of the same burst, while keeping at most 2.5 times as many photos as the user exported. Model: sonnet, with the Plan 2 prompt.
- Measured on 8 past shoots (about 690 photos, 122 exports), three runs each: the original prompt caught 78-80% (mean 79%) and kept 2.3-2.6x; signals only caught 33%. Opus was no better and cost about 1.8x. A tighter prompt variant kept 2.3x but caught only 72%, so the original stays. Over-keeping is left to the learning loop (taste profile and recorded overrides), which a cold-start eval cannot measure.

---

## 10. Review screen

Direction agreed from mockups (`.superpowers/brainstorm/…/highlights-combined-v3.html`): "ok start" — treat as starting direction, tune against real helper output. **(user)**

Top to bottom:
1. **Helper summary** — what it did, counts per outcome, anything unusual (e.g. a camera-clock offset that was applied).
2. **Outcome counts as filters** — Album · Library · Your call · Archive · Delete?
3. **Highlights row** — the strongest shots, large, so the first seconds feel rewarding.
4. **Moments**, in time order — each moment shows its picks (with a one-line reason under each), "your call" pairs (with the tradeoff), bloopers (with why they're worth keeping), and a dimmed strip of the rest, one key away.
5. **"Review album →"** — the album as family will see it, as the last check before sending to Immich.

Requirements:
- **Every photo shown whole** (no cropping in grids or strips). **(user: crops in the first mockup were unacceptable)**
- **Keyboard-first** (moment/photo navigation, set outcome, compare frames side-by-side, swap pick), mouse works too. **(call, consistent with the prototype's ethos)**
- **Fast**: next/previous preview painted within one frame when prefetched (spike: ~13 ms p50); thumbnail grid of a 400-photo shoot loads in <1 s (spike: 0.41 s). **(call)**
- **Every action persists immediately**; closing mid-review is a non-event; resuming days later picks up where they left off. **(call)**
- **Small imports** (≈10 photos) use the same screen but feel light — likely one moment, a couple of album suggestions, no ceremony. **(call)**

---

## 11. People **(user: "I didn't imagine it, but I don't see it being a bad thing. I'm open to that.")**

Supporting feature, not the centerpiece.
- **Opt-in and local.** Face identity data stays in the local database; Claude only ever sees names.
- **Asks rather than requires setup**: after some imports, "this face appears in 38 photos — who is it?"; unnamed clusters still work, just unlabelled.
- **Uses:** album suggestions ("6 new photos of the baby → baby album?"), the helper's judgement ("keep grandparents with the baby"), and `People|<name>` keywords for Lightroom.
- **Infants change fast**: weight recent confirmed examples more heavily and keep re-anchoring on confirmations, rather than relying on one naming session. **(call)**

---

## 12. Risks and things to verify early

| Risk | Why it matters | What to do |
|---|---|---|
| **Subscription route for the helper** | **Measured (Plan 2 Task 0).** `claude -p` signed in with a plan draws on the plan's usage limits, not the separate monthly API credit, which applies only when an API key is used. Anthropic's terms bar automated access unless explicitly permitted; the docs are not an explicit green light, the user was informed and chose to proceed with the official unmodified CLI under their own login (never store or proxy tokens, never `--bare`, no sharing of plan access). Measured cost: one 372-photo shoot with sonnet and no crops was $0.57 (client-side estimate, 168 s, 12 album picks); about $1.2 estimated with crops. Isolation flags (empty cwd outside any repo, `--strict-mcp-config`, `--setting-sources local`, `--disable-slash-commands`, `--permission-mode dontAsk`, `--system-prompt-file`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`) cut the fixed per-run context from about 55k to about 1.6k tokens. On Pro the 5-hour session limit stopped a run, and dev sessions share that allowance; budget cap `helper_max_budget_usd` is the effective guard. | Transport stays swappable (`cli`, `cli-api-key`, `off`, §8); re-measure after prompt changes; the user reads the usage meter for the measured run. |
| **Immich API changes (v3)** | Reports say the old replace-asset endpoint was removed (copy-asset suggested instead) and album responses no longer list assets (use metadata search). Affects "swap in the Lightroom edit". | Confirm the user's Immich version and build against its OpenAPI spec. |
| **Lightroom XMP field handling** | Which fields Lightroom reads on import (rating, label, hierarchical keywords, `xmpDM:pick`) and precedence when the catalog already has the photo. | Verify with a real import into a test catalog before fixing the mapping. |
| **Face recall in group shots** | Prototype YuNet setup misses small faces (user memory `project_face_detection_recall_gap.md`). Party photos are full of them. | Test tiling/larger input against the party shoot. |
| **Expression models** | No working eye/smile classifier is shipped today. | Source/train, or lean on Claude reading face crops; measure with the harness. |
| **Maintenance burden** (anti-goal 1) | Every integration is a thing that breaks. | Keep the surface small; e2e + golden fixture; integrations degrade gracefully (e.g. Immich down → exports still written, album sync retried). |

---

## 13. Development workflow (call)

- **Claude must be able to see and verify the running app.** Because the UI is a normal browser page, use Playwright (or the in-app browser) against the real server for e2e and visual checks, with console/network/DOM access — this replaces the fragile Tauri MCP bridge that made UI bugs hard to find.
- **Golden-shoot fixture:** a small, committed-or-locally-cached sample of real RAF+JPG and NEF files (including portraits, two cameras in one event, near-identical bursts, a blooper) that every test run imports end-to-end.
- Rust unit tests for ingest/XMP/grouping; the evaluation harness (§9) for helper quality.
- Dev/prod isolation as in the prototype (separate data root for debug builds; `PHOTOSIFT_HOME` override).

---

## 14. Left to the builder

- Exact event-split and moment-split thresholds (make them tunable; start from time gaps + visual change).
- Preview/thumbnail sizes and prefetch depth (spike defaults: 2560/400 px, prefetch 3).
- Database schema (the prototype's is not binding).
- Frontend libraries and component structure.
- How the background server starts with Windows (Task Scheduler, startup shortcut, service).
- Whether Album/Library JPEGs reach Immich via the existing `Export\` task or a direct API upload (the task must keep working).
- Prompt design, batching and contact-sheet format for the helper, within the budget principle in §8.
- Phasing/milestones, provided "usable" means both the fast review and good helper picks (§2).

## 15. Open questions for the user

None blocking. Calls marked **(call)** above are open to correction.
