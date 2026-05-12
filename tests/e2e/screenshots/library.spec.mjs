import {
  startSession,
  seedFixture,
  setScreenshotState,
  navigateTo,
  waitForQuiescence,
  waitForTestId,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Library screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  it("library-empty", async () => {
    await seedFixture(F.libraryEmpty());
    await navigateTo("/shoots", "shoot-list-page");
    await waitForQuiescence();
    await browser.checkScreen("library-empty");
  });

  it("library-one-shoot", async () => {
    await seedFixture(F.libraryOneShoot());
    await navigateTo("/shoots", "shoot-list-page");
    await waitForQuiescence();
    await browser.checkScreen("library-one-shoot");
  });

  it("library-three-shoots", async () => {
    await seedFixture(F.libraryThreeShoots());
    await navigateTo("/shoots", "shoot-list-page");
    await waitForQuiescence();
    await browser.checkScreen("library-three-shoots");
  });

  it("library-curator-running", async () => {
    const seed = await seedFixture(F.libraryThreeShoots());
    const runningShootId = seed.shootIds[0];
    await setScreenshotState({
      curatorStatus: "running",
      curatorRunningShootId: { set: runningShootId },
      curatorProgress: { processed: 3, failed: 0, total: 8 },
    });
    await navigateTo("/shoots", "shoot-list-page");
    await waitForQuiescence();
    await browser.checkScreen("library-curator-running");
  });
});
