import { useSettingsStore, DEFAULT_FOLDER_TEMPLATE } from "../settingsStore";
import { setupMockIpc } from "../../test/mockIpc";

beforeEach(() => {
  useSettingsStore.setState({
    settings: {
      groupThreshold: 12,
      groupTimeWindowS: 60,
      selectRequiresPick: true,
      routeMinStar: 3,
      libraryRoot: null,
      enableAiOnImport: true,
      hideSoftThreshold: 30,
      eyeOpenConfidence: 0.7,
      immichIngestPath: null,
      curatorDefaultRunOnImport: true,
      curatorModel: "claude-sonnet-4-6",
      curatorMaxCostPerShootCents: 500,
      curatorProvider: "anthropic",
      curatorModelAnthropic: "claude-sonnet-4-6",
      curatorModelGemini: "gemini-2.5-flash",
      curatorModelLocal: "",
      curatorLocalBaseUrl: "http://localhost:11434/v1",
      folderTemplate: DEFAULT_FOLDER_TEMPLATE,
      onboardedTriage: true,
      onboardedSelect: true,
      onboardedRoute: true,
      onboardedWizard: true,
      curatorTriageOnImport: true,
      onboardedReview: true,
    },
    isLoaded: false,
    isOpen: false,
    wizardReplay: false,
  });
});

describe("settingsStore", () => {
  test("loadSettings pulls from IPC", async () => {
    setupMockIpc({
      get_settings: {
        groupThreshold: 8,
      },
    });

    await useSettingsStore.getState().loadSettings();
    const s = useSettingsStore.getState();

    expect(s.settings.groupThreshold).toBe(8);
    expect(s.isLoaded).toBe(true);
  });

  test("updateSettings merges partial + persists via IPC", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    await useSettingsStore.getState().updateSettings({ groupThreshold: 20 });

    const s = useSettingsStore.getState();
    expect(s.settings.groupThreshold).toBe(20);
    expect(s.settings.groupTimeWindowS).toBe(60); // unchanged

    const call = spy.mock.calls.find((c) => c[0] === "update_settings");
    expect(call).toBeDefined();
    expect((call![1] as { settings: { groupThreshold: number } }).settings.groupThreshold).toBe(20);
  });

  test("openDialog / closeDialog flip isOpen", () => {
    expect(useSettingsStore.getState().isOpen).toBe(false);
    useSettingsStore.getState().openDialog();
    expect(useSettingsStore.getState().isOpen).toBe(true);
    useSettingsStore.getState().closeDialog();
    expect(useSettingsStore.getState().isOpen).toBe(false);
  });

  test("openWizardTour / closeWizardTour flip wizardReplay", () => {
    expect(useSettingsStore.getState().wizardReplay).toBe(false);
    useSettingsStore.getState().openWizardTour();
    expect(useSettingsStore.getState().wizardReplay).toBe(true);
    useSettingsStore.getState().closeWizardTour();
    expect(useSettingsStore.getState().wizardReplay).toBe(false);
  });

  test("reclusterShoot invokes recluster_shoot with shootId", async () => {
    const spy = vi.fn();
    setupMockIpc({ recluster_shoot: 3 }, spy);

    const count = await useSettingsStore.getState().reclusterShoot(42);

    expect(count).toBe(3);
    const call = spy.mock.calls.find((c) => c[0] === "recluster_shoot");
    expect(call).toBeDefined();
    expect((call![1] as { shootId: number }).shootId).toBe(42);
  });

  test("reclusterShootWith passes the explicit threshold through to IPC", async () => {
    const spy = vi.fn();
    setupMockIpc({ recluster_shoot_with: 7 }, spy);

    const count = await useSettingsStore
      .getState()
      .reclusterShootWith(42, 20, 60);

    expect(count).toBe(7);
    const call = spy.mock.calls.find((c) => c[0] === "recluster_shoot_with");
    expect(call).toBeDefined();
    expect(call![1]).toEqual({
      shootId: 42,
      threshold: 20,
      timeWindowS: 60,
    });
  });
});
