# Handoff: Photosift

A Windows 11 desktop app for photographers to quickly cull thousands of photos down to a handful of picks, then route them to a RAW processor.

Core loop: **Library → Triage → Select → Route**, with **Compare** and **Grid mode** as cross-cutting tools.

---

## About the design files

The files in this bundle are **design references created in HTML + JSX + inline styles**. They are prototypes that show the intended look, layout, and behavior — they are **not production code to copy directly**.

Your task is to **recreate these designs in the target codebase's existing environment** (React + Tailwind, native Win32/WinUI, Electron, Tauri, SwiftUI, etc.), using that codebase's established patterns, component library, and design tokens.

If there is no existing environment yet, the natural choice is **Electron or Tauri + React + TypeScript + CSS modules or Tailwind**, since this is a desktop photo app that needs filesystem access and native menus. Windows 11 Fluent styling should be applied to match the mockups.

The HTML prototypes render inside a pan/zoom **design canvas** — an organizational wrapper, not part of the product. Ignore `design-canvas.jsx` when implementing. Each `<DCArtboard>` in `Photosift.html` represents one screen state.

---

## Fidelity

**High-fidelity.** All colors, spacing, typography, keyboard shortcuts, and layouts are specified. Recreate pixel-perfectly using the target codebase's libraries. Photos in mockups are striped-gradient placeholders (see `placeholders.jsx > Photo`); in production, render real RAW thumbnails.

---

## Design tokens

All values are defined in `theme.jsx`. Port these to your token system (Tailwind config, CSS custom properties, SwiftUI Color extension, etc.).

### Colors

| Token          | Hex / value                     | Usage |
|----------------|---------------------------------|-------|
| `bg`           | `#151515`                       | app background |
| `bg2`          | `#1c1c1c`                       | titlebar, bottom bars, bg2 panels |
| `bg3`          | `#232323`                       | elevated elements |
| `fg`           | `#e8e6e2`                       | primary text |
| `fgDim`        | `rgba(232,230,226,0.55)`        | secondary text |
| `fgMute`       | `rgba(232,230,226,0.32)`        | tertiary / captions |
| `accent`       | `#d4a574`                       | warm tan — stars, primary accent, "current pass" indicators |
| `accent2`      | `#7fb8d9`                       | cool blue — AI scores, Tab/compare hints |
| `accentBlue`   | `#4a82d9`                       | Windows-style selection blue (active cell outline, buttons) |
| `success`      | `#6fbb7b`                       | keep verdict, kept-count |
| `danger`       | `#d97a7a`                       | toss verdict, tossed-count |
| `warning`      | `#e8c64a`                       | "blink" face verdict |
| `borderColor`  | `rgba(232,230,226,0.07)`        | hairline dividers |
| `hover`        | `rgba(232,230,226,0.04)`        | hover surfaces |
| `selected`     | `rgba(74,130,217,0.18)`         | selection tint |

Color-label chips (Lightroom-style): `#d94a3d` red · `#e8c64a` yellow · `#4aa96c` green · `#4a82d9` blue · `#9c6bd9` purple.

### Typography

- Primary UI: **Segoe UI Variable**, fallback `Segoe UI, system-ui, sans-serif`
- Monospace (filenames, EXIF, counts, shortcuts): **JetBrains Mono**, weights 400/500
- Sizes: 9 (labels/captions — often uppercase with 1.2 letter-spacing), 10, 11 (body), 12 (titlebar/tabs), 13 (root), 14 (emphasized), 18 (rating overlay)

### Spacing / layout

No formal scale — grid gaps and padding are typically 4/6/8/10/12/14/16/20/24 px. Use whatever scale your codebase already has.

### Borders, radii, shadows

- Border radius: 2 (chips/badges), 3 (kbd keys), 4 (buttons, pills, rating cards)
- Borders are always 1px hairline (`borderColor` above)
- No drop shadows except on Kbd keys: `inset 0 -1px 0 rgba(0,0,0,0.3)` (dark) or `rgba(0,0,0,0.08)` (light)

