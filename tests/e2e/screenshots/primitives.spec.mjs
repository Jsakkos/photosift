import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  waitForTestId,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Primitives showcase screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  it("primitives-triage-tab", async () => {
    await seedFixture(F.libraryEmpty());
    await navigateTo("/primitives", "primitives-page");
    await waitForQuiescence();
    await browser.checkScreen("primitives-triage-tab");
  });

  it("primitives-select-tab", async () => {
    await seedFixture(F.libraryEmpty());
    await navigateTo("/primitives", "primitives-page");
    await waitForQuiescence();
    // Click the "Select" tab button.
    await browser.execute(() => {
      const buttons = Array.from(document.querySelectorAll("button"));
      const btn = buttons.find(
        (b) => (b.textContent ?? "").trim().toLowerCase() === "select",
      );
      btn?.click();
    });
    await waitForQuiescence();
    await browser.checkScreen("primitives-select-tab");
  });

  it("primitives-route-tab", async () => {
    await seedFixture(F.libraryEmpty());
    await navigateTo("/primitives", "primitives-page");
    await waitForQuiescence();
    await browser.execute(() => {
      const buttons = Array.from(document.querySelectorAll("button"));
      const btn = buttons.find(
        (b) => (b.textContent ?? "").trim().toLowerCase() === "route",
      );
      btn?.click();
    });
    await waitForQuiescence();
    await browser.checkScreen("primitives-route-tab");
  });
});
