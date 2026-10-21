import fs from "node:fs/promises";

import type { Locator, Page } from "@playwright/test";

import type { OutcomeResponse } from "../../src/api/types";
import { expect, test, type ClaudeScript } from "./fixtures";
import { burstCard, importShoot } from "./review-helpers";

// 12 single-frame bursts in two moments of six; a fresh database numbers the
// photos 1..12 in capture order, which the scripts below rely on.
const TWELVE = burstCard([
  [1, 1, 1, 1, 1, 1],
  [1, 1, 1, 1, 1, 1],
]);

const SUMMARY = "A calm morning with three clear keepers.";

function proposal(photoId: number, outcome: string, reason: string) {
  return { photo_id: photoId, outcome, reason };
}

function items(): ReturnType<typeof proposal>[] {
  const out = [
    proposal(1, "album", "Big laugh, eyes open"),
    proposal(2, "album", "Whole family in frame"),
    proposal(3, "album", "Sharpest of the cake moment"),
  ];
  for (let id = 4; id <= 11; id += 1) out.push(proposal(id, "archive", `Soft frame ${id}`));
  out.push(proposal(12, "delete", "Lens cap in the way"));
  return out;
}

function finish(summary = SUMMARY) {
  return { tool: "finish_run", args: { run_id: "$RUN", summary } };
}

function success(extra: Record<string, unknown> = {}): ClaudeScript {
  return {
    auth: "claude.ai",
    calls: [
      { tool: "propose", args: { run_id: "$RUN", items: items() } },
      {
        tool: "propose_your_call",
        args: {
          run_id: "$RUN",
          photo_a: 4,
          photo_b: 5,
          tradeoff: "4 has the better smile; 5 is sharper",
        },
      },
      finish(),
    ],
    result: { is_error: false, subtype: "success", result: "Done." },
    exit: 0,
    delay_ms: 0,
    ...extra,
  };
}

async function openReview(page: Page, shootId: number): Promise<void> {
  await page.goto(`/review/${shootId}`);
  await expect(page.locator(".tile").first()).toBeVisible();
}

function tile(page: Page, id: number): Locator {
  return page.locator(`.tile[data-photo-id="${id}"]`);
}

function chip(page: Page, label: string, count: number): Locator {
  return page.getByRole("button", { name: new RegExp(`^${label.replace("?", "\\?")} ${count}$`) });
}