---

## App chrome

Defined in `theme.jsx > Win11Chrome`. Wraps every screen.

- **Titlebar** — 32px tall, `bg2`. Logo (16px, accent color) + app title + optional " — Project Name". Min/Max/Close caption buttons on the right (46px each, drawn as SVG, no hover fill in mockups but should have one in production).
- **Tab bar** — 40px tall, appears when a `stage` prop is set. Four tabs: **1 Library · 2 Triage · 3 Select · 4 Route**. Tab numbers rendered in JetBrains Mono at 10px in `fgMute`. Active tab: `fg` color + 2px `accentBlue` bottom border. Right-aligned: project name in mono `fgDim`.
- Do NOT render the tab bar on Library (no project context yet).

---

## Screens

### 1. Library

**File:** `screens-library.jsx` — `LibraryScreen` component. Read the file for exact values.

**Purpose:** Landing screen. Lists all shoots (projects) with import status, triage/select progress, and last-opened time.

**Layout:** Single-column. Top bar with search + "Import" primary button. Main area is a list of shoot cards. Each card: left thumbnail stack, middle metadata column (name, date, camera, photo count), right stats column (✓ kept / ✕ tossed / ★ picks), progress bar spanning the bottom of the card.

**States to implement:**
- `LibraryScreen` (idle) — default list
- `LibraryScreen showImportModal importStage="form"` — import modal open, user picks a folder
- `LibraryScreen showImportModal importStage="progress"` — import running, files being hashed + thumbnails generated, progress bar + log tail

**Import modal form:** Project name input, folder picker (shows selected path + photo count), "Apply AI annotations now" checkbox (default on), Cancel/Start buttons.

**Import progress:** Stage labels (indexing → hashing → thumbnails → AI annotations), a progress bar, a log tail in mono.

### 2. Triage

**File:** `screens-triage.jsx` — `TriageScreenV2` component.

**Purpose:** Keep/toss pass over all imported photos. Narrative-Select-inspired layout with burst detection (consecutive similar photos grouped).

**Layout:** 4-column CSS grid.
1. **All-photos filmstrip** — 92px wide, `bg.rail` background. Thumbnail per photo (78×52). Shows verdict badges (✓ green / ✕ red in top-right corner), group indicators (2px accent stripe on left edge), and stars if rated.
2. **Current group strip** — 148px wide, `bg2`. Label "Group G2" + photo count + burst duration. Larger thumbs (full-width, 82px tall). Active photo has 2px `accentBlue` border.
3. **Main photo area** — flex 1. Top bar: filename (mono) + EXIF (shutter · f-stop · ISO · focal length) + strip/faces toggles + current/total counter. Photo centered with 24px padding, max 780px wide. Two verdict hint cards floating over the photo: `Keep` (P key) on the left in success green, `Toss` (X key) on the right in danger red. Bottom bar: keyboard shortcuts + kept/tossed/remaining counts.
4. **Faces panel** — 220px wide, `bg.rail`. "Faces · N detected" header + 2-col grid of face crops. Each face crop has a verdict pill (✓ keep / ◑ blink / ⌀ blur) + confidence score. Below: frame scores (sharp/face/eye/smile) as `ScoreBar` components. Below that: contextual note (e.g. "Best in group so far · +4% eye-sharpness vs G2-B. One blink detected.").

**Props / variants:**
- `showAllStrip` — boolean, default true. Hides the 92px filmstrip.
- `showFaces` — boolean, default true. Hides the 220px face panel.
- `T` toggles strip, `F` toggles faces (keyboard).

**Artboards:** `triage-full` (all rails), `triage-focus` (no filmstrip), `triage-nofaces` (no filmstrip, no faces — max photo).

**Verdict indicators** (see `FilmstripThumb`):
- keep: 12×12 green square, top-right, white checkmark SVG
- toss: 12×12 red square, top-right, white ✕ SVG
- group member: 2px accent stripe, left edge

