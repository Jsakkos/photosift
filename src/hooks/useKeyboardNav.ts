import { useEffect } from "react";
import { useProjectStore } from "../stores/projectStore";

export function useKeyboardNav() {
  const {
    navigateNext,
    navigatePrev,
    setRating,
    undo,
    redo,
    toggleMetadata,
    toggleShortcutHints,
    toggleZoom,
    images,
  } = useProjectStore();

  useEffect(() => {
    if (images.length === 0) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      ) {
        return;
      }

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

      // Ctrl+O: open folder
      if (e.ctrlKey && e.key === "o") {
        e.preventDefault();
        window.dispatchEvent(new CustomEvent("photosift:open-folder"));
        return;
      }

      switch (e.key) {
        case "ArrowRight":
        case "ArrowDown":
          e.preventDefault();
          navigateNext();
          break;
        case "ArrowLeft":
        case "ArrowUp":
          e.preventDefault();
          navigatePrev();
          break;
        case "1": setRating(1); break;
        case "2": setRating(2); break;
        case "3": setRating(3); break;
        case "4": setRating(4); break;
        case "5": setRating(5); break;
        case "0": setRating(0); break;
        case " ":
          e.preventDefault();
          toggleZoom();
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
    images.length, navigateNext, navigatePrev, setRating,
    undo, redo, toggleMetadata, toggleShortcutHints, toggleZoom,
  ]);
}
