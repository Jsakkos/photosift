// Helpers shared by every spec under tests/e2e/screenshots/.
//
// The pattern each spec follows:
//   beforeEach -> startSession() (navigate + inject e2e.css + reset)
//   inside it: seedFixture(...) / navigateTo(...) / waitForQuiescence()
//   inside it: browser.checkScreen("tag")
//
// Don't reach into Tauri internals directly from a spec — go through
// these helpers so the determinism mitigations (font AA, animations,
// caret) stay in lockstep across the suite.

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const E2E_CSS_PATH = resolve(__dirname, "../fixtures/e2e.css");

// ---- low-level page bootstrap -----------------------------------------

/** Wait for window.__TAURI__ to appear after navigation. */
export async function waitForTauri(timeoutMs = 20_000) {
  await browser.waitUntil(
    async () => await browser.execute(() => !!window.__TAURI__?.core?.invoke),
    {
      timeout: timeoutMs,
      timeoutMsg: "window.__TAURI__ never appeared",
    },
  );
}

/** Inject the deterministic-rendering stylesheet. Idempotent — safe to
 * call after every navigation in case the page reloaded. */
export async function injectE2eCss() {
  const css = await readFile(E2E_CSS_PATH, "utf8");
  await browser.execute((cssText) => {
    const existing = document.getElementById("__photosift_e2e_css");
    if (existing) existing.remove();
    const style = document.createElement("style");
    style.id = "__photosift_e2e_css";
    style.textContent = cssText;
    document.head.appendChild(style);
  }, css);
}

/** Pin the Tauri window to a known size so screenshots are dimension-stable. */
export async function pinWindowSize(width = 1440, height = 900) {
  await browser.execute(
    async (w, h) => {
      try {
        const { getCurrentWindow } = window.__TAURI__.window;
        const win = getCurrentWindow();
        await win.setSize(new window.__TAURI__.dpi.LogicalSize(w, h));
      } catch (e) {
        // Older Tauri builds expose the API under `appWindow`.
        console.warn("setSize failed:", e);
      }
    },
    width,
    height,
  );
}

/** All-in-one session bootstrap. Call from `before` / `beforeEach`. */
export async function startSession() {
  await browser.url("http://tauri.localhost/");
  await waitForTauri();
  await injectE2eCss();
  await pinWindowSize();
}

// ---- debug-only Tauri command wrappers --------------------------------

/** Drop the existing fixture state and seed a fresh one. The `spec` is
 * the SeedRequest contract from src-tauri/src/commands/testing.rs.
 *
 * After seeding, push the new DB state into the React stores so the UI
 * reflects it without a page reload — ShootListPage fetched its data
 * at mount time, well before this helper runs, so any test that does
 * `seedFixture(...)` then `navigateTo("/shoots")` would otherwise see
 * stale shoots from a previous spec. */
export async function seedFixture(spec) {
  const result = await browser.execute(async (request) => {
    try {
      const out = await window.__TAURI__.core.invoke("seed_test_fixtures", {
        request,
      });
      // Refresh the shoot list + settings stores after the DB swap.
      const ps = window.__PHOTOSIFT__;
      if (ps?.shootListStore) {
        try {
          await ps.shootListStore.getState().refresh();
        } catch {}
      }
      if (ps?.settingsStore) {
        try {
          await ps.settingsStore.getState().loadSettings();
        } catch {}
      }
      return { ok: true, value: out };
    } catch (e) {
      return { ok: false, err: String(e?.message ?? e) };
    }
  }, spec);
  if (!result.ok) throw new Error(`seed_test_fixtures: ${result.err}`);
  return result.value;
}

/** Pin ephemeral AppState fields (curator status etc.) before a shot. */
export async function setScreenshotState(patch) {
  const result = await browser.execute(async (p) => {
    try {
      await window.__TAURI__.core.invoke("set_screenshot_state", { patch: p });
      return { ok: true };
    } catch (e) {
      return { ok: false, err: String(e?.message ?? e) };
    }
  }, patch);
  if (!result.ok) throw new Error(`set_screenshot_state: ${result.err}`);
}

// ---- navigation + quiescence ------------------------------------------

