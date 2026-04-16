# Three-Pass Culling UI — Design Spec

**Date**: 2026-04-15
**Status**: Approved
**Scope**: View filtering, group display, progress bar, visual feedback, grid view, 2-up comparison

## Overview

Add the three-pass culling workflow to PhotoSift: Triage → Select → Route. Each pass is a filter on the photo list with pass-specific group behavior and keyboard actions. This builds on the existing sequential view, store, and culling commands.

## Architecture: Store-Centric Derived State

All filtering is client-side in the Zustand store. The backend provides raw data (all photos + all groups); the store computes `displayItems` as a derived list based on the active view.

### New Store State

```typescript
currentView: "triage" | "select" | "route"
groups: Group[]  // loaded once on shoot open
viewMode: "sequential" | "grid" | "comparison"
```

### New Type

```typescript
interface Group {
  id: number
  shootId: number
  groupType: "near_duplicate" | "related"
  members: { photoId: number; isCover: boolean }[]
}
```

### Derived State: `displayItems`

A computed list representing what the user navigates through. `currentIndex` indexes into this list, not raw `images`.

| View | Filter | Group behavior |
|------|--------|----------------|
| Triage | `flag = "unreviewed"` | Collapsed: cover image only, count badge. P/X applies to all members via `bulk_set_flag`. |
| Select | `flag != "reject"` | Expanded: all members visible, ordered together. P auto-rejects siblings. |
| Route | `flag = "pick"` AND `destination = "unrouted"` | None: groups dissolved, per-photo routing. |

### Default View

When opening a shoot for the first time, default to Triage. On subsequent opens, restore the last-used view from `view_cursors`.

### View Cursor Persistence

On view change, save current photo_id to `view_cursors` (Tauri command). On switch back, restore position from `view_cursors`. Falls back to index 0 if no saved cursor.

### Auto-Show Metadata in Route

`showMetadata` defaults to `true` when switching to Route view (EXIF helps assess edit vs publish). User can still toggle off with `I`.

## Keyboard Remapping

**Breaking change**: Space and Z are remapped to match the spec.

| Key | Old behavior | New behavior |
|-----|-------------|--------------|
| Space | Toggle zoom | Advance to next unreviewed |
| Z | (unbound) | Toggle fit-to-screen vs 100% crop |

### Full Keyboard Map by Pass

**Navigation (all passes)**:
- `→` / `←` — Next / previous in display list
- `↑` / `↓` — Next / previous group (jump to cover)
- `Space` — Advance to next unreviewed
- `Home` / `End` — First / last in current view
- `G` — Toggle grid view
- `Enter` — Grid: jump to sequential. Sequential: expand/collapse group.

**Triage & Select**:
- `P` — Pick (in Triage: applies to all group members; in Select: auto-rejects group siblings)
- `Shift+P` — Pick without auto-rejecting siblings (Select only)
- `X` — Reject
- `U` — Reset to unreviewed
- `C` — Set as group cover image (Select only)
- `Ctrl+Z` — Undo

**Route**:
- `E` — Mark for edit
- `D` — Mark as publish direct
- `U` — Reset to unrouted

**Comparison (2-up)** — these override normal key bindings while in comparison mode:
- `Tab` — Enter comparison (from expanded group in Select)
- `Shift+Tab` / `Escape` — Exit comparison
- `← →` — Cycle right panel through group members (overrides normal navigation)
- `1` — Pick left panel (auto-reject right) — overrides star rating `1`
- `2` — Pick right panel (auto-reject left) — overrides star rating `2`
- Scroll wheel — Linked zoom
- Click-drag — Linked pan

**Zoom**:
- `Z` — Toggle fit-to-screen vs 100% crop
- Scroll wheel — Zoom in/out (when not in comparison)
- Click-drag — Pan when zoomed

## Toolbar Layout: Two-Row (Option B)

**Row 1**: Back button, shoot name + date, auto-advance toggle.
**Row 2**: Underline-style view tabs (Triage | Select | Route) on the left, inline progress bar (180px, pick=green/reject=red) + stats text on the right.
**Below Row 2**: 3px colored progress bar spanning full width showing pick/reject/unreviewed ratio.

### Progress Stats Format

