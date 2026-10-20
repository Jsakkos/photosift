import { expect, test } from "./fixtures";
import { burstCard, importShoot } from "./review-helpers";

// Playwright's default headless shell composites in software, which alone costs
// ~2 frames at 1080p; the full Chromium ("new headless") paints like a user's
// browser, which is what the spike's 13 ms figure was measured on.
test.use({ channel: "chromium" });

// Shared CI runners are slower and noisier than the desktop these budgets were set on, so
// there only the functional assertions run; the timings are still measured and logged.
const enforceTimings = !process.env.CI;

test("a 400-photo shoot: the thumbnail grid loads under a second and the loupe paints fast", async ({
  page,
  server,
}) => {
  test.setTimeout(900_000);
  await page.setViewportSize({ width: 1920, height: 1080 });
  // 4 moments, 25 bursts each, 4 frames per burst, at 1200 px so previews stay real.
  const bursts = Array.from({ length: 4 }, () => Array.from({ length: 25 }, () => 4));
  const shootId = await importShoot(server, burstCard(bursts, { size: [1200, 800] }));

  // Warm the server's own caches so this measures the page, not first renders.
  await page.request.get(`/api/shoots/${shootId}/review`);

  const started = Date.now();
  await page.goto(`/review/${shootId}`);
  await expect(page.locator(".tile").first()).toBeVisible();
  // Lazy strips only load near the viewport; force every thumbnail so the whole
  // 400-photo grid is what gets timed.
  const loaded = await page.evaluate(async () => {
    const t0 = performance.now();
    const imgs = Array.from(document.querySelectorAll<HTMLImageElement>(".tile img"));
    imgs.forEach((i) => (i.loading = "eager"));
    await Promise.all(
      imgs.map((i) =>
        i.complete && i.naturalWidth > 0
          ? Promise.resolve()
          : new Promise<void>((resolve, reject) => {
              i.addEventListener("load", () => resolve(), { once: true });
              i.addEventListener("error", () => reject(new Error(`thumb failed: ${i.src}`)), {
                once: true,
              });
            }),
      ),
    );
    return { count: imgs.length, ms: performance.now() - t0 };
  });
  const sinceNavigation = Date.now() - started;
  console.log(`grid: ${loaded.count} thumbs, forced load ${loaded.ms.toFixed(0)} ms, navigation to all loaded ${sinceNavigation} ms`);
  expect(loaded.count).toBe(400);
  if (enforceTimings) expect(sinceNavigation).toBeLessThan(1000);

  // Loupe: 50 presses of -> with a 300 ms think time between them.
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("loupe-image")).toBeVisible();
  await page.waitForTimeout(1000);
  const latency = await page.evaluate(async () => {
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    const frames = () =>
      new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
    const loupeImg = () => document.querySelector<HTMLImageElement>('[data-testid="loupe-image"]');
    const changed = (before: string | null) =>
      new Promise<void>((resolve) => {
        const root = document.querySelector('[data-testid="loupe"]');
        if (!root) return resolve();
        const done = () => loupeImg()?.getAttribute("data-photo-id") !== before;
        if (done()) return resolve();
        const mo = new MutationObserver(() => {
          if (done()) {
            mo.disconnect();
            resolve();
          }
        });
        mo.observe(root, { subtree: true, attributes: true, childList: true });
      });
    const lat: number[] = [];
    for (let k = 0; k < 50; k += 1) {
      await sleep(300);
      const before = loupeImg()?.getAttribute("data-photo-id") ?? null;
      const t0 = performance.now();
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
      await changed(before);
      await loupeImg()?.decode();
      await frames();
      lat.push(performance.now() - t0);
    }
    // The floor: two animation frames with nothing to paint, same cadence.
    const floor: number[] = [];
    for (let k = 0; k < 50; k += 1) {
      await sleep(300);
      const t0 = performance.now();
      await frames();
      floor.push(performance.now() - t0);
    }
    const q = (xs: number[], p: number) => {
      const s = [...xs].sort((a, b) => a - b);
      return s[Math.min(s.length - 1, Math.floor(p * s.length))]!;
    };
    return { p50: q(lat, 0.5), p90: q(lat, 0.9), max: q(lat, 1), floorP50: q(floor, 0.5) };
  });
  console.log(
    `loupe: p50 ${latency.p50.toFixed(1)} ms, p90 ${latency.p90.toFixed(1)} ms, max ${latency.max.toFixed(1)} ms (two empty frames alone: p50 ${latency.floorP50.toFixed(1)} ms)`,
  );
  if (enforceTimings) expect(latency.p50).toBeLessThan(30);
});
