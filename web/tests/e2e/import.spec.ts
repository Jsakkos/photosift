import fs from "node:fs/promises";
import path from "node:path";

import type { Page } from "@playwright/test";

import type { CardScan, CardScanSummary } from "../../src/api/types";
import { expect, test, type CardFile } from "./fixtures";

const CAMERA = "TestCam One";

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** `count` frames one minute apart starting at `hour:00` on `date` (YYYY-MM-DD). */
function burst(prefix: string, date: string, hour: number, count: number, camera = CAMERA): CardFile[] {
  return Array.from({ length: count }, (_, i) => ({
    name: `${prefix}${pad(i)}.jpg`,
    time: `${date}T${pad(hour)}:${pad(i)}:00`,
    camera,
  }));
}

/** 30 photos on 2026-10-18 and 5 on 2026-10-19. */
function twoDayCard(): CardFile[] {
  return [...burst("A", "2026-10-18", 10, 30), ...burst("B", "2026-10-19", 9, 5)];
}

function summaryOf(scan: CardScan): CardScanSummary {
  return {
    scan_id: scan.scan_id,
    source: scan.source,
    photo_count: scan.items.filter((i) => i.kind === "photo").length,
    video_count: scan.items.filter((i) => i.kind === "video").length,
    already_imported: scan.already_imported,
    cameras: scan.cameras,
    events: scan.events.map((e) => ({
      start: e.start,
      end: e.end,
      count: e.item_idxs.length,
      kind: e.kind,
    })),
  };
}

/** The server only announces real removable drives, which a test machine has none of. */
async function announceCards(page: Page, cards: CardScanSummary[]): Promise<void> {
  await page.route("**/api/cards", (route) => route.fulfill({ json: cards }));
}

async function dirNames(dir: string): Promise<string[]> {
  return (await fs.readdir(dir)).sort();
}

