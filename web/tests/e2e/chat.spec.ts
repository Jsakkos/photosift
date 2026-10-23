import type { Page } from "@playwright/test";

import { expect, test, type ClaudeScript } from "./fixtures";
import { burstCard, importShoot } from "./review-helpers";

// 12 single-frame bursts; a fresh database numbers the photos 1..12 in capture order.
const TWELVE = burstCard([
  [1, 1, 1, 1, 1, 1],
  [1, 1, 1, 1, 1, 1],
]);

// The automatic run after the import: photo 3 starts out suggested for the archive.
const BACKGROUND: ClaudeScript = {
  auth: "claude.ai",
  calls: [
    {
      tool: "propose",
      args: {
        run_id: "$RUN",
        items: Array.from({ length: 12 }, (_, i) => ({
          photo_id: i + 1,
          outcome: "archive",
          reason: `Soft frame ${i + 1}`,
        })),
      },
    },
    { tool: "finish_run", args: { run_id: "$RUN", summary: "Twelve frames." } },
  ],
  result: { is_error: false, subtype: "success", result: "Done." },
  exit: 0,
  delay_ms: 0,
};

// What the helper does when the user chats: one proposal and a reply.
const CHAT: ClaudeScript = {
  auth: "claude.ai",
  calls: [
    {
      tool: "propose",
      args: {
        run_id: "$RUN",
        items: [{ photo_id: 3, outcome: "album", reason: "You asked for this one" }],
      },
    },
  ],
  reply: "Photo 3 is now suggested for the album.",
  result: { is_error: false, subtype: "success", result: "Done." },
  exit: 0,
  delay_ms: 0,
};

function tile(page: Page, id: number) {
  return page.locator(`.tile[data-photo-id="${id}"]`);
}

test.describe("helper chat", () => {
  test.use({ claudeScript: BACKGROUND });
  let shootId: number;

  test.beforeEach(async ({ server, page }) => {
    shootId = await importShoot(server, TWELVE);
    await page.goto(`/review/${shootId}`);
    await expect(tile(page, 3).locator(".badge-suggest")).toHaveText("suggest: Archive");
    await server.setClaudeScript(CHAT);
  });

  test("H opens the drawer; a message changes a suggestion and gets a reply", async ({ page }) => {
    const drawer = page.getByRole("complementary", { name: "Helper chat" });
    await expect(drawer).toBeHidden();
    await page.keyboard.press("h");
    await expect(drawer).toBeVisible();
    const box = drawer.getByRole("textbox");
    await expect(box).toBeFocused();

    await box.fill("make the third one album");
    await box.press("Enter");

    await expect(drawer.getByText("Photo 3 is now suggested for the album.")).toBeVisible();
    await expect(drawer.getByText("make the third one album")).toBeVisible();
    await expect(box).toHaveValue("");
    await expect(tile(page, 3).locator(".badge-suggest")).toHaveText("suggest: Album");
    // A suggestion is not an outcome.
    await expect(tile(page, 3).locator(".badge-album")).toHaveCount(0);

    // The conversation survives a reload.
    await page.reload();
    await expect(tile(page, 1)).toBeVisible();
    await page.keyboard.press("h");
    await expect(drawer.getByText("make the third one album")).toBeVisible();
    await expect(drawer.getByText("Photo 3 is now suggested for the album.")).toBeVisible();
  });

  test("review keys are inactive while typing; Esc blurs, then closes", async ({ page }) => {
    const outcomes: string[] = [];
    page.on("request", (r) => {
      if (r.url().includes("/outcome")) outcomes.push(r.url());
    });
    await tile(page, 1).locator(".frame").click();
    await page.getByRole("button", { name: "Helper", exact: true }).click();
    const drawer = page.getByRole("complementary", { name: "Helper chat" });
    const box = drawer.getByRole("textbox");
    await expect(box).toBeFocused();

    await page.keyboard.press("a");
    await page.keyboard.press("h");
    await expect(box).toHaveValue("ah");
    await page.keyboard.press("Shift+Enter");
    await page.keyboard.type("b");
    await expect(box).toHaveValue("ah\nb");
    expect(outcomes).toEqual([]);
    await expect(tile(page, 1).locator(".badge-album")).toHaveCount(0);

    await page.keyboard.press("Escape");
    await expect(box).not.toBeFocused();
    await expect(drawer).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(drawer).toBeHidden();
    // Still on the review: Esc did not leave the shoot.
    await expect(page).toHaveURL(new RegExp(`/review/${shootId}$`));

    // With the drawer closed the review keys work again.
    const saved = page.waitForResponse((r) => r.url().includes("/outcome"));
    await page.keyboard.press("a");
    await saved;
    await expect(tile(page, 1).locator(".badge-album")).toBeVisible();
  });
});
