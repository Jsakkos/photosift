import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  pressKeys,
  switchView,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Select screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  async function openShootInSelect(seedFn) {
    const seed = await seedFixture(seedFn());
    const shootId = seed.shootIds[0];
    await navigateTo(`/shoots/${shootId}`, "cull-page");
    await waitForQuiescence();
    await switchView("select");
    return shootId;
  }

  it("select-1up", async () => {
    await openShootInSelect(F.selectBasic);
    await browser.checkScreen("select-1up");
  });

  it("select-2up", async () => {
    await openShootInSelect(F.selectBasic);
    // 'Tab' enters bracket / 2-up mode.
    await pressKeys(["Tab"]);
    await browser.checkScreen("select-2up");
  });

  it("select-detail-rail", async () => {
    await openShootInSelect(F.selectBasicWithAi);
    // 'F' opens the DetailRail.
    await pressKeys(["f"]);
    await browser.checkScreen("select-detail-rail");
  });

  it("select-tier-0", async () => {
    await openShootInSelect(F.selectMixedRatings);
    await browser.checkScreen("select-tier-0");
  });

  it("select-tier-1", async () => {
    await openShootInSelect(F.selectMixedRatings);
    await pressKeys(["]"]);
    await browser.checkScreen("select-tier-1");
  });

  it("select-tier-2", async () => {
    await openShootInSelect(F.selectMixedRatings);
    await pressKeys(["]", "]"]);
    await browser.checkScreen("select-tier-2");
  });

  it("select-tier-3", async () => {
    await openShootInSelect(F.selectMixedRatings);
    await pressKeys(["]", "]", "]"]);
    await browser.checkScreen("select-tier-3");
  });

  it("select-tier-4", async () => {
    await openShootInSelect(F.selectMixedRatings);
    await pressKeys(["]", "]", "]", "]"]);
    await browser.checkScreen("select-tier-4");
  });

  it("select-empty-no-picks", async () => {
    await openShootInSelect(F.triageCompleteAllRejects);
    await browser.checkScreen("select-empty-no-picks");
  });

  it("select-empty-all-routed", async () => {
    await openShootInSelect(F.selectAllRouted);
    await browser.checkScreen("select-empty-all-routed");
  });

  it("select-empty-floor-too-high", async () => {
    await openShootInSelect(F.selectMixedRatings);
    // Bump past the max rating in the fixture (max is 5/2 = 2 from selectMixedRatings).
    await pressKeys(["]", "]", "]", "]", "]"]);
    await browser.checkScreen("select-empty-floor-too-high");
  });
});
