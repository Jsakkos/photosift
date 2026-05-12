import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  pressKeys,
  waitForTestId,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Triage screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  async function openShootInTriage(seedFn) {
    const seed = await seedFixture(seedFn());
    const shootId = seed.shootIds[0];
    // Wait for the CullPage shell regardless of which sub-view renders
    // (Triage with photos = TriageShell; Triage with 0 unreviewed =
    // EmptyViewState). Either way `cull-page` exists.
    await navigateTo(`/shoots/${shootId}`, "cull-page");
    await waitForQuiescence();
    return shootId;
  }

  it("triage-cold", async () => {
    await openShootInTriage(F.triageCold);
    await browser.checkScreen("triage-cold");
  });

  it("triage-mid-pass", async () => {
    await openShootInTriage(F.triageCold);
    // P (pick), Space (next), X (reject), Space (next), P (pick)
    await pressKeys(["p", "Space", "x", "Space", "p"]);
    await browser.checkScreen("triage-mid-pass");
  });

  it("triage-all-strip-on", async () => {
    await openShootInTriage(F.triageCold);
    // 'A' toggles the AllStrip per useKeyboardNav.
    await pressKeys(["a"]);
    await browser.checkScreen("triage-all-strip-on");
  });

  it("triage-faces-rail-on", async () => {
    await openShootInTriage(F.triageColdWithFaces);
    // 'F' toggles the FacesRail.
    await pressKeys(["f"]);
    await browser.checkScreen("triage-faces-rail-on");
  });

  it("triage-heatmap-on", async () => {
    await openShootInTriage(F.triageColdWithSharpness);
    // 'H' toggles the sharpness heatmap.
    await pressKeys(["h"]);
    await browser.checkScreen("triage-heatmap-on");
  });

  it("triage-ai-rejects-filter", async () => {
    await openShootInTriage(F.triageWithAiJudgments);
    // The toggle is in the top bar; click via text match.
    await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll("button"));
      const btn = btns.find((b) =>
        /AI rejects|Only AI/i.test(b.textContent ?? ""),
      );
      btn?.click();
    });
    await waitForQuiescence();
    await browser.checkScreen("triage-ai-rejects-filter");
  });

  it("triage-complete", async () => {
    await openShootInTriage(F.triageComplete);
    await browser.checkScreen("triage-complete");
  });
});