**Keyboard shortcuts:**
- `P` keep · `X` toss · `␣` skip · `⇧P` keep all in group · `Z` undo · `T` toggle strip · `F` toggle faces · `G` grid mode

### 3. Select

**File:** `screens-select.jsx` — `SelectScreenV2` component. Mirrors Triage structure but for iterative star-rating passes.

**Purpose:** After keep/toss, narrow the "keep" pool down with repeated ★ passes. Pass 1 rates everything ★1; Pass 2 promotes the best to ★2; etc. Typical photographer workflow: 3–4 passes to reach final picks.

**Layout:** Same 4-column grid as Triage.
1. **All-kept filmstrip** — grouped by star level (★★★ / ★★ / ★ headers, count on the right). Each level is a separate mini-section inside the strip.
2. **Current rating peer strip** — "Rating ★★" header + count. Shows other photos at the same rating as the hero. Active photo has `accentBlue` border.
3. **Main photo area** — Top bar: filename + EXIF + filter pills (all · ★≥1 · ★≥2 · ★≥3 · ★≥4 · ★≥5, selected pill uses `accent` bg) + "Pass N · X/Y" counter. Photo centered. Floating overlays:
   - Top-left: current rating stars (14px) + "current" label in mono.
   - Left side, vertical column of 5 rating cards: `1` `2` `3` `4` `5` key + stars preview. Current rating card highlighted with `accent` border + tinted bg. Below those cards: `0` clear.
   - Right side: `Tab Compare` hint (blue accent2 styling) + `[` `]` narrow-pass hint.
4. **Detail rail** — "Rating" header with current stars + "⇧ rated" caption. Faces grid (same as Triage). Frame scores. Color label row (5 swatches, selected has white ring).

**Props / variants:**
- `variant` — `'hero'` (default) or `'grid'`. Grid falls back to the earlier 3-col filmstrip + grid + rail layout.
- `showAllStrip` / `showRail` — same semantics as Triage.
- `passLevel` — integer, default 2. Selects which filter pill is active.

**Artboards:** `select-hero` (all rails), `select-hero-focus` (no filmstrip), `select-hero-norail` (hero only), `select-grid` (grid variant).

**Keyboard shortcuts:** `1`–`5` rate · `0` clear · `Tab` compare · `[` `]` narrow pass · `G` toggle grid

### 4. Route

**File:** `screens-route.jsx` — `RouteScreenV2` component.

**Purpose:** Send final picks to a RAW processor (Capture One, DxO) or publish destination. Writes XMP sidecars beside the RAW files so ratings travel with them.

Read the file for layout specifics. Typical elements: selected destination cards, picks preview (thumb strip with stars), sidecar write confirmation, "Send" primary button.

### 5. Compare

**File:** `screens-compare.jsx` — `CompareScreen` component.

**Purpose:** 2-up side-by-side when two frames are neck-and-neck. Invoked from Triage or Select with `Tab`.

**Layout:** Top bar ("2-up compare · Group N · locked zoom" + L/R pick hints). 2-col grid, 2px gap, `borderColor` between. Each `ComparePanel`:
- Absolute top-left: `L` or `R` side marker in mono. Shows "✓ PICKED" in green when selected.
- Centered photo with 20px padding, `objectFit: contain`.
- Bottom strip with filename + stars. MVP variant adds score pills (sharp/face/eye/smile, values ≥85 use `accent2`). Lean variant is just "sharp N · eye N".

Bottom bar: "Winner promoted to 2★" + "pan + zoom synchronised · eye-level".

**Props:** `variant` — `'mvp'` (default, score pills) or `'lean'` (minimal metadata).

**Artboards:** `compare-mvp`, `compare-lean`.

**Keyboard shortcuts:** `1` pick L · `2` pick R · `Tab` toggle back · `Esc` exit

### 6. Grid mode (cross-cutting)

**File:** `screens-grid.jsx` — `GridMode` component.

