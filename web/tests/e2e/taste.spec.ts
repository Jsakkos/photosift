import { expect, test, type ClaudeScript, type TestServer } from "./fixtures";
import { burstCard, importShoot } from "./review-helpers";

const TWELVE = burstCard([
  [1, 1, 1, 1, 1, 1],
  [1, 1, 1, 1, 1, 1],
]);

const BACKGROUND: ClaudeScript = {
  auth: "claude.ai",
  calls: [
    {
      tool: "propose",
      args: {
        run_id: "$RUN",
        items: Array.from({ length: 12 }, (_, i) => ({
          photo_id: i + 1,
          outcome: "album",
          reason: `Keeper ${i + 1}`,
        })),
      },
    },
    { tool: "finish_run", args: { run_id: "$RUN", summary: "Twelve frames." } },
  ],
  result: { is_error: false, subtype: "success", result: "Done." },
  exit: 0,
  delay_ms: 0,
};

const CHAT_TASTE: ClaudeScript = {
  auth: "claude.ai",
  calls: [{ tool: "set_taste_profile", args: { text: "Prefers candid laughs." } }],
  reply: "Noted.",
  result: { is_error: false, subtype: "success", result: "Done." },
  exit: 0,
  delay_ms: 0,
};

/** The helper's MCP tool, called directly: what a chat turn does. */
async function helperSetsTaste(server: TestServer, text: string): Promise<void> {
  const res = await fetch(`${server.baseUrl}/mcp`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { name: "set_taste_profile", arguments: { text } },
    }),
  });
  expect(res.status).toBe(200);
}

test.describe("taste profile", () => {
  test.use({ claudeScript: BACKGROUND });
  let shootId: number;

  test.beforeEach(async ({ server }) => {
    shootId = await importShoot(server, TWELVE);
  });

  test("a note autosaves after a pause and survives a reload", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("link", { name: "Taste" }).click();
    await expect(page).toHaveURL(/\/taste$/);
    await expect(
      page.getByText("The helper reads these notes and your last 40 disagreements before every run."),
    ).toBeVisible();

    const box = page.getByRole("textbox", { name: /what you keep/i });
    await expect(box).toBeEnabled();
    const saved = page.waitForResponse(
      (r) => r.url().endsWith("/api/taste") && r.request().method() === "PUT",
    );
    await box.fill("I keep candid laughs.");
    await expect(page.getByTestId("taste-save-state")).toContainText("Unsaved changes");
    await saved;
    await expect(page.getByTestId("taste-save-state")).toContainText("Saved");

    await page.reload();
    await expect(box).toHaveValue("I keep candid laughs.");
  });

  test("lists an override with its reason and criterion", async ({ page }) => {
    await page.goto(`/review/${shootId}`);
    const tile = page.locator('.tile[data-photo-id="2"]');
    await expect(tile.locator(".badge-suggest")).toBeVisible();
    await tile.locator(".frame").click();
    const answered = page.waitForResponse(
      (r) => r.url().includes("/api/photos/2/outcome") && r.request().method() === "POST",
    );
    await page.keyboard.press("l");
    await answered;
    const why = tile.getByRole("group", { name: /why did you choose differently/i });
    const noted = page.waitForResponse((r) => r.url().includes("/api/photos/2/decision-note"));
    await why.getByRole("button", { name: "expression" }).click();
    await noted;

    await page.goto("/taste");
    const row = page.locator(".taste-decision");
    await expect(row).toHaveCount(1);
    await expect(row).toContainText("Suggested Album because");
    await expect(row).toContainText("Keeper 2");
    await expect(row).toContainText("you chose Library");
    await expect(row).toContainText("Because of expression");
    await expect(row.locator("img")).toHaveAttribute("src", "/media/thumb/2");
    await expect(row.locator("img")).toBeVisible();
  });

  test("a helper-written profile shows a notice that saving clears", async ({
    server,
    page,
  }) => {
    await helperSetsTaste(server, "Helper wrote this.");
    await page.goto("/taste");
    const notice = page.getByTestId("taste-helper-notice");
    await expect(notice).toContainText("The helper changed these notes on");
    await expect(notice).toContainText("check they still say what you want.");
    const box = page.getByRole("textbox", { name: /what you keep/i });
    await expect(box).toHaveValue("Helper wrote this.");

    const saved = page.waitForResponse(
      (r) => r.url().endsWith("/api/taste") && r.request().method() === "PUT",
    );
    await box.fill("Helper wrote this, and I agree.");
    await saved;
    await expect(notice).toHaveCount(0);

    await page.reload();
    await expect(box).toHaveValue("Helper wrote this, and I agree.");
    await expect(page.getByTestId("taste-helper-notice")).toHaveCount(0);
  });

  test("a chat turn that changes the notes leaves a note linking to the Taste page", async ({
    server,
    page,
  }) => {
    await page.goto(`/review/${shootId}`);
    await expect(page.locator(".tile").first()).toBeVisible();
    await server.setClaudeScript(CHAT_TASTE);
    await page.keyboard.press("h");
    const drawer = page.getByRole("complementary", { name: "Helper chat" });
    const input = drawer.getByRole("textbox");
    await input.fill("remember that I like candid laughs");
    await input.press("Enter");

    await expect(drawer.getByText("Noted.")).toBeVisible();
    const note = drawer.getByText("Updated your taste notes", { exact: false });
    await expect(note).toBeVisible();
    await note.getByRole("link", { name: "Taste page" }).click();
    await expect(page).toHaveURL(/\/taste$/);
    await expect(page.getByRole("textbox", { name: /what you keep/i })).toHaveValue(
      "Prefers candid laughs.",
    );
    await expect(page.getByTestId("taste-helper-notice")).toBeVisible();
  });
});
