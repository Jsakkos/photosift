import { clearMocks } from "@tauri-apps/api/mocks";
import { useProjectStore } from "../stores/projectStore";

afterEach(() => {
  clearMocks();
  useProjectStore.setState({
    currentShoot: null,
    images: [],
    currentIndex: 0,
    isLoading: false,
    showMetadata: false,
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
    comparisonPinnedId: null,
    comparisonCyclingId: null,
    comparisonGroupMembers: [],
  });
});