async function sidecarRating(photo: OutcomeResponse): Promise<string> {
  const image = photo.raw_path ?? photo.jpeg_path;
  if (!image) throw new Error("photo has no file");
  const text = await fs.readFile(image.replace(/\.[^.\\/]+$/, ".xmp"), "utf8");
  const m = /Rating(?:="|>)(\d)/.exec(text);
  if (!m) throw new Error(`no rating in the sidecar:\n${text}`);
  return m[1] ?? "";
}

async function press(page: Page, id: number, key: string): Promise<OutcomeResponse> {
  const answered = page.waitForResponse(
    (r) => r.url().includes(`/api/photos/${id}/outcome`) && r.request().method() === "POST",
  );
  await page.keyboard.press(key);
  return (await (await answered).json()) as OutcomeResponse;
}

test.describe("helper suggestions", () => {
  test.use({ claudeScript: success() });
  let shootId: number;

  test.beforeEach(async ({ server }) => {
    shootId = await importShoot(server, TWELVE);
  });

  test("shows the summary, the suggested counts and a reason on every large tile", async ({
    page,
  }) => {
    await openReview(page, shootId);
    await expect(page.getByTestId("helper-summary")).toHaveText(SUMMARY);
    await expect(page.getByTestId("helper-suggested")).toHaveText(
      "Suggested: 3 album · 8 archive · 1 delete?",
    );
    const large = page.locator('.tile[data-large="true"]');
    await expect(large).toHaveCount(10);
    for (let i = 0; i < 10; i += 1) {
      await expect(large.nth(i).locator('[data-slot="reason"]')).not.toHaveText("");
    }
    await expect(tile(page, 1).locator('[data-slot="reason"]')).toHaveText("Big laugh, eyes open");
    await expect(tile(page, 1).locator(".badge-suggest")).toHaveAttribute(
      "data-suggested",
      "album",
    );
    await expect(tile(page, 1).locator(".badge-suggest")).toHaveText("suggest: Album");
  });

  test("Y accepts the focused suggestion and writes the sidecar", async ({ page }) => {
    await openReview(page, shootId);
    await expect(tile(page, 1).locator(".badge-suggest")).toBeVisible();
    await tile(page, 1).locator(".frame").click();
    const saved = await press(page, 1, "y");
    expect(saved.outcome).toBe("album");
    await expect(tile(page, 1).locator(".badge-album")).toBeVisible();
    await expect(tile(page, 1).locator(".badge-suggest")).toHaveCount(0);
    await expect(chip(page, "Album", 1)).toBeVisible();
    // The suggested counts follow the decision at once.
    await expect(page.getByTestId("helper-suggested")).toHaveText(
      "Suggested: 2 album · 8 archive · 1 delete?",
    );
    expect(await sidecarRating(saved)).toBe("4");
  });

  test("deciding differently asks why, and the reason survives a reload", async ({ page }) => {
    await openReview(page, shootId);
    await tile(page, 2).locator(".frame").click();
    await expect(tile(page, 2).locator(".badge-suggest")).toBeVisible();
    await press(page, 2, "l");
    const why = tile(page, 2).getByRole("group", { name: /why did you choose differently/i });
    await expect(why).toBeVisible();
    await expect(tile(page, 2).locator('[data-slot="reason"] s')).toHaveText(
      "Whole family in frame",
    );
    // The chips never take the keyboard: the review keys still work afterwards.
    const noted = page.waitForResponse(
      (r) => r.url().includes("/api/photos/2/decision-note") && r.request().method() === "POST",
    );
    await why.getByRole("button", { name: "expression" }).click();
    expect((await noted).status()).toBe(204);
    await expect(why.getByRole("button", { name: "expression" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await page.keyboard.press("ArrowRight");
    await expect(tile(page, 2)).toHaveAttribute("data-focused", "false");

    await page.reload();
    await expect(tile(page, 2).locator(".badge-library")).toBeVisible();
    await expect(
      tile(page, 2)
        .getByRole("group", { name: /why did you choose differently/i })
        .getByRole("button", { name: "expression" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  test("Shift+Y accepts the rest of the moment and leaves the other moments alone", async ({
    page,
  }) => {
    await openReview(page, shootId);
    await tile(page, 1).locator(".frame").click();
    await expect(tile(page, 7).locator(".badge-suggest")).toBeVisible();
    await page.keyboard.press("Shift+Y");
    await expect(chip(page, "Album", 3)).toBeVisible();
    await expect(chip(page, "Archive", 3)).toBeVisible();
    await expect(tile(page, 6).locator(".badge-archive")).toBeVisible();
    await expect(tile(page, 7).locator(".badge-suggest")).toBeVisible();
  });

  test("Accept all asks first, names the counts and applies the suggestions", async ({ page }) => {
    await openReview(page, shootId);
    await page.getByRole("button", { name: /^Accept all suggestions \(12\)$/ }).click();
    const dialog = page.getByRole("dialog", { name: /accept all suggestions/i });
    await expect(dialog.getByTestId("accept-counts")).toHaveText("3 Album · 8 Archive · 1 Delete?");
    await dialog.getByRole("button", { name: "Not now" }).click();
    await expect(chip(page, "Album", 0)).toBeVisible();

    await page.getByRole("button", { name: /^Accept all suggestions/ }).click();
    await page.getByRole("button", { name: "Accept 12" }).click();
    await expect(chip(page, "Album", 3)).toBeVisible();
    await expect(chip(page, "Archive", 8)).toBeVisible();
    await expect(chip(page, "Delete?", 1)).toBeVisible();
    await expect(page.getByRole("button", { name: /^Accept all suggestions/ })).toHaveCount(0);
  });

  test("a your-call pair shows both frames whole, with the tradeoff", async ({ page }) => {
    await openReview(page, shootId);
    const pair = page.locator("[data-your-call-id]");
    await expect(pair).toHaveCount(1);
    await expect(pair).toContainText("4 has the better smile; 5 is sharper");
    await expect(pair.locator(".tile")).toHaveCount(2);
    // The pair sits inside its moment, above the burst rows.
    const moment = page.locator('.moment[data-moment="0"]');
    await expect(moment.locator("[data-your-call-id]")).toHaveCount(1);
    const pairBox = await pair.boundingBox();
    const rowBox = await tile(page, 1).boundingBox();
    expect(pairBox && rowBox && pairBox.y < rowBox.y).toBe(true);

    const imgs = pair.locator("img");
    for (let i = 0; i < 2; i += 1) {
      const img = imgs.nth(i);
      await expect
        .poll(() => img.evaluate((el: HTMLImageElement) => el.naturalWidth), { timeout: 15_000 })
        .toBeGreaterThan(0);
      const m = await img.evaluate((el: HTMLImageElement) => {
        const r = el.getBoundingClientRect();
        return { w: r.width, h: r.height, nw: el.naturalWidth, nh: el.naturalHeight };
      });
      expect(Math.abs(m.w / m.h / (m.nw / m.nh) - 1)).toBeLessThan(0.02);
    }

    // Arrows move between the two, and A on one sends the other to archive.
    await expect(pair.locator('.tile[data-focused="true"]')).toHaveCount(1);
    await pair.locator(".tile").first().locator(".frame").click();
    const left = Number(await pair.locator(".tile").first().getAttribute("data-photo-id"));
    const right = Number(await pair.locator(".tile").nth(1).getAttribute("data-photo-id"));
    await page.keyboard.press("ArrowRight");
    await expect(tile(page, right)).toHaveAttribute("data-focused", "true");
    await page.keyboard.press("ArrowLeft");
    await expect(tile(page, left)).toHaveAttribute("data-focused", "true");
    await press(page, left, "a");
    await expect(tile(page, left).locator(".badge-album")).toBeVisible();
    await expect(tile(page, right).locator(".badge-archive")).toBeVisible();
  });

  test("a pair's other frame keeps a decision the user already made", async ({ page }) => {
    await openReview(page, shootId);
    const pair = page.locator("[data-your-call-id]");
    const [left, right] = [4, 5];
    await tile(page, right).locator(".frame").click();
    await press(page, right, "l");
    await tile(page, left).locator(".frame").click();
    await press(page, left, "a");
    await expect(pair.locator(`.tile[data-photo-id="${left}"] .badge-album`)).toBeVisible();
    await expect(pair.locator(`.tile[data-photo-id="${right}"] .badge-library`)).toBeVisible();
  });
});

test.describe("helper failure", () => {
  test.use({
    claudeScript: {
      auth: "claude.ai",
      calls: [],
      retry_error: "rate_limit",
      result: { is_error: true, subtype: "error_during_execution", result: "API Error: rate limited" },
      exit: 1,
      delay_ms: 0,
    },
  });

  test("helper_failure_is_visible_and_retryable", async ({ page, server }) => {
    const shootId = await importShoot(server, TWELVE);
    await openReview(page, shootId);
    const status = page.locator(".helper-problem");
    await expect(status).toContainText("usage limit");
    await expect(page.getByTestId("helper-suggested")).toHaveCount(0);

    // Plan 1 keeps working while the helper is down.
    await tile(page, 1).locator(".frame").click();
    await press(page, 1, "a");
    await expect(tile(page, 1).locator(".badge-album")).toBeVisible();
    await expect(chip(page, "Album", 1)).toBeVisible();

    await server.setClaudeScript(success());
    await page.getByRole("button", { name: "Retry helper" }).click();
    await expect(page.getByTestId("helper-suggested")).toContainText("Suggested:");
    await expect(page.getByTestId("helper-summary")).toHaveText(SUMMARY);
    await expect(page.locator(".helper-problem")).toHaveCount(0);
    // The photo decided during the outage stays as decided.
    await expect(tile(page, 1).locator(".badge-album")).toBeVisible();
  });
});

test.describe("stopping a helper run", () => {
  test.use({ claudeScript: success({ delay_ms: 60000 }) });

  test("stop_kills_the_run_and_says_so", async ({ page, server }) => {
    const shootId = await importShoot(server, TWELVE);
    await openReview(page, shootId);
    await expect(page.getByText("Helper is reading the shoot…")).toBeVisible();
    const stopped = page.waitForResponse(
      (r) => r.url().includes(`/api/shoots/${shootId}/helper/cancel`),
    );
    await page.getByRole("button", { name: "Stop", exact: true }).click();
    expect((await stopped).status()).toBe(204);
    await expect(page.locator(".helper-problem")).toContainText("stopped by you");
    await expect(page.getByRole("button", { name: "Retry helper" })).toBeVisible();
    await expect(page.getByTestId("helper-suggested")).toHaveCount(0);
  });
});

test.describe("helper run in progress", () => {
  test.use({
    claudeScript: {
      auth: "claude.ai",
      calls: [
        {
          tool: "propose",
          args: { run_id: "$RUN", items: [proposal(1, "delete", "Blurred by a hand")] },
        },
        finish("One frame to look at."),
      ],
      result: { is_error: false, subtype: "success", result: "Done." },
      exit: 0,
      delay_ms: 4000,
    },
  });

  test("deciding_during_run_keeps_user_choice", async ({ page, server }) => {
    const shootId = await importShoot(server, TWELVE);
    await openReview(page, shootId);
    await expect(page.getByText("Helper is reading the shoot…")).toBeVisible();
    await tile(page, 1).locator(".frame").click();
    const saved = await press(page, 1, "a");
    expect(saved.outcome).toBe("album");

    await expect(page.getByTestId("helper-summary")).toHaveText("One frame to look at.");
    await expect(tile(page, 1).locator(".badge-album")).toBeVisible();
    await expect(tile(page, 1).locator('[data-slot="reason"] s')).toHaveText("Blurred by a hand");
    await expect(chip(page, "Album", 1)).toBeVisible();
    await expect(chip(page, "Delete?", 0)).toBeVisible();
  });
});
