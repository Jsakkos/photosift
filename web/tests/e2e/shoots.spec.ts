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
