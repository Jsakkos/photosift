import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  waitForTestId,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Settings dialog screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  async function openSettings() {
    await seedFixture(F.libraryOneShoot());
    await navigateTo("/shoots", "shoot-list-page");
    await waitForQuiescence();
    // The "⚙ Settings" header button opens the dialog.
    await browser.execute(() => {
      const buttons = Array.from(document.querySelectorAll("button"));
      const btn = buttons.find((b) =>
        (b.textContent ?? "").includes("Settings"),
      );
      btn?.click();
    });
    await waitForTestId("settings-dialog");
    await waitForQuiescence();
  }

  it("settings-open", async () => {
    await openSettings();
    await browser.checkScreen("settings-open");
  });
});