- Triage: `"187/412 reviewed · 94✓ · 93✗"`
- Select: `"319 remaining · 94 picks"`
- Route: `"12 → Edit · 34 → Publish Direct · 19 unrouted"`

Stats computed client-side from the `images` array — no backend query needed.

## Visual Feedback: Flag Flash

A `FlagFlash` overlay component on the loupe:
- **Pick**: `rgba(34, 197, 94, 0.15)` full-viewport wash, fades over 300ms
- **Reject**: `rgba(239, 68, 68, 0.15)` full-viewport wash, fades over 300ms
- Triggered by store state: `lastFlagAction: { color, timestamp }`
- CSS `@keyframes` animation, no JS timers
- **Auto-advance timing**: 150ms delay after flash starts, then image swaps. User sees flash on the photo they judged.

## Group Display

### New Backend Commands

1. **`get_groups_for_shoot(shoot_id: i64)`** → `Vec<GroupInfo>`
   ```rust
   struct GroupInfo {
       id: i64,
       shoot_id: i64,
       group_type: String,  // "near_duplicate" | "related"
       members: Vec<GroupMemberInfo>,
   }
   struct GroupMemberInfo {
       photo_id: i64,
       is_cover: bool,
   }
   ```

2. **`set_group_cover(group_id: i64, photo_id: i64)`** → `()`
   Sets `is_cover = true` for the specified member, clears it on the previous cover.

### Triage: Collapsed Groups

- **Filmstrip**: Stacked thumbnail with offset shadow effect (two shadow layers behind cover). Blue count badge top-right (e.g., "12").
- **Loupe**: Group badge top-right: stack icon + "Group · 12 photos".
- **Navigation**: Arrow keys skip hidden members — only covers and singles are in `displayItems`.
- **Actions**: P/X on cover → `bulk_set_flag(all_member_ids, flag)`. Group disappears from `displayItems` after action.

### Select: Expanded Groups

- **Filmstrip**: Group members wrapped in a subtle blue-bordered container with "Group · N" label. Vertical separator bars between groups.
- **Group context strip**: Between loupe and filmstrip. Shows all members of the current group at larger size (72×54px). Active member has blue ring + glow. Label: "Tab for 2-up comparison".
- **Actions**: P auto-rejects other group members. Shift+P picks without auto-rejecting. C sets group cover.

### Route: No Groups

Groups are dissolved. Only individual picked photos appear.

## Grid View

Toggle with `G` key. Replaces loupe + filmstrip with a scrollable thumbnail grid.

### Layout
- CSS Grid with `auto-fill`, `minmax(columnWidth, 1fr)`
- Three sizes controlled by `+`/`-`: Small (100px), Medium (160px), Large (240px)
- `overflow-y: auto` — scrolls vertically to fit all photos
- Fills entire viewport below toolbar

### Thumbnail Features
- Flag dot: green (pick), red (reject), top-left corner
- Destination badge: "EDIT" (purple) or "PUBLISH" (blue), top-right corner
- Rejected photos: 35% opacity + desaturated
- Group stack effect: offset shadow + "+N" count badge (bottom-right)
- Filename on hover (bottom gradient overlay)

### Selection & Bulk Actions
- Click: select (blue border)
- Shift+click: range multi-select (purple border)
- Multi-select action bar appears at bottom: P Pick, X Reject, U Reset, Enter → Loupe
- P/X with selection: applies to all selected photos
- Enter on selection: jump to sequential view at that photo

### Grid Navigation
- Arrow keys move selection through the grid (left/right within row, up/down across rows)
- `Home`/`End` jump to first/last item
- `P`/`X`/`U` apply to the focused thumbnail (or all selected if multi-selected)

### View Filtering
Grid respects the same `displayItems` filter as sequential view — in Triage shows only unreviewed, in Route shows only unrouted picks.

## 2-Up Comparison View

Entered with `Tab` when viewing a group member in Select pass.

### Layout
- Two panels side by side, separated by 2px divider
- **Left panel** (blue, "Pinned"): fixed reference image. Label: "① Pinned"
- **Right panel** (purple, "Cycling"): swaps between group members with arrow keys. Label: "③ Cycling ← →". Position indicator: "3 / 5"

