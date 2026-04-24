import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";

export function useKeyboardNav() {
  const {
    navigateNext,
    navigatePrev,
    setRating,
    setFlag,
    undo,
    redo,
    toggleShortcutHints,
    toggleAutoAdvance,
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
    enterBracket,
    exitBracket,
    bracketDecision,
    pickCurrent,
    skipCurrent,
    setActiveInnerGroup,
  } = useProjectStore();
  const selectBracket = useProjectStore((s) => s.selectBracket);
  const openSettings = useSettingsStore((s) => s.openDialog);
  const setToast = useProjectStore((s) => s.setToast);

  useEffect(() => {
    // Don't guard on `displayItems.length === 0` here — view-level keys
    // like `[` / `]` (Select pass floor), `,` (settings), and `?`
    // (shortcut hints) must still work when the current filter yields
    // zero photos, otherwise the user gets trapped in an empty tier with
    // no way back. Per-branch handlers that dereference the current item
    // are already defensive (setRating/setFlag early-return on `!item`,
    // navigateNext bounds-checks, etc.).
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement
      ) {
        return;
      }

      const mode = viewMode as string;
      // Bracket sub-state of Select: overrides 1/2/3 + Tab + arrows so
      // pair decisions and single/2-up toggling work without conflict
      // with the single-photo bindings below.
      if (selectBracket !== null) {
        switch (e.key) {
          case "1":
            e.preventDefault();
            void bracketDecision("L");
            return;
          case "2":
            e.preventDefault();
            void bracketDecision("R");
            return;
          case "3":
          case " ":
            e.preventDefault();
            void bracketDecision("both");
            return;
          case "Tab":
            e.preventDefault();
            exitBracket();
            return;
          case "Escape":
            e.preventDefault();
            exitBracket();
            return;
          case "z":
          case "Z":
            if (!e.ctrlKey && !e.metaKey) toggleZoom();
            return;
        }
        return;
      }

      if (mode === "grid") return;

      // Cross-platform primary modifier: Ctrl on Windows/Linux, Cmd on
      // macOS. The shortcut hints still say "Ctrl" (current UI is
      // Windows-first) but the handler accepts either so Mac testers
      // don't get blank-keystroke surprises.
      const primaryMod = e.ctrlKey || e.metaKey;

      if (primaryMod && e.key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
        return;
      }
      if (primaryMod && (e.key === "Z" || (e.shiftKey && e.key === "z"))) {
        e.preventDefault();
        redo();
        return;
      }

      if (e.key === ",") {
        e.preventDefault();
        openSettings();
        return;
      }

      // Shift+A: accept the AI-recommended photo as group cover.
      if (e.shiftKey && (e.key === "a" || e.key === "A")) {
        e.preventDefault();
        useProjectStore.getState().acceptAiPick();
        return;
      }

      // Alt+S: cycle AI sort (none → sharpness → faces → none).
      if (e.altKey && (e.key === "s" || e.key === "S")) {
        e.preventDefault();
        useProjectStore.getState().cycleSortByAi();
        return;
      }

      switch (e.key) {
        case "ArrowRight":
          e.preventDefault();
          // In Select, the right arrow is the "skip" verb — move to
          // next photo without changing rating. In other views it's
          // plain navigation.
          if (currentView === "select") {
            skipCurrent();
          } else {
            navigateNext();
          }
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
          if (currentView === "select") {
            setRating(parseInt(e.key));
          }
          break;
        case "0":
          if (currentView === "select") setRating(0);
          break;
        case "p":
          // P is a Triage-only gesture. In Select every photo is already
          // picked, so binding P to setFlag would be a no-op at best and
          // (via the Select-P group cascade) a silent bulk reject at
          // worst. Stars are the Select verb.
          if (currentView === "triage") setFlag("pick");
          break;
        case "P":
          if (currentView === "triage") {
            if (e.shiftKey) {
              e.preventDefault();
              useProjectStore.getState().keepAllInCurrentGroup();
            } else {
              setFlag("pick");
            }
          }
          break;
        case "x":
        case "X":
          if (currentView !== "route") setFlag("reject");
          break;
        case "[":
          if (currentView === "select") {
            e.preventDefault();
            const cur = useProjectStore.getState().selectMinStar;
            useProjectStore.getState().setSelectMinStar(cur - 1);
          }
          break;
        case "]":
          if (currentView === "select") {
            e.preventDefault();
            const cur = useProjectStore.getState().selectMinStar;
            useProjectStore.getState().setSelectMinStar(cur + 1);
          }
          break;
        case "u":
        case "U":
          setFlag("unreviewed");
          break;
        case " ":
          e.preventDefault();
          // In Select, Space is "pick + advance" (+1 star). In Triage/
          // Route it keeps the legacy "advance to next unreviewed" verb.
          if (currentView === "select") {
            void pickCurrent();
          } else {
            advanceToNextUnreviewed();
          }
          break;
        case "z":
        case "Z":
          if (!e.ctrlKey && !e.metaKey) toggleZoom();
          break;
        case "f":
        case "F":
          // Toggles the 220px Faces rail in the new Triage/Select shell.
          // The legacy aiPanelForced flag is still live for the old
          // AiPanel path (Select/Route), which stays on F until those
          // phases land their redesigned shells.
          if (!e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
            useProjectStore.getState().toggleFaces();
            useProjectStore.getState().toggleAiPanel();
          }
          break;
        case "t":
        case "T":
          if (!e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
            useProjectStore.getState().toggleAllStrip();
          }
          break;
        case "h":
        case "H":
          if (!e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
            useProjectStore.getState().toggleHeatmap();
          }
          break;
        case "g":
        case "G":
          // Bare G toggles grid. Ctrl/Cmd+G is reserved for grouping
          // inside GridView, so don't hijack it here even though grid
          // mode has its own handler — stopping here keeps the two from
          // fighting when a press lands just before the mode switch.
          if (!e.ctrlKey && !e.metaKey)
            setViewMode(mode === "grid" ? "sequential" : "grid");
          break;
        case "Tab":
          if (!e.shiftKey && currentView === "select") {
            e.preventDefault();
            enterBracket();
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
        case "a":
        case "A":
          if (currentView === "triage") {
            const { toast } = useProjectStore.getState();
            toggleAutoAdvance();
            const newState = useProjectStore.getState().autoAdvance;
            if (!toast) {
              setToast(`Auto-advance ${newState ? "on" : "off"}`);
            }
          }
          break;
        case "?":
          toggleShortcutHints();
          break;
        case "Enter":
          {
            const focused = displayItems[currentIndex];
            if (
              focused?.isGroupCover &&
              focused.groupId !== undefined &&
              (currentView === "triage" || currentView === "select")
            ) {
              e.preventDefault();
              setActiveInnerGroup(focused.groupId);
            }
          }
          break;
        case "Escape":
          // Esc closes the inner strip if one is open. Doesn't compete
          // with the Grid-mode ExitPlanMode-ish escapes elsewhere.
          if (useProjectStore.getState().activeInnerGroupId != null) {
            e.preventDefault();
            setActiveInnerGroup(null);
          }
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
    selectBracket,
    navigateNext,
    navigatePrev,
    setRating,
    setFlag,
    undo,
    redo,
    toggleShortcutHints,
    toggleAutoAdvance,
    toggleZoom,
    advanceToNextUnreviewed,
    setCurrentIndex,
    setGroupCover,
    setViewMode,
    getGroupForCurrentItem,
    enterBracket,
    exitBracket,
    bracketDecision,
    pickCurrent,
    skipCurrent,
    openSettings,
    setToast,
    setActiveInnerGroup,
  ]);
}