/** Navigate via the URL hash because the app mounts under <HashRouter>.
 * Setting `location.hash` triggers the router's hashchange listener and
 * unmounts the previous screen, so we always wait for the target
 * data-testid before returning. */
export async function navigateTo(path, expectTestId) {
  await browser.execute((p) => {
    // HashRouter expects `#/foo`, not `#foo`. Strip leading slashes
    // we may have been handed and re-add exactly one.
    const clean = p.startsWith("/") ? p : `/${p}`;
    window.location.hash = `#${clean}`;
  }, path);
  if (expectTestId) await waitForTestId(expectTestId);
}

/** Wait for a `[data-testid=X]` element to appear in the DOM. */
export async function waitForTestId(testId, timeoutMs = 10_000) {
  await browser.waitUntil(
    async () =>
      await browser.execute(
        (t) => !!document.querySelector(`[data-testid="${t}"]`),
        testId,
      ),
    {
      timeout: timeoutMs,
      timeoutMsg: `data-testid="${testId}" never appeared`,
    },
  );
}

/** Poll until the UI is "still": no images decoding, no fonts loading,
 * no in-flight micro-tasks scheduled at this exact moment. Critical for
 * screenshot determinism — without it, a thumb that decodes 30ms after
 * the test's last interaction will flap the diff. */
export async function waitForQuiescence(timeoutMs = 8_000) {
  await browser.waitUntil(
    async () =>
      await browser.execute(() => {
        if (document.readyState !== "complete") return false;
        if (typeof document.fonts !== "undefined" && document.fonts.status !== "loaded") {
          return false;
        }
        const imgs = Array.from(document.querySelectorAll("img"));
        for (const img of imgs) {
          if (!img.complete) return false;
          if (img.naturalWidth === 0 && img.src) return false;
        }
        return true;
      }),
    {
      timeout: timeoutMs,
      timeoutMsg: "UI never settled (fonts/images still loading)",
    },
  );
  // One more rAF to flush any last-tick layout changes.
  await browser.execute(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );
}

/** Sequence of keyboard keys, one event per press. Uses
 * `browser.keys` which dispatches via WebDriver — matches user input. */
export async function pressKeys(keys) {
  for (const k of keys) {
    await browser.keys(k);
  }
  await waitForQuiescence();
}

/** Switch the cull view via the exposed Zustand store. The TabBar
 * "1/2/3/4" badges aren't actual keyboard bindings (useKeyboardNav
 * binds those keys to ratings inside Select view), so the only stable
 * way to switch is programmatic. */
export async function switchView(view) {
  const result = await browser.execute(async (v) => {
    try {
      const store = window.__PHOTOSIFT__?.projectStore;
      if (!store) return { ok: false, err: "__PHOTOSIFT__.projectStore unavailable" };
      await store.getState().setView(v);
      return { ok: true };
    } catch (e) {
      return { ok: false, err: String(e?.message ?? e) };
    }
  }, view);
  if (!result.ok) throw new Error(`switchView(${view}): ${result.err}`);
  await browser.waitUntil(
    async () =>
      await browser.execute(
        (v) =>
          document
            .querySelector('[data-testid="cull-page"]')
            ?.getAttribute("data-view") === v,
        view,
      ),
    { timeout: 5000, timeoutMsg: `cull-page never switched to ${view} view` },
  );
  await waitForQuiescence();
}

/** Force-reload settings from the DB. Needed after a fixture seed that
 * changed `onboarded_*` flags — the React app's settingsStore was
 * populated at mount time and won't notice DB rewrites otherwise. */
export async function reloadSettings() {
  const result = await browser.execute(async () => {
    try {
      const store = window.__PHOTOSIFT__?.settingsStore;
      if (!store) return { ok: false, err: "__PHOTOSIFT__.settingsStore unavailable" };
      await store.getState().loadSettings();
      return { ok: true };
    } catch (e) {
      return { ok: false, err: String(e?.message ?? e) };
    }
  });
  if (!result.ok) throw new Error(`reloadSettings: ${result.err}`);
}

/** Click the element matching the given data-testid. Convenience wrapper
 * because `$('[data-testid=X]')` syntax differs slightly per WDIO version. */
export async function clickTestId(testId) {
  const el = await browser.$(`[data-testid="${testId}"]`);
  await el.click();
  await waitForQuiescence();
}
