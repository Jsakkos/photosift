# PhotoSift — Future Work

Items flagged after shipping Phase A–E (settings panel, pHash config, XMP import/export, manual groups, triage-expand) on 2026-04-16. Add your own action items below.

## 1. Stricter per-view filter gates (user-configurable)

The current view filters don't match the intended mental model:

| View    | Current filter                              | Intended filter                          |
|---------|---------------------------------------------|------------------------------------------|
| Triage  | `flag = unreviewed`                         | same (pick/reject gate)                  |
| Select  | `flag != reject` (picks + unreviewed)       | `flag = pick` only (must pass triage)    |
| Route   | `flag = pick && destination = unrouted`     | `flag = pick && star >= 3 && unrouted`   |

Implication: stars become part of the Select pass, not post-edit. `PhotoSift_Spec.md` currently says stars are post-edit — the spec needs to be updated (or the divergence called out explicitly).

**Why:** Stricter progressive-narrowing pipeline — each pass fully qualifies a photo before it's eligible for the next pass.

**Approach:** Add two fields to the `settings` table + Zustand settings store:
- `select_requires_pick` (default `true`) — Select view filters `flag = pick`
- `route_min_star` (default `3`, `0` disables) — Route view adds `star_rating >= N`

Update `computeDisplayItems` in `src/stores/projectStore.ts` (select branch ~line 98, route branch ~line 116) to read these. Surface in `SettingsDialog.tsx` as two extra controls. Extend `computeDisplayItems.test.ts` for the new gates.

---

## 2. Shortcut reference + tooltips

`src/components/ShortcutHints.tsx` already exists, bound to `?`. It's **incomplete** — missing recent additions: `,` (settings), `Ctrl+G` / `Ctrl+Shift+G` (group/ungroup), `Ctrl+E` (export XMP), comparison mode keys (`1`/`2`/`Tab`/`Shift+Tab`). Users don't know `?` opens it — no persistent hint anywhere in the UI.

**Approach:**
- Add the missing entries to `NAV_SHORTCUTS` / section arrays in `ShortcutHints.tsx` (group editing in `SELECT_SHORTCUTS`; global `,` and `Ctrl+E` in `NAV_SHORTCUTS`).
- Add a small persistent footer hint (Toolbar or above Filmstrip): `Press ? for shortcuts` — subtle text, dismissible.
- Add `title=` tooltips on every clickable button currently missing one. The gear icon and Group/Ungroup buttons already have titles; follow that pattern. Quick audit: most Toolbar / ViewSelector / GridView bulk-action buttons are bare.

---

## 3. Group expand/collapse in triage (keyboard + double-click)

No way to expand a collapsed group from the keyboard in triage. The Phase A change made double-click on a GroupStack enter loupe mode; the intent is that double-click should expand the group (and a keyboard shortcut should do the same).

**Design tension:** double-click currently routes to loupe everywhere (consistent gesture). Expanding is a different intent.

**Recommended split:**
- **Keyboard:** `Enter` on a focused GroupStack in triage (or select) expands inline; Enter again collapses. Scope this so in sequential/triage with a GroupStack focused, Enter = toggle-expand.
- **Double-click:** Reserve for loupe globally, but special-case GroupStack — double-click on a *collapsed* group expands it; double-click on a thumb (or expanded member) enters loupe. Context-sensitive but readable.

Reuse the Phase-D `expandedGroupIds: Set<number>` idea that was deferred in the original plan — add to `projectStore`, modify `computeDisplayItems` triage branch to emit members when the group's id is in the set. `Filmstrip.tsx` and `GridView.tsx` call the toggle action on GroupStack double-click / Enter.

**Why:** The "Expand groups in triage by default" setting (Phase D) is all-or-nothing; per-group drill-down lets users stay in triage tempo while resolving individual bursts.

---

## Your action items

1. Allow file import to be more configurable. I want to be able to set either import in-place, which will help me go through my existing files, or copy to library, for new photos. I need to be able to set the library location. Right now, we've been using an alternate location for testing, but that will not be the location I want for production.
2. Add eye detection and sharpness viewing, like narrative select.
3. Add face dectection and sharpness viewing like narrative select.

