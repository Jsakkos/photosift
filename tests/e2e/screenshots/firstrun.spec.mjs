import {
  startSession,
  seedFixture,
  navigateTo,
  waitForQuiescence,
  waitForTestId,
  switchView,
  reloadSettings,
} from "../helpers/screenshot-helpers.mjs";
import * as F from "../helpers/fixtures.mjs";

describe("First-run modal screenshots", () => {
  beforeEach(async () => {
    await startSession();
  });

  it("firstrun-triage", async () => {
    const seed = await seedFixture(F.triageColdNotOnboarded());
    await reloadSettings();
    await navigateTo(`/shoots/${seed.shootIds[0]}`, "cull-page");
    await waitForTestId("first-run-modal");
    await waitForQuiescence();
    await browser.checkScreen("firstrun-triage");
  });

  it("firstrun-select", async () => {
    const seed = await seedFixture(F.selectFirstRunNotOnboarded());
    await reloadSettings();
    await navigateTo(`/shoots/${seed.shootIds[0]}`, "cull-page");
    await waitForQuiescence();
    await switchView("select");
    await waitForTestId("first-run-modal");
    await waitForQuiescence();
    await browser.checkScreen("firstrun-select");
  });

  it("firstrun-route", async () => {
    const seed = await seedFixture(F.routeFirstRunNotOnboarded());
    await reloadSettings();
    await navigateTo(`/shoots/${seed.shootIds[0]}`, "cull-page");
    await waitForQuiescence();
    await switchView("route");
    await waitForTestId("first-run-modal");
    await waitForQuiescence();
    await browser.checkScreen("firstrun-route");
  });

  it("onboarding-wizard", async () => {
    await seedFixture(F.wizardFirstRun());
    await reloadSettings();
    await navigateTo("/shoots", "onboarding-wizard");
    await waitForQuiescence();
    await browser.checkScreen("onboarding-wizard");
  });
});
