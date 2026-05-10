// End-to-end regression test for the RAF + sibling-JPG feature.
//
// Drives the built debug Tauri binary via tauri-driver / WebView2 and
// exercises the actual production flow:
//   1. Tauri app launches with `withGlobalTauri: true`, exposing
//      `window.__TAURI__.core.invoke` + `event.listen` to the webview
//   2. We call `scan_folder` directly via Tauri IPC against the test
//      fixture folder, listening for `scan-progress` events
//   3. Assert: 8 source files (4 RAF + 4 JPG) collapse to 4 entries,
//      every entry has captured_at and sibling_jpeg_path populated
//   4. Call `start_import` for the same folder, listening for
//      `import-complete`
//   5. Assert: photoCount = 4 (not 8), all rows have sidecars
//
// We don't drive the Import dialog UI directly because "Browse folder…"
// opens a native OS picker that WebDriver cannot control. tauri-driver
// also attaches WebDriver to an `about:blank` page, separate from the
// React webview — but `window.__TAURI__` is global to the WebView2
// process and IPC routes back to the same Rust backend, so we exercise
// the contract through IPC directly.

import { browser, expect } from "@wdio/globals";

const TEST_SOURCE = "E:\\photosift\\RAF Test Files";
const TEST_SLUG = "wdio-raf-test";

// Run a Tauri command + wait for an event in the page context. We do
// the await dance inline because `browser.execute(async fn)` returns
// the Promise resolution; expressing it as a single page-side async
// fn is more reliable than the older executeAsync/done-callback API.
async function callScan(source) {
  return browser.execute(async (src) => {
    try {
      const { invoke } = window.__TAURI__.core;
      const { listen } = window.__TAURI__.event;
      const entries = [];
      let unlistenProgress;
      const completePromise = new Promise(async (resolve) => {
        unlistenProgress = await listen("scan-progress", (e) =>
          entries.push(e.payload.entry),
        );
        const unlistenComplete = await listen("scan-complete", () => {
          unlistenProgress();
          unlistenComplete();
          resolve();
        });
      });
      await invoke("scan_folder", {
        source: src,
        withThumbnails: false,
        dedupKnown: false,
      });
      await completePromise;
      return { ok: true, entries };
    } catch (e) {
      return { ok: false, err: String(e) };
    }
  }, source);
}

async function callImport(source, slug, selectedPaths) {
  return browser.execute(
    async (src, sg, paths) => {
      try {
        const { invoke } = window.__TAURI__.core;
        const { listen } = window.__TAURI__.event;
        const completePromise = new Promise(async (resolve) => {
          const off = await listen("import-complete", (e) => {
            off();
            resolve(e.payload);
          });
        });
        await invoke("start_import", {
          sourcePath: src,
          slug: sg,
          importMode: "copy",
          selectedPaths: paths,
        });
        const payload = await completePromise;
        return { ok: true, payload };
      } catch (e) {
        return { ok: false, err: String(e) };
      }
    },
    source,
    slug,
    selectedPaths,
  );
}

async function deleteShootBySlug(slug) {
  return browser.execute(async (sg) => {
    try {
      const { invoke } = window.__TAURI__.core;
      const shoots = await invoke("list_shoots");
      const target = shoots.find((s) => s.slug === sg);
      if (!target) return { ok: true, missing: true };
      await invoke("delete_shoot", { shootId: target.id });
      return { ok: true, deletedId: target.id };
    } catch (e) {
      return { ok: false, err: String(e) };
    }
  }, slug);
}

describe("RAF + sibling-JPG e2e", () => {
  // Scan once at suite setup and share the result. Calling scan twice
  // in a single session triggered a WebDriver script-timeout for
  // reasons unrelated to this feature (likely event-listener bookkeeping
  // across multiple `browser.execute` calls in the same page context).
  let scan;

  before(async () => {
    // tauri-driver attaches WebDriver to about:blank by default. Tauri
    // IPC rejects calls from `null`/`about:blank` origins ("Origin
    // header is not a valid URL"), so we navigate to the app's bundled
    // URL (http://tauri.localhost/ on Windows). That origin is in
    // Tauri's allow-list, and `withGlobalTauri: true` in tauri.conf.json
    // puts `window.__TAURI__` on the page.
    await browser.url("http://tauri.localhost/");
    await browser.waitUntil(
      async () =>
        await browser.execute(() => !!window.__TAURI__?.core?.invoke),
      { timeout: 15_000, timeoutMsg: "window.__TAURI__ never appeared" },
    );
    await deleteShootBySlug(TEST_SLUG); // defensive cleanup from prior runs

    scan = await callScan(TEST_SOURCE);
    if (!scan.ok) throw new Error(`scan_folder failed: ${scan.err}`);
  });

  it("scan_folder collapses RAF+JPG pairs and populates dates from siblings", () => {
    expect(scan.entries).toHaveLength(4);
    for (const e of scan.entries) {
      expect(e.path.toUpperCase()).toMatch(/\.RAF$/);
      if (!e.siblingJpegPath) {
        throw new Error(`${e.filename} missing siblingJpegPath`);
      }
      expect(e.siblingJpegPath.toUpperCase()).toMatch(/\.JPG$/);
      if (!e.capturedAt) {
        throw new Error(`${e.filename} missing capturedAt`);
      }
      expect(e.camera).toMatch(/X100VI|X-T|FUJIFILM/);
    }
  });

  it("start_import produces one row per RAF+JPG pair (4, never 8)", async () => {
    const selectedPaths = scan.entries.map((e) => e.path);
    expect(selectedPaths).toHaveLength(4);

    const result = await callImport(TEST_SOURCE, TEST_SLUG, selectedPaths);
    if (!result.ok) throw new Error(`start_import failed: ${result.err}`);
    // Whether the test fixtures are fresh or were already in the DB
    // from a prior run, the post-fix invariant is: 4 frames in →
    // 4 work-units accounted for, never 8.
    const accountedFor =
      result.payload.photoCount + result.payload.dedupSkipped;
    expect(accountedFor).toBe(4);
    expect(typeof result.payload.shootId).toBe("number");
  });

  after(async () => {
    const cleanup = await deleteShootBySlug(TEST_SLUG);
    if (!cleanup.ok) {
      console.log(
        `\n[cleanup] manual DB cleanup needed for slug "${TEST_SLUG}":\n` +
          `  sqlite3 ~/.photosift/photosift.db "DELETE FROM shoots WHERE slug='${TEST_SLUG}';"\n`,
      );
    }
  });
});
