import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  switchView,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("Route screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  async function openShootInRoute(seedFn) {
    const seed = await seedFixture(seedFn());
    const shootId = seed.shootIds[0];
    await navigateTo(`/shoots/${shootId}`, "cull-page");
    await waitForQuiescence();
    await switchView("route");
    return shootId;
  }

  it("route-empty-gated-by-star", async () => {
    await openShootInRoute(F.routeLowRated);
    await browser.checkScreen("route-empty-gated-by-star");
  });

  it("route-grid-cold", async () => {
    await openShootInRoute(F.routeMixed);
    await browser.checkScreen("route-grid-cold");
  });

  it("route-mixed-destinations", async () => {
    await openShootInRoute(F.routeMixedWithDests);
    await browser.checkScreen("route-mixed-destinations");
  });

  it("route-ship-strip-c1", async () => {
    await openShootInRoute(F.routeAllEdit);
    await browser.checkScreen("route-ship-strip-c1");
  });

  it("route-ship-strip-export", async () => {
    await openShootInRoute(F.routeAllExport);
    await browser.checkScreen("route-ship-strip-export");
  });

  it("route-ship-strip-both", async () => {
    await openShootInRoute(F.routeMixedWithDests);
    await browser.checkScreen("route-ship-strip-both");
  });
});
