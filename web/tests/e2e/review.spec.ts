import fs from "node:fs/promises";

import type { Page } from "@playwright/test";

import type { OutcomeResponse, ShootReview } from "../../src/api/types";
import { expect, test } from "./fixtures";
import { FORTY, importShoot } from "./review-helpers";

async function openReview(page: Page, shootId: number): Promise<void> {
  await page.goto(`/review/${shootId}`);
  await expect(page.locator(".tile").first()).toBeVisible();
}

async function focusedId(page: Page): Promise<number> {
  const tile = page.locator('.tile[data-focused="true"]');
  await expect(tile).toHaveCount(1);
  return Number(await tile.getAttribute("data-photo-id"));
}

function chip(page: Page, label: string, count: number) {
  return page.getByRole("button", { name: new RegExp(`^${label.replace("?", "\\?")} ${count}$`) });
}

async function nextOutcomeResponse(page: Page): Promise<OutcomeResponse> {
  const res = await page.waitForResponse(
    (r) => r.url().includes("/outcome") && r.request().method() === "POST",
  );
  return (await res.json()) as OutcomeResponse;
}

async function sidecarRating(photo: OutcomeResponse): Promise<string> {
  const image = photo.raw_path ?? photo.jpeg_path;
  if (!image) throw new Error("photo has no file");
  const xmp = image.replace(/\.[^.\\/]+$/, ".xmp");
  const text = await fs.readFile(xmp, "utf8");
  const m = /Rating(?:="|>)(\d)/.exec(text);
  if (!m) throw new Error(`no rating in ${xmp}:\n${text}`);
  return m[1] ?? "";
}

test.describe("review screen", () => {
  let shootId: number;

  test.beforeEach(async ({ server }) => {
    shootId = await importShoot(server, FORTY);
  });

  test("shows every photo whole at its own aspect ratio", async ({ page }) => {
    await openReview(page, shootId);
    const review = (await (
      await page.request.get(`/api/shoots/${shootId}/review`)
    ).json()) as ShootReview;
    expect(review.moments).toHaveLength(3);
    await expect(page.locator(".moment")).toHaveCount(3);
    if (review.highlights.length > 0) {
      await expect(page.locator(".highlights img").first()).toBeVisible();
    }

    const imgs = page.locator(".highlights img, .moment img");
    const count = await imgs.count();
    expect(count).toBeGreaterThanOrEqual(40);
    let sawPortrait = false;
    for (let i = 0; i < count; i += 1) {
      const img = imgs.nth(i);
      await img.scrollIntoViewIfNeeded();
      await expect
        .poll(() => img.evaluate((el: HTMLImageElement) => el.naturalWidth), { timeout: 15_000 })
        .toBeGreaterThan(0);
      const m = await img.evaluate((el: HTMLImageElement) => {
        const r = el.getBoundingClientRect();
        return { w: r.width, h: r.height, nw: el.naturalWidth, nh: el.naturalHeight };
      });
      const rendered = m.w / m.h;
      const natural = m.nw / m.nh;
      expect(Math.abs(rendered / natural - 1), `img ${i}`).toBeLessThan(0.02);
      if (natural < 1) sawPortrait = true;
    }
    expect(sawPortrait).toBe(true);
  });

  test("A sets album, survives a reload and writes the sidecar", async ({ page }) => {
    await openReview(page, shootId);
    await expect(chip(page, "Album", 0)).toBeVisible();
    const id = await focusedId(page);

    const response = nextOutcomeResponse(page);
    await page.keyboard.press("a");
    await expect(chip(page, "Album", 1)).toBeVisible();
    const saved = await response;
    expect(saved.id).toBe(id);
    expect(saved.outcome).toBe("album");
    expect(await sidecarRating(saved)).toBe("4");

    await page.reload();
    await expect(page.locator(`.tile[data-photo-id="${id}"]`)).toHaveAttribute(
      "data-outcome",
      "album",
    );
    await expect(chip(page, "Album", 1)).toBeVisible();
  });

  test("the loupe steps with the arrows and A there persists too", async ({ page }) => {
    await openReview(page, shootId);
    const first = await focusedId(page);
    await page.keyboard.press("Enter");
    const loupe = page.getByTestId("loupe");
    await expect(loupe).toBeVisible();
    await expect(page.getByTestId("loupe-image")).toHaveAttribute("data-photo-id", String(first));

    await page.keyboard.press("ArrowRight");
    await expect(page.getByTestId("loupe-image")).not.toHaveAttribute(
      "data-photo-id",
      String(first),
    );
    const second = Number(await page.getByTestId("loupe-image").getAttribute("data-photo-id"));

    const response = nextOutcomeResponse(page);
    await page.keyboard.press("a");
    const saved = await response;
    expect(saved.id).toBe(second);
    expect(saved.outcome).toBe("album");

    await page.keyboard.press("Escape");
    await expect(loupe).toHaveCount(0);
    await expect(page.locator(`.tile[data-photo-id="${second}"]`)).toHaveAttribute(
      "data-outcome",
      "album",
    );
    await page.reload();
    await expect(page.locator(`.tile[data-photo-id="${second}"]`)).toHaveAttribute(
      "data-outcome",
      "album",
    );
  });

  test("X marks two for deletion and the Recycle Bin batch removes them", async ({ page }) => {
    await openReview(page, shootId);
    const a = await focusedId(page);
    await page.keyboard.press("x");
    await expect(chip(page, "Delete?", 1)).toBeVisible();
    await page.keyboard.press("ArrowRight");
    const b = await focusedId(page);
    expect(b).not.toBe(a);
    await page.keyboard.press("x");
    await expect(chip(page, "Delete?", 2)).toBeVisible();

    const trash = page.getByRole("button", { name: /Move to Recycle Bin/ });
    await expect(trash).toContainText("Delete? (2)");
    await trash.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toContainText("2 photos");
    await expect(dialog).toContainText("Lightroom");
    await expect(dialog).toContainText("Delete Rejected Photos");
    await dialog.getByRole("button", { name: /Recycle Bin/ }).click();

    await expect(page.locator(`.tile[data-photo-id="${a}"]`)).toHaveCount(0);
    await expect(page.locator(`.tile[data-photo-id="${b}"]`)).toHaveCount(0);
    await expect(page.locator(".tile")).toHaveCount(38);
    await expect(chip(page, "Delete?", 0)).toBeVisible();
    await expect(page.getByRole("button", { name: /Move to Recycle Bin/ })).toContainText(
      "Delete? (0)",
    );
    await expect(page.getByRole("button", { name: /Move to Recycle Bin/ })).toBeDisabled();
  });

  test("C lays the current burst out side by side", async ({ page }) => {
    await openReview(page, shootId);
    const big = page.locator('.tile[data-burst-size="3"][data-large="true"]').first();
    await big.click();
    const id = Number(await big.getAttribute("data-photo-id"));
    expect(await focusedId(page)).toBe(id);

    await page.keyboard.press("c");
    const frames = page.getByTestId("compare").locator(".compare-frame");
    await expect(frames).toHaveCount(3);
    const boxes = await frames.evaluateAll((els) =>
      els.map((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height };
      }),
    );
    expect(new Set(boxes.map((b) => Math.round(b.y))).size).toBe(1);
    expect(boxes[0]!.x + boxes[0]!.w).toBeLessThanOrEqual(boxes[1]!.x + 1);
    expect(boxes[1]!.x + boxes[1]!.w).toBeLessThanOrEqual(boxes[2]!.x + 1);

    await page.keyboard.press("Escape");
    await expect(page.getByTestId("compare")).toHaveCount(0);
  });

  test("the last focused photo comes back after a reload", async ({ page }) => {
    await openReview(page, shootId);
    for (let i = 0; i < 11; i += 1) await page.keyboard.press("ArrowRight");
    const twelfth = await focusedId(page);
    const order = await page
      .locator(".tile")
      .evaluateAll((els) => els.map((el) => Number(el.getAttribute("data-photo-id"))));
    expect(order.indexOf(twelfth)).toBe(11);

    await page.reload();
    await expect(page.locator(".tile").first()).toBeVisible();
    expect(await focusedId(page)).toBe(twelfth);
  });

  test("a filter chip narrows the screen and the number key clears it", async ({ page }) => {
    await openReview(page, shootId);
    await page.keyboard.press("a");
    await expect(chip(page, "Album", 1)).toBeVisible();
    await page.keyboard.press("1");
    await expect(page.locator(".tile")).toHaveCount(1);
    await page.keyboard.press("1");
    await expect(page.locator(".tile")).toHaveCount(40);
  });

  test("? lists the bindings from the one table", async ({ page }) => {
    await openReview(page, shootId);
    await page.keyboard.press("?");
    const help = page.getByRole("dialog", { name: /Keyboard/ });
    await expect(help).toContainText("Toggle blooper");
    await expect(help).toContainText("Compare this burst");
    await page.keyboard.press("Escape");
    await expect(help).toHaveCount(0);
  });
});