test("two days become a named event and an everyday one, then import into the library", async ({
  page,
  server,
}) => {
  const card = await server.makeCard(twoDayCard());
  const scan = await server.scanCard(card);

  await page.goto(`/import/${scan.scan_id}`);

  const events = page.getByTestId("event");
  await expect(events).toHaveCount(2);
  await expect(events.nth(0).getByRole("radio", { name: "Named" })).toBeChecked();
  await expect(events.nth(1).getByRole("radio", { name: "Everyday" })).toBeChecked();
  await expect(events.nth(0)).toContainText("30 photos");
  await expect(events.nth(1)).toContainText("5 photos");
  // Everyday events are filed per month, so they take no name.
  await expect(events.nth(1).getByLabel("Event name")).toHaveCount(0);

  await events.nth(0).getByLabel("Event name").fill("Baby's First Meal");
  const folder = events.nth(0).getByTestId("folder-path");
  await expect(folder).toContainText("2026-10-18_Babys-First-Meal");
  await expect(folder).toContainText(server.libraryRoot);
  await expect(events.nth(1).getByTestId("folder-path")).toContainText("2026-10_Everyday");

  await page.getByRole("button", { name: /^Import 35 photos/ }).click();

  await expect(page.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  const reviewLinks = page.getByRole("link", { name: /^Review/ });
  await expect(reviewLinks).toHaveCount(2);
  for (const href of await reviewLinks.evaluateAll((els) => els.map((e) => e.getAttribute("href")))) {
    expect(href).toMatch(/^\/review\/\d+$/);
  }
  await expect(page.getByTestId("import-result")).toContainText("Copied 35");

  const named = path.join(server.libraryRoot, "2026", "2026-10-18_Babys-First-Meal");
  const everyday = path.join(server.libraryRoot, "2026", "2026-10_Everyday");
  expect(await dirNames(named)).toHaveLength(30);
  expect(await dirNames(everyday)).toHaveLength(5);
});

test("a camera's clock offset regroups the events live", async ({ page, server }) => {
  // CamB's clock is 3 hours behind: its 07:00 frames were really shot at 10:00.
  const card = await server.makeCard([
    ...burst("A", "2026-10-18", 10, 10, "CamA"),
    ...burst("B", "2026-10-18", 7, 10, "CamB"),
  ]);
  const scan = await server.scanCard(card);

  await page.goto(`/import/${scan.scan_id}`);

  const events = page.getByTestId("event");
  await expect(events).toHaveCount(2);
  const cameras = page.getByTestId("camera");
  await expect(cameras).toHaveCount(2);
  await expect(cameras.filter({ hasText: "CamB" })).toContainText("10 photos");

  await page.getByLabel("Clock offset for CamB (minutes)").fill("180");

  await expect(events).toHaveCount(1);
  await expect(events.first()).toContainText("20 photos");
  await expect(events.first()).toContainText("2026-10-18 10:00 to 10:09");
});

test("names survive a regroup", async ({ page, server }) => {
  const card = await server.makeCard([
    ...burst("A", "2026-10-18", 10, 30, "CamA"),
    ...burst("B", "2026-10-18", 7, 30, "CamB"),
  ]);
  const scan = await server.scanCard(card);
  await page.goto(`/import/${scan.scan_id}`);
  const events = page.getByTestId("event");
  await expect(events).toHaveCount(2);

  // The morning event (CamB at 07:00) comes first.
  await events.nth(0).getByLabel("Event name").fill("Hike");
  await page.getByLabel("Clock offset for CamA (minutes)").fill("-1");

  await expect(events.nth(0).getByLabel("Event name")).toHaveValue("Hike");
});

test("a card imported twice announces what is already in the library and copies nothing", async ({
  page,
  server,
}) => {
  const card = await server.makeCard(twoDayCard());
  const first = await server.scanCard(card);
  await page.goto(`/import/${first.scan_id}`);
  await page.getByRole("button", { name: /^Import 35 photos/ }).click();
  await expect(page.getByTestId("import-result")).toContainText("Copied 35");

  const second = await server.scanCard(card);
  expect(second.already_imported).toBe(35);
  await announceCards(page, [summaryOf(second)]);
  await page.goto("/");

  const banner = page.getByTestId("card-banner");
  await expect(banner).toContainText("SD card: 35 photos, 2 days. Import?");
  await expect(banner).toContainText("35 already imported");

  await banner.getByRole("link", { name: "Import?" }).click();
  await expect(page).toHaveURL(new RegExp(`/import/${second.scan_id}$`));
  await expect(page.getByTestId("event")).toHaveCount(2);
  // The page says so itself, and the banner is not repeated on the card's own page.
  await expect(page.getByTestId("card-banner")).toHaveCount(0);
  await expect(page.getByText("35 already imported")).toBeVisible();

  await page.getByRole("button", { name: /^Import 35 photos/ }).click();

  const result = page.getByTestId("import-result");
  await expect(result).toContainText("Copied 0");
  await expect(result).toContainText("35 already in the library");
});

test("events can be merged and split by hand", async ({ page, server }) => {
  const card = await server.makeCard(twoDayCard());
  const scan = await server.scanCard(card);
  await page.goto(`/import/${scan.scan_id}`);
  const events = page.getByTestId("event");
  await expect(events).toHaveCount(2);

  await events.nth(0).getByRole("button", { name: "Merge with next" }).click();
  await expect(events).toHaveCount(1);
  await expect(events.first()).toContainText("35 photos");
  await expect(events.first().getByRole("button", { name: "Merge with next" })).toHaveCount(0);

  await events.first().getByRole("button", { name: /^Show all/ }).click();
  await events.first().getByRole("button", { name: "Split before photo 31" }).click();

  await expect(events).toHaveCount(2);
  await expect(events.nth(0)).toContainText("30 photos");
  await expect(events.nth(1)).toContainText("5 photos");
});

test("scan thumbnails are shown whole, never cropped", async ({ page, server }) => {
  const card = await server.makeCard([
    { name: "wide.jpg", time: "2026-10-18T10:00:00", camera: CAMERA, w: 800, h: 400 },
    { name: "tall.jpg", time: "2026-10-18T10:01:00", camera: CAMERA, w: 400, h: 800 },
  ]);
  const scan = await server.scanCard(card);
  await page.goto(`/import/${scan.scan_id}`);

  const thumbs = page.getByTestId("event").first().locator("img");
  await expect(thumbs).toHaveCount(2);
  await expect.poll(() => thumbs.evaluateAll((els) => els.every((e) => (e as HTMLImageElement).complete))).toBe(true);
  const ratios = await thumbs.evaluateAll((els) =>
    els.map((e) => {
      const img = e as HTMLImageElement;
      const box = img.getBoundingClientRect();
      return {
        natural: img.naturalWidth / img.naturalHeight,
        shown: box.width / box.height,
        fit: getComputedStyle(img).objectFit,
      };
    }),
  );
  expect(ratios).toHaveLength(2);
  for (const r of ratios) {
    expect(r.shown).toBeCloseTo(r.natural, 1);
    expect(r.fit).not.toBe("cover");
  }
  expect(ratios[0]?.natural).toBeGreaterThan(1);
  expect(ratios[1]?.natural).toBeLessThan(1);
});

test("Enter in a name field does not start the import", async ({ page, server }) => {
  const card = await server.makeCard(burst("A", "2026-10-18", 10, 30));
  const scan = await server.scanCard(card);
  await page.goto(`/import/${scan.scan_id}`);

  const name = page.getByTestId("event").first().getByLabel("Event name");
  await name.fill("Hike");
  await name.press("Enter");

  await expect(page.getByRole("progressbar")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Import 30 photos/ })).toBeEnabled();
  expect(await fs.readdir(server.libraryRoot)).toEqual([]);
});