### Linked Zoom/Pan
- Scroll wheel zooms both panels simultaneously
- Click-drag pans both panels
- Shared viewport transform: `{ x, y, scale }`
- Zoom indicator shows current scale (e.g., "100%")

### Quick Pick
- `1` picks left panel photo, auto-rejects right panel photo
- `2` picks right panel photo, auto-rejects left panel photo
- After pick, exits comparison if only one member remains unpicked

### EXIF Comparison Strip
Thin row below panels showing side-by-side exposure: f-stop, shutter speed, ISO, focal length. Helps spot exposure differences.

### Group Member Strip
Below EXIF strip. Shows all group members as small thumbnails. Blue dot = pinned (left), purple dot = cycling (right).

### Entry/Exit
- Enter: `Tab` from sequential view when current photo is in a group (Select pass)
- Exit: `Shift+Tab` or `Escape` → returns to sequential view at the pinned photo
- If not in a group or not in Select pass, `Tab` is a no-op

## New Backend Commands Summary

| Command | Params | Returns | Purpose |
|---------|--------|---------|---------|
| `get_groups_for_shoot` | `shoot_id: i64` | `Vec<GroupInfo>` | Load groups + members on shoot open |
| `set_group_cover` | `group_id: i64, photo_id: i64` | `()` | C key in Select pass |

## Files to Create/Modify

### New Files
- `src/components/ViewSelector.tsx` — Underline tab bar for Triage/Select/Route
- `src/components/ProgressBar.tsx` — Colored ratio bar + stats text
- `src/components/FlagFlash.tsx` — Green/red flash overlay
- `src/components/GridView.tsx` — Thumbnail grid with selection
- `src/components/ComparisonView.tsx` — 2-up linked zoom/pan
- `src/components/GroupStrip.tsx` — Group context strip (below loupe in Select)
- `src/components/GroupStack.tsx` — Collapsed group thumbnail for filmstrip/grid

### Modified Files
- `src/stores/projectStore.ts` — Add `currentView`, `groups`, `viewMode`, `displayItems` computation, view switching, cursor persistence
- `src/hooks/useKeyboardNav.ts` — Remap Space/Z, add view-aware key handling (Shift+P, C, Tab, G, +/-, 1/2, group navigation)
- `src/pages/CullPage.tsx` — Compose view modes (sequential/grid/comparison), integrate new toolbar components
- `src/components/Toolbar.tsx` — Two-row layout with ViewSelector + ProgressBar
- `src/components/Filmstrip.tsx` — Render GroupStack for collapsed groups, group containers for expanded
- `src/components/LoupeView.tsx` — Group badge overlay, integrate FlagFlash
- `src-tauri/src/db/schema.rs` — Add `get_groups_for_shoot()`, `set_group_cover()` queries
- `src-tauri/src/commands/culling.rs` — Add `get_groups_for_shoot`, `set_group_cover` commands
- `src-tauri/src/lib.rs` — Register new commands
- `src/types/index.ts` — Add `Group`, `GroupMemberInfo`, view type enums

## Implementation Priority

1. **View filtering** (store + toolbar + keyboard) — highest impact, enables the three-pass workflow
2. **Progress bar** — immediate visual feedback on culling progress
3. **Flag flash** — visual polish for triage tempo
4. **Group display** — collapsed (Triage) + expanded (Select) + new backend commands
5. **Grid view** — thumbnail overview with multi-select
6. **2-up comparison** — linked zoom/pan for Select pass

## Verification

1. Import a shoot of 50+ NEF files with natural bursts
2. Triage: verify only unreviewed photos appear, groups are collapsed, P/X on cover affects all members, Space advances to next unreviewed
3. Select: verify rejected photos are hidden, groups expanded, P auto-rejects siblings, Shift+P doesn't, Tab opens comparison
4. Route: verify only unrouted picks appear, E/D sets destination, metadata auto-shows
5. Grid: verify G toggles, thumbnails show badges, multi-select works, Enter jumps to sequential
6. Comparison: verify linked zoom/pan, arrow cycling, 1/2 quick pick, Shift+Tab exits
7. View switching: verify cursor restores position per view
8. Progress bar: verify counts update live on flag/destination changes
9. Use `scripts/screenshot.ps1` for visual verification of each view
