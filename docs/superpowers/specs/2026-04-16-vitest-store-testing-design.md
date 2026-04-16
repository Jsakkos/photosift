# Vitest Store Testing for PhotoSift

**Date**: 2026-04-16  
**Status**: Approved  
**Scope**: Zustand store logic tests with mocked Tauri invoke()

## Problem

PhotoSift has zero frontend tests. The most complex logic lives in the Zustand store — group flag propagation, auto-reject siblings, undo across batched operations, view cursor restore, comparison mode lifecycle. These are pure state machine problems that have been verified only by manual testing. A regression in `computeDisplayItems` or `setFlag` would silently break the culling workflow.

## Approach

**Vitest + mocked invoke()** — test the store layer in isolation without Tauri or DOM rendering. Uses the official `@tauri-apps/api/mocks` module to intercept IPC calls. Zustand stores expose `.getState()` / `.setState()` for direct manipulation outside React.

### Dependencies

```
npm install -D vitest @vitest/coverage-v8 happy-dom
```

No other packages needed. `@tauri-apps/api/mocks` ships with the existing `@tauri-apps/api@^2`.

## Architecture

### Test Infrastructure (`src/test/`)

| File | Purpose |
|------|---------|
| `setup.ts` | Global afterEach: `clearMocks()` + reset Zustand store to initial state |
| `mockIpc.ts` | `setupMockIpc(handlers, spyFn)` — configurable return values per command + optional spy callback |
| `fixtures.ts` | `makeImage()`, `makeGroup()`, `makeShoot()`, `makeGroupWithImages()` factories with auto-incrementing IDs |

### Mock Strategy

`mockIPC(handler)` from `@tauri-apps/api/mocks` replaces `window.__TAURI_INTERNALS__.invoke`. The `setupMockIpc` helper wraps this with:
- A `handlers` object mapping command names to return values (configurable per test)
- An optional `spyFn` callback for asserting which commands were called with what arguments
- Throws on unmocked commands to catch missing mock coverage

### Test Files (`src/stores/__tests__/`)

| File | Covers | P1/P2 |
|------|--------|-------|
| `computeDisplayItems.test.ts` | Triage collapse, select expand, route filter, empty states | P1, P2 |
| `projectStore.setFlag.test.ts` | Auto-reject siblings, Shift+P, triage group flag, invoke assertions | P1 |
| `projectStore.undo.test.ts` | Single revert, batch revert, redo, empty stack | P2 |
| `projectStore.views.test.ts` | Cursor save/restore, view switch roundtrip, empty route | P2 |
| `projectStore.comparison.test.ts` | Enter/exit, cycle, quickPick, edge cases | P1 |
| `projectStore.routing.test.ts` | setDestination, displayItems update, index clamping | P2 |
| `projectStore.loadShoot.test.ts` | Initial load, cursor restore, error handling | — |

### Source Change Required

Export 3 pure functions from `src/stores/projectStore.ts`:
- `computeDisplayItems(images, currentView, groups)`
- `buildPhotoGroupMap(groups)`
- `getGroupCover(group)`

These have no side effects — exporting enables direct testing without the store's async machinery.

## Configuration

**vitest.config.ts** (project root):
- `environment: "happy-dom"` — provides `window`, `crypto` needed by `@tauri-apps/api/mocks`
- `globals: true` — `describe`/`test`/`expect`/`vi` available without imports
- `setupFiles: ["src/test/setup.ts"]`
- Coverage scoped to `src/stores/**` and `src/hooks/**`

**package.json** scripts:
- `"test": "vitest"` (watch mode)
- `"test:run": "vitest run"` (single run)
- `"test:coverage": "vitest run --coverage"`

**tsconfig.json**: add `"vitest/globals"` to `compilerOptions.types`.

## Known Issue Discovered

The undo system has a bug: undoing a group flag in triage only reverts the cover image, not siblings. The undo entry stores `batchSize` but only the cover's `imageId` and `oldValue`. Tests will document this behavior for a follow-up fix.

## Test Patterns

### Pure function tests (computeDisplayItems)
Set up images + groups, call function directly, assert on returned DisplayItem array.

### Store action tests (setFlag, undo, setView)
1. `setupMockIpc(handlers, spy)` — configure IPC mock
2. `useProjectStore.setState({...}, true)` — set initial state
3. `await useProjectStore.getState().someAction()` — call action
4. `useProjectStore.getState()` — assert on resulting state
5. Assert on `spy` calls for invoke verification

### Timer-dependent tests (auto-advance)
Use `vi.useFakeTimers()` + `vi.advanceTimersByTime(150)` or set `autoAdvance: false`.

## Not In Scope

- Component rendering tests (React Testing Library)
- E2E / Playwright tests
- Keyboard navigation hook tests (depend on DOM events)
- Visual regression testing

These can be added incrementally later.