test("an everyday event spanning two months shows the server's refusal", async ({
  page,
  server,
}) => {
  const card = await server.makeCard([
    { name: "a.jpg", time: "2026-10-31T23:30:00", camera: CAMERA },
    { name: "b.jpg", time: "2026-11-02T09:00:00", camera: CAMERA },
  ]);
  const scan = await server.scanCard(card);
  await page.goto(`/import/${scan.scan_id}`);
  await expect(page.getByTestId("event")).toHaveCount(2);

  await page.getByTestId("event").nth(0).getByRole("button", { name: "Merge with next" }).click();
  await expect(page.getByTestId("event")).toHaveCount(1);
  await page.getByRole("radio", { name: "Everyday" }).check();
  await page.getByRole("button", { name: /^Import 2 photos/ }).click();

  await expect(page.getByRole("alert")).toContainText("Everyday");
  await expect(page.getByRole("alert")).toContainText("month");
  await expect(page.getByRole("button", { name: /^Import 2 photos/ })).toBeEnabled();
});

test("an evicted scan can be rescanned from its announced card", async ({ page, server }) => {
  const card = await server.makeCard(burst("A", "2026-10-18", 10, 30));
  const live = await server.scanCard(card);
  const gone = { ...summaryOf(live), scan_id: "evicted-scan" };
  await announceCards(page, [gone]);

  await page.goto("/import/evicted-scan");
  await expect(page.getByText("no longer available")).toBeVisible();
  await page.getByRole("button", { name: "Rescan card" }).click();

  await expect(page).toHaveURL(/\/import\/(?!evicted-scan)[\w-]+$/);
  await expect(page.getByTestId("event")).toHaveCount(1);
});

test("an unknown scan with no announced card explains itself", async ({ page }) => {
  await page.goto("/import/nope");

  await expect(page.getByText("no longer available")).toBeVisible();
  await expect(page.getByRole("button", { name: "Rescan card" })).toHaveCount(0);
});

test("the banner catches up with a card announced while the socket was down", async ({
  page,
  server,
}) => {
  const card = await server.makeCard(burst("A", "2026-10-18", 10, 3));
  const summary = summaryOf(await server.scanCard(card));
  let announced: CardScanSummary[] = [];
  await page.route("**/api/cards", (route) => route.fulfill({ json: announced }));
  // A socket the test controls: it opens, says nothing, and can be dropped.
  const sockets: { close(): Promise<void> }[] = [];
  await page.routeWebSocket("**/ws", (ws) => {
    sockets.push(ws);
  });

  await page.goto("/");
  await expect(page.getByText("No shoots yet")).toBeVisible();
  await expect(page.getByTestId("card-banner")).toHaveCount(0);

  announced = [summary];
  await sockets[0]?.close();

  // The client reconnects with backoff and fetches the cards again on open.
  await expect(page.getByTestId("card-banner")).toContainText("SD card: 3 photos, 1 day. Import?");
});