**Purpose:** `G` hotkey toggles a pure-grid view on any screen. Dense thumbnails, contextual badges, stage-specific hint bar.

**Layout:** Top bar (stage-specific: shows filter pills for Select/Triage, title+count for Library/Route). Main area: 8-column grid of 110px-tall thumbnails, 10px gap, 18px padding, `bg` background. Bottom bar: stage-specific keyboard hints + `range X–Y · focused #N` status.

**Per-stage behaviors** (switched by `stage` prop):
- `library` — shoot label + photo count in bottom corners. Hints: `G` grid/list · `↵` open · `⌫` archive.
- `triage` — ✓ keep or ✕ toss badge in top-right (14×14 circle, success/danger bg). Tossed photos dimmed to 0.35 opacity. Filter pills: all / untriaged / kept / tossed.
- `select` — stars badge bottom-left (black translucent pill, 9px stars). Filter pills: all / ★≥1 / ★≥2 / ... / ★≥5.
- `route` — stars + "→ C1" destination tag bottom-right (accent2 mono).

**Focus / selection state:**
- Focused cell: `2px solid accentBlue` outline, 3px offset
- Range-selected: `1px solid accentBlue` + `rgba(74,130,217,0.12)` inner wash
- Props: `focusedIndex` (integer), `selectedRange` ([start, end] inclusive)

**Artboards:** `grid-library`, `grid-triage`, `grid-select`, `grid-route`.

---

## Shared primitives

Defined in `placeholders.jsx`. Reimplement in your target framework.

### `Photo({ seed, w, h, sharp, dim, children, style })`
Placeholder image — deterministic gradient keyed by `seed`, muted palette with diagonal stripe overlay. In production, replace with real thumbnail rendering. `sharp` (0–1, default 0.85) applies a CSS blur for low-sharp photos. `dim` (default 1) is opacity — e.g. tossed photos render at 0.35.

### `Stars({ n, max=5, size=11, color='#e8d37a' })`
Star cluster. Filled if `i < n`, outlined otherwise. Uses inline SVG (see file for path).

### `ExifChip({ shutter, iso, fstop, lens })`
Inline mono pill — `1/500 · f/2.8 · ISO 400 · 85mm`. Black translucent bg, backdrop-blur.

### `Kbd({ children, dark=true })`
Keyboard-key pill. 18×18 min, mono 10px, subtle inner shadow, 3px radius.

### `ScoreBar({ label, value, max=100, color })`
Horizontal bar: 48px label (uppercase, mono, fgDim) + track + tabular-nums value. Bar tracks are 3px tall on rgba(255,255,255,0.08) background.

### `ColorLabel({ color })`
8×8 rounded color chip (Lightroom label colors).

### Logo — `logos.jsx`
Four aperture-based marks (A/B/C/D). **Hero mark is `LogoB`** — three stacked apertures as sieves, recommended throughout. Used at size 16 in the titlebar; larger sizes in brand contexts.

---

## Interactions & behavior

### Navigation
- Top tabs clickable to jump between stages within a project
- `1`/`2`/`3`/`4` number keys (when no photo focused) jump stages
- Back button from Triage/Select/Route returns to Library

### Photo navigation (Triage / Select / Compare)
- `←` `→` — previous/next photo
- Clicking a filmstrip or strip thumb jumps to that photo
- Filmstrip auto-scrolls to keep the active thumb in view

### Rating (Select)
- `1`–`5` sets the rating. `0` clears.
- `[` narrows the pass filter (e.g. Pass 2 → Pass 3, showing only ★≥3). `]` widens.
- Rating promotes/demotes the photo in the all-kept filmstrip grouping without leaving the screen.

### Keep/toss (Triage)
- `P` keeps, `X` tosses. Auto-advance to next photo.
- `⇧P` keeps all in current burst group.
- `Z` undoes the last verdict. Multi-level undo.

