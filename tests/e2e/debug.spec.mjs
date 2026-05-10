// Debug-only: dump app state so we can see what tauri-driver is seeing.
import { browser } from "@wdio/globals";

describe("debug", () => {
  it("dumps state and window handles", async () => {
    await new Promise((r) => setTimeout(r, 3000));
    // Try navigating to a few likely Tauri webview URLs.
    for (const u of ["tauri://localhost/", "https://tauri.localhost/", "http://tauri.localhost/"]) {
      try {
        await browser.url(u);
        const after = await browser.execute(() => ({ url: location.href, len: (document.body?.innerText || "").length }));
        console.log(`[debug] tried ${u} → ${JSON.stringify(after)}`);
      } catch (e) {
        console.log(`[debug] tried ${u} → err ${String(e).slice(0, 200)}`);
      }
    }
    const handles = await browser.getWindowHandles();
    console.log("[debug] handles:", JSON.stringify(handles));
    const dumps = [];
    for (const h of handles) {
      try {
        await browser.switchToWindow(h);
        const dump = await browser.execute(() => ({
          url: location.href,
          title: document.title,
          bodyTextLen: (document.body?.innerText || "").length,
          bodyTextSample: (document.body?.innerText || "").slice(0, 200),
          hasTauri: !!window.__TAURI__,
          rootChildren: document.getElementById("root")?.childElementCount ?? -1,
        }));
        dumps.push({ handle: h, dump });
      } catch (e) {
        dumps.push({ handle: h, err: String(e) });
      }
    }
    console.log("[debug-dumps]", JSON.stringify(dumps, null, 2));
  });
});
