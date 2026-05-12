import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  waitForTestId,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Import dialog screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  // The dialog's internal state (`importing`, `error`, `source`,
  // selected paths) is React component-local, not in any Zustand store,
  // so `set_screenshot_state` can't reach it. For MVP we capture just
  // the initial open state. Variants that require driving the
  // import flow further are noted as follow-up work in #20.
  it("import-initial", async () => {
    await seedFixture(F.libraryEmpty());
    await navigateTo("/shoots", "shoot-list-page");
    await waitForQuiescence();

    // Click the "＋ Import" header button to open the dialog.
    await browser.execute(() => {
      const buttons = Array.from(document.querySelectorAll("button"));
      const btn = buttons.find((b) =>
        (b.textContent ?? "").includes("Import"),
      );
      btn?.click();
    });
    await waitForTestId("import-dialog");
    await waitForQuiescence();
    await browser.checkScreen("import-initial");
  });
});
