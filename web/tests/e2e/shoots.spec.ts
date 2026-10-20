import { expect, test } from "./fixtures";

test("an empty library says there are no shoots yet", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByText("No shoots yet")).toBeVisible();
});

test("a card imported through the API appears live as one shoot of 3 photos", async ({
  page,
  server,
}) => {
  await page.goto("/");
  await expect(page.getByText("No shoots yet")).toBeVisible();

  const card = await server.makeCard([
    { name: "IMG_0001.jpg", time: "2024-05-01T10:00:00", camera: "TestCam One" },
    { name: "IMG_0002.jpg", time: "2024-05-01T10:00:20", camera: "TestCam One" },
    { name: "IMG_0003.jpg", time: "2024-05-01T10:01:00", camera: "TestCam One", w: 480, h: 640 },
  ]);
  await server.importCard(card);

  // No reload: the page learns about the import from the websocket.
  await expect(page.getByText("3 photos")).toBeVisible();
  await expect(page.getByText("No shoots yet")).toHaveCount(0);
  await expect(page.getByRole("link", { name: /Everyday/ })).toHaveCount(1);
});

test("a reloaded page lists the imported shoot", async ({ page, server }) => {
  const card = await server.makeCard([
    { name: "A.jpg", time: "2024-06-02T09:00:00", camera: "TestCam One" },
    { name: "B.jpg", time: "2024-06-02T09:05:00", camera: "TestCam One" },
    { name: "C.jpg", time: "2024-06-02T09:10:00", camera: "TestCam One" },
  ]);
  await server.importCard(card);
  await expect
    .poll(async () => (await (await page.request.get("/api/shoots")).json()).length)
    .toBe(1);

  await page.goto("/");

  await expect(page.getByText("3 photos")).toBeVisible();
});

test("a folder can be imported by hand from the Shoots page", async ({ page, server }) => {
  const card = await server.makeCard([
    { name: "A.jpg", time: "2024-07-04T09:00:00", camera: "TestCam One" },
    { name: "B.jpg", time: "2024-07-04T09:01:00", camera: "TestCam One" },
    { name: "C.jpg", time: "2024-07-04T09:02:00", camera: "TestCam One" },
  ]);
  await page.goto("/");
  await expect(page.getByText("No shoots yet")).toBeVisible();

  const submit = page.getByRole("button", { name: "Import from folder…" });
  await expect(submit).toBeDisabled();
  await page.getByLabel("Folder or card to import").fill(card);
  await submit.click();

  await expect(page).toHaveURL(/\/import\/[0-9a-f]+$/);
  await page.getByRole("button", { name: /^Import 3 photos/ }).click();
  await expect(page.getByTestId("import-result")).toContainText("Copied 3");

  await page.goto("/");
  await expect(page.getByText("3 photos")).toBeVisible();
});

test("a folder that cannot be scanned is reported on the Shoots page", async ({ page, server }) => {
  await page.goto("/");

  await page.getByLabel("Folder or card to import").fill(`${server.home}\no-such-folder`);
  await page.getByRole("button", { name: "Import from folder…" }).click();

  await expect(page.getByRole("alert").filter({ hasText: "Could not scan" })).toBeVisible();
  await expect(page).toHaveURL(/\/$/);
});

test("a card the server could not read is flagged, with a way to import by hand", async ({ page }) => {
  // The watcher only sees real removable drives; stand in for its event.
  await page.routeWebSocket(/\/ws$/, (ws) => {
    ws.send(JSON.stringify({ type: "card_scan_failed", source: "F:\\", error: "unreadable card" }));
  });
  await page.goto("/");

  const banner = page.getByTestId("card-banner-error");
  await expect(banner).toContainText("Could not read the card in F:\\: unreadable card");
  await expect(banner.getByRole("link", { name: /from a folder/ })).toBeVisible();
  await expect(page.getByLabel("Folder or card to import")).toBeVisible();
});
