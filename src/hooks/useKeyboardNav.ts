import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";

export function useKeyboardNav() {
  const {
    navigateNext,
    navigatePrev,
    setRating,
    setFlag,
    setFlagNoAutoReject,
    setDestination,
    undo,
    redo,
    toggleMetadata,
    toggleShortcutHints,
    toggleZoom,
    advanceToNextUnreviewed,
    displayItems,
    currentView,
    currentIndex,
    viewMode,
    setCurrentIndex,
    setGroupCover,
    setViewMode,
    getGroupForCurrentItem,
    enterComparison,
    exitComparison,
    cycleComparison,
    comparisonQuickPick,
  } = useProjectStore();
  const openSettings = useSettingsStore((s) => s.openDialog);

  useEffect(() => {
    if (displayItems.length === 0) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

      const mode = viewMode as string;
      if (mode === "comparison") {
        switch (e.key) {
          case "ArrowRight":
            e.preventDefault();
            cycleComparison(1);
            return;
          case "ArrowLeft":
            e.preventDefault();
            cycleComparison(-1);
            return;
          case "1":
            comparisonQuickPick("left");
            return;
          case "2":
            comparisonQuickPick("right");
            return;
          case "Tab":
            if (e.shiftKey) {
              e.preventDefault();
              exitComparison();
            }
            return;
          case "Escape":
            e.preventDefault();
            exitComparison();
            return;
          case "z":
          case "Z":
            if (!e.ctrlKey) toggleZoom();
            return;
        }
        return;
      }

      if (mode === "grid") return;

      if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
        return;
      }
      if (e.ctrlKey && (e.key === "Z" || (e.shiftKey && e.key === "z"))) {
        e.preventDefault();
        redo();
        return;
      }

      if (e.ctrlKey && e.key === "o") {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("photosift:open-folder"));
        return;
      }

      if (e.key === ",") {
        e.preventDefault();
        openSettings();
        return;
      }

      switch (e.key) {
        case "ArrowRight":
          e.preventDefault();
          navigateNext();
          break;
        case "ArrowLeft":
          e.preventDefault();
          navigatePrev();
          break;
        case "ArrowDown":
          e.preventDefault();
          {
            const curItem = displayItems[currentIndex];
            const curGroup = curItem?.groupId;
            if (curGroup !== undefined) {
              let next = currentIndex + 1;
              while (next < displayItems.length && displayItems[next].groupId === curGroup) {
                next++;
              }
              if (next < displayItems.length) setCurrentIndex(next);
            } else {
              navigateNext();
            }
          }
          break;
        case "ArrowUp":
          e.preventDefault();
          {
            const curItem = displayItems[currentIndex];
            const curGroup = curItem?.groupId;
            if (curGroup !== undefined) {
              let prev = currentIndex - 1;
              while (prev >= 0 && displayItems[prev].groupId === curGroup) {
                prev--;
              }
              if (prev >= 0) setCurrentIndex(prev);
            } else {
              navigatePrev();
            }
          }
          break;
        case "Home":
          e.preventDefault();
          setCurrentIndex(0);
          break;
        case "End":
          e.preventDefault();
          setCurrentIndex(displayItems.length - 1);
          break;
        case "1":
        case "2":
        case "3":
        case "4":
        case "5":
          if (viewMode !== "comparison") {
            setRating(parseInt(e.key));
          }
          break;
        case "0":
          setRating(0);
          break;
        case "p":
          if (currentView !== "route") setFlag("pick");
          break;
        case "P":
          if (currentView === "select" && e.shiftKey) {
            setFlagNoAutoReject("pick");
          } else if (currentView !== "route") {
            setFlag("pick");
          }
          break;
        case "x":
        case "X":
          if (currentView !== "route") setFlag("reject");
          break;
        case "u":
        case "U":
          if (currentView === "route") {
            setDestination("unrouted");
          } else {
            setFlag("unreviewed");
          }
          break;
        case "e":
          if (!e.ctrlKey && currentView === "route") setDestination("edit");
          break;
        case "d":
        case "D":
          if (currentView === "route") setDestination("publish_direct");
          break;
        case " ":
          e.preventDefault();
          advanceToNextUnreviewed();
          break;
        case "z":
        case "Z":
          if (!e.ctrlKey) toggleZoom();
          break;
        case "g":
        case "G":
          setViewMode(mode === "grid" ? "sequential" : "grid");
          break;
        case "Tab":
          if (!e.shiftKey && currentView === "select") {
            e.preventDefault();
            enterComparison();
          }
          break;
        case "c":
          if (currentView === "select") {
            const group = getGroupForCurrentItem();
            const item = displayItems[currentIndex];
            if (group && item) {
              setGroupCover(group.id, item.image.id);
            }
          }
          break;
        case "i":
        case "I":
          toggleMetadata();
          break;
        case "?":
          toggleShortcutHints();
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    displayItems,
    currentIndex,
    currentView,
    viewMode,
    navigateNext,
    navigatePrev,
    setRating,
    setFlag,
    setFlagNoAutoReject,
    setDestination,
    undo,
    redo,
    toggleMetadata,
    toggleShortcutHints,
    toggleZoom,
    advanceToNextUnreviewed,
    setCurrentIndex,
    setGroupCover,
    setViewMode,
    getGroupForCurrentItem,
    enterComparison,
    exitComparison,
    cycleComparison,
    comparisonQuickPick,
    openSettings,
  ]);
}
