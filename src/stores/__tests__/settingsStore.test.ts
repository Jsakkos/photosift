import { useSettingsStore } from "../settingsStore";
import { setupMockIpc } from "../../test/mockIpc";

beforeEach(() => {
  useSettingsStore.setState({
    settings: {
      nearDupThreshold: 4,
      relatedThreshold: 12,
      groupTimeWindowS: 60,
      selectRequiresPick: true,
      routeMinStar: 3,
      libraryRoot: null,
      enableAiOnImport: true,
      hideSoftThreshold: 30,
      eyeOpenConfidence: 0.7,
    },
    isLoaded: false,
    isOpen: false,
  });
});

describe("settingsStore", () => {
  test("loadSettings pulls from IPC", async () => {
    setupMockIpc({
      get_settings: {
        nearDupThreshold: 2,
        relatedThreshold: 8,
      },
    });

    await useSettingsStore.getState().loadSettings();
    const s = useSettingsStore.getState();

    expect(s.settings.nearDupThreshold).toBe(2);
    expect(s.settings.relatedThreshold).toBe(8);
    expect(s.isLoaded).toBe(true);
  });

  test("updateSettings merges partial + persists via IPC", async () => {
    const spy = vi.fn();
    setupMockIpc({}, spy);

    await useSettingsStore.getState().updateSettings({ nearDupThreshold: 6 });

    const s = useSettingsStore.getState();
    expect(s.settings.nearDupThreshold).toBe(6);
    expect(s.settings.relatedThreshold).toBe(12); // unchanged

    const call = spy.mock.calls.find((c) => c[0] === "update_settings");
    expect(call).toBeDefined();
    expect((call![1] as { settings: { nearDupThreshold: number } }).settings.nearDupThreshold).toBe(6);
  });

  test("openDialog / closeDialog flip isOpen", () => {
    expect(useSettingsStore.getState().isOpen).toBe(false);
    useSettingsStore.getState().openDialog();
    expect(useSettingsStore.getState().isOpen).toBe(true);
    useSettingsStore.getState().closeDialog();
    expect(useSettingsStore.getState().isOpen).toBe(false);
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
});
