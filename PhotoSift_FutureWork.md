# PhotoSift — Future Work

Forward-looking action items. Shipped work is logged at the bottom.

---

## Open items

### 1. Complete the shortcuts overlay

`src/components/ShortcutsOverlay.tsx` exists and is bound to `?` (and `,` opens settings), but it is still missing several shortcuts that `useKeyboardNav.ts` actually binds:

- `Ctrl+E` — export XMP sidecars (global)
- `Ctrl+G` / `Ctrl+Shift+G` — group / ungroup (Select view)

There is also no persistent on-screen hint that `?` opens the overlay — users have to discover it. Consider a subtle, dismissible "Press `?` for shortcuts" line in the toolbar or above the filmstrip.

While in there: audit every clickable button in `Toolbar.tsx`, `ViewSelector.tsx`, and the Grid bulk-action buttons for missing `title=` attributes (the gear icon and Group/Ungroup already have them — follow that pattern).

---

## Your action items

Add your own here. Resolved items move to the Shipped log below.

---

## Shipped log

### 2026-04-21 — UI overhaul, AI enrichment surface

- **Per-view filter gates** (originally listed as future item #1) — `select_requires_pick` and `route_min_star` are first-class settings stored in the `settings` table and read by `computeDisplayItems` in `src/stores/projectStore.ts`. Exposed in `SettingsDialog.tsx` and covered by `src/stores/__tests__/computeDisplayItems.test.ts`.
- **Group expand/collapse in triage** (originally listed as future item #3) — `expandedGroupIds: Set<number>` lives on `projectStore`. `computeDisplayItems` triage branch emits members when a group's id is in the set. Toggled from `GroupStack` interactions.
- **Eye + sharpness viewing** (originally an action item) — `EyeStatusBadge`, `AiSharpnessBadge`, `AiSmileIcon`, and `FaceThumb` surface the per-photo AI verdicts in `select/DetailRail` and `triage/FacesRail`.
- **Face detection viewing** (originally an action item) — `FaceChip`, `FaceThumb`, `triage/FacesRail`, and `HeatmapOverlay` (toggle with `H`) visualize face detections from the on-device AI worker.
- **In-place vs copy import** (originally an action item) — `src/components/ImportDialog.tsx` has an `ImportMode = "copy" | "in_place"` selector. Library root + bucket folder names are user-configurable in Settings via `FolderLayoutEditor.tsx`, persisted to the `settings` table.
- **Shortcut overlay (partial)** — `ShortcutsOverlay` exists and is keyboard-discoverable. Remaining gaps tracked above as open item #1.

### 2026-04-16 — Phase A–E

Initial settings panel, configurable pHash thresholds, XMP import/export, manual groups, triage-expand foundation.