### Grid mode (any stage)
- `G` toggles grid ↔ drill-down
- Arrow keys move focused cell; `Shift+Arrow` extends range
- Same stage-specific action keys work on the focused cell (`1`–`5` rate, `P`/`X` keep/toss, etc.)

### Compare
- `Tab` from Triage or Select with 2+ selected photos opens compare
- Pan/zoom are synchronized across both panels
- `1` picks left, `2` picks right — the winner is promoted (in Select) or kept (in Triage), the loser is demoted/tossed

### Animation
No complex animations specified. Use:
- 150ms ease-out for hover state changes
- 200ms ease-in-out for panel show/hide (T/F toggles in Triage, grid transition)
- Instant for verdict/rating changes (snappy feedback is the whole point)

---

## State

Persistent, per-project:
- Photo records: `{ path, hash, exif, thumbnail, verdict, stars, color, groupId, faces[], scores }`
- Faces: `{ crop, verdict: 'keep'|'blink'|'blur', confidence }`
- Scores: `{ sharp, face, eye, smile }` (0–100)
- Burst groups: detected during import (consecutive shots within N seconds of each other)
- History: verdict/rating log for undo

Session (not persisted):
- Current stage, focused photo, pass level, selection range, grid-vs-hero toggle, rail visibility

XMP sidecars (on Route): write `xmp:Rating`, `xmp:Label`, and custom Photosift namespace tags next to the RAW files so ratings survive outside the app.

---

## Assets

All visuals are drawn with CSS + SVG primitives. No raster assets to bundle. In production, you'll need:
- Real icon set for caption buttons and UI icons (use Fluent System Icons or Phosphor)
- JetBrains Mono + Segoe UI Variable fonts (Segoe UI Variable ships with Windows 11; include JetBrains Mono as a web font or bundle)
- Photo thumbnails generated from actual RAW files on import

---

## Files in this bundle

| File | Purpose |
|------|---------|
| `Photosift.html` | Entry point — mounts the design canvas with all artboards |
| `design-canvas.jsx` | **Ignore** — canvas wrapper for design presentation, not product code |
| `theme.jsx` | `darkTheme` tokens + `Win11Chrome` (titlebar + tab bar) |
| `placeholders.jsx` | `Photo`, `Stars`, `Kbd`, `ScoreBar`, `ExifChip`, `ColorLabel` — shared primitives |
| `logos.jsx` | Four logo variants (A/B/C/D). Hero is `LogoB`. |
| `screens-library.jsx` | Library + Import modal |
| `screens-import.jsx` | (May overlap with library; check for the import modal states) |
| `screens-triage.jsx` | Triage screen + FilmstripThumb + FaceChip primitives |
| `screens-select.jsx` | Select screen (hero + grid variants) |
| `screens-route.jsx` | Route / destinations |
| `screens-compare.jsx` | 2-up compare + ComparePanel |
| `screens-grid.jsx` | Cross-cutting grid mode |

Open `Photosift.html` in a browser to see all artboards side-by-side. Each artboard is labeled with its state.

---

## Suggested implementation order

1. **Tokens + chrome** — port `theme.jsx` + `Win11Chrome`
2. **Primitives** — port `Photo` (as real thumbnail renderer), `Stars`, `Kbd`, `ScoreBar`, `ExifChip`
3. **Library + Import** — gets you a working shell that can ingest photos
4. **Triage** — the hot path. Most users spend the most time here.
5. **Select** — reuses Triage patterns (filmstrip, detail rail); go faster here
6. **Compare** — small, self-contained
7. **Grid mode** — cross-cutting; implement last with the `G` hotkey wired at the app level
8. **Route** — XMP writing + external-app handoff

---

## Questions for the designer

If anything in this doc is ambiguous, ping with the screen name + what's unclear. Common things I'd expect follow-ups on:
- Exact RAW processor integrations (Capture One `.cosessiondb`? just launch with files?)
- Burst group detection threshold (currently implicit; spec as "N seconds between shots")
- Multi-monitor support
- Touch/pen input (probably out of scope for v1)
