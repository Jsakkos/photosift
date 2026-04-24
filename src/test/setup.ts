import { clearMocks } from "@tauri-apps/api/mocks";
import { useProjectStore } from "../stores/projectStore";

afterEach(() => {
  clearMocks();
  useProjectStore.setState({
    currentShoot: null,
    images: [],
    currentIndex: 0,
    isLoading: false,
    showShortcutHints: false,
    autoAdvance: true,
    isZoomed: false,
    undoStack: [],
    redoStack: [],
    currentView: "triage",
    viewMode: "sequential",
    groups: [],
    displayItems: [],
    lastFlagAction: null,
    selectBracket: null,
    selectBracketSuppressedForGroup: null,
    selectVisitedAtFloor: new Set<number>(),
  });
});
