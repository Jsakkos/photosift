import fs from "node:fs/promises";
import path from "node:path";

import type { OutcomeResponse } from "../../src/api/types";
import { expect, test } from "./fixtures";

test("a library folder opens in place: culled where it is, nothing copied", async ({
  page,
  server,
}) => {
  // A shoot folder as Lightroom names it (YYYY-MM_, not PhotoSift's YYYY-MM-DD_).
  const card = await server.makeCard([
    { name: "DSC_0001.jpg", time: "2024-05-04T10:00:00", camera: "TestCam One" },
    { name: "DSC_0002.jpg", time: "2024-05-04T10:01:00", camera: "TestCam One" },
    { name: "DSC_0003.jpg", time: "2024-05-04T10:02:00", camera: "TestCam One", w: 480, h: 640 },
  ]);
  const folder = path.join(server.libraryRoot, "2024", "2024-05_Trip");
  await fs.mkdir(path.dirname(folder), { recursive: true });
  await fs.rename(card, folder);
  const before = (await fs.readdir(server.libraryRoot, { recursive: true })).sort();

  await page.goto("/");
  await expect(page.getByText("No shoots yet")).toBeVisible();
  const open = page.getByRole("button", { name: "Open in place" });
  await expect(open).toBeDisabled();
  await page.getByLabel("Folder or card to import").fill(folder);
  await open.click();

  await expect(page).toHaveURL(/\/review\/\d+$/);
  const reviewUrl = page.url();
  await expect(page.locator(".tile")).toHaveCount(3);
  expect((await fs.readdir(server.libraryRoot, { recursive: true })).sort()).toEqual(before);

  const response = page.waitForResponse(
    (r) => r.url().includes("/outcome") && r.request().method() === "POST",
  );
  await page.keyboard.press("a");
  const saved = (await response.then((r) => r.json())) as OutcomeResponse;
  const image = saved.jpeg_path ?? "";
  expect(path.dirname(image).toLowerCase()).toBe(folder.toLowerCase());
  const sidecar = image.replace(/\.[^.\\/]+$/, ".xmp");
  expect(await fs.readFile(sidecar, "utf8")).toMatch(/Rating(?:="|>)4/);
  expect((await fs.readdir(folder)).sort()).toEqual(
    ["DSC_0001.jpg", "DSC_0002.jpg", "DSC_0003.jpg", path.basename(sidecar)].sort(),
  );

  // Opening it again lands on the same shoot; the list has just the one.
  await page.goto("/");
  await expect(page.getByRole("link", { name: /2024-05_Trip/ })).toHaveCount(1);
  await page.getByLabel("Folder or card to import").fill(folder);
  await page.getByRole("button", { name: "Open in place" }).click();
  await expect(page).toHaveURL(reviewUrl);
});

test("a folder outside the library is refused on the Shoots page", async ({ page, server }) => {
  const card = await server.makeCard([
    { name: "A.jpg", time: "2024-07-04T09:00:00", camera: "TestCam One" },
  ]);
  await page.goto("/");

  await page.getByLabel("Folder or card to import").fill(card);
  await page.getByRole("button", { name: "Open in place" }).click();

  await expect(
    page.getByRole("alert").filter({ hasText: "Could not open" }).filter({ hasText: "inside the library" }),
  ).toBeVisible();
  await expect(page).toHaveURL(/\/$/);
  expect(await fs.readdir(card)).toEqual(["A.jpg"]);
});