test("Import waits for a pending clock-offset regroup", async ({ page, server }) => {
  const card = await server.makeCard([
    ...burst("A", "2026-10-18", 10, 10, "CamA"),
    ...burst("B", "2026-10-18", 7, 10, "CamB"),
  ]);
  const scan = await server.scanCard(card);
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => (release = resolve));
  await page.route("**/api/scan/*/events", async (route) => {
    await gate;
    await route.continue();
  });
  await page.goto(`/import/${scan.scan_id}`);
  const importButton = page.getByRole("button", { name: /^Import 20 photos/ });
  await expect(page.getByTestId("event")).toHaveCount(2);
  await expect(importButton).toBeEnabled();

  await page.getByLabel("Clock offset for CamB (minutes)").fill("180");

  // The old grouping is still on screen, so the offsets must not be imported with it.
  await expect(importButton).toBeDisabled();
  await expect(page.getByTestId("event")).toHaveCount(2);
  release();
  await expect(page.getByTestId("event")).toHaveCount(1);
  await expect(importButton).toBeEnabled();
});

test("Import stays disabled while the regroup has failed", async ({ page, server }) => {
  const card = await server.makeCard([
    ...burst("A", "2026-10-18", 10, 10, "CamA"),
    ...burst("B", "2026-10-18", 7, 10, "CamB"),
  ]);
  const scan = await server.scanCard(card);
  let failing = true;
  await page.route("**/api/scan/*/events", (route) =>
    failing
      ? route.fulfill({ status: 500, json: { error: "boom" } })
      : route.continue(),
  );
  await page.goto(`/import/${scan.scan_id}`);
  const importButton = page.getByRole("button", { name: /^Import 20 photos/ });
  await expect(page.getByTestId("event")).toHaveCount(2);

  const offset = page.getByLabel("Clock offset for CamB (minutes)");
  await offset.fill("180");
  await expect(page.getByRole("alert")).toContainText("boom");
  await expect(importButton).toBeDisabled();

  // Editing again retries, and a success clears the error.
  failing = false;
  await offset.fill("181");
  await expect(page.getByTestId("event")).toHaveCount(1);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(importButton).toBeEnabled();
});

test("a missed import_done is recovered when the socket reconnects", async ({ page, server }) => {
  const card = await server.makeCard(twoDayCard());
  const scan = await server.scanCard(card);
  let dropDone = true;
  const sockets: { close(): Promise<void> }[] = [];
  await page.routeWebSocket("**/ws", (ws) => {
    const upstream = ws.connectToServer();
    upstream.onMessage((message) => {
      if (dropDone && typeof message === "string" && message.includes('"import_done"')) return;
      ws.send(message);
    });
    ws.onMessage((message) => upstream.send(message));
    sockets.push(ws);
  });
  await page.goto(`/import/${scan.scan_id}`);
  await expect(page.getByTestId("event")).toHaveCount(2);

  await page.getByRole("button", { name: /^Import 35 photos/ }).click();
  // The server finishes, but the page never hears about it.
  await expect
    .poll(async () => (await (await page.request.get("/api/import/current")).json()).job_id)
    .toBeNull();
  await page.waitForTimeout(300);
  await expect(page.getByTestId("import-result")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Import 35 photos/ })).toBeDisabled();

  dropDone = false;
  // Every hook on the page has its own socket; all of them drop and reconnect.
  await Promise.all(sockets.map((socket) => socket.close()));

  await expect(page.getByTestId("import-result")).toContainText("Copied 35");
  await expect(page.getByRole("link", { name: /^Review/ })).toHaveCount(2);
  await expect(page.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
});
