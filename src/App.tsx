import { useCallback, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "./stores/projectStore";
import { useKeyboardNav } from "./hooks/useKeyboardNav";
import { LoupeView } from "./components/LoupeView";
import { Filmstrip } from "./components/Filmstrip";
import { Toolbar } from "./components/Toolbar";
import { RatingBar } from "./components/RatingBar";
import { MetadataOverlay } from "./components/MetadataOverlay";
import { ShortcutHints } from "./components/ShortcutHints";

function App() {
  const { projectInfo, isLoading, openProject } = useProjectStore();
  useKeyboardNav();

  const handleOpen = useCallback(async () => {
    const selected = await open({ directory: true });
    if (selected) {
      await openProject(selected);
    }
  }, [openProject]);

  // Listen for Ctrl+O from keyboard handler
  useEffect(() => {
    const handler = () => handleOpen();
    window.addEventListener("photosift:open-folder", handler);
    return () => window.removeEventListener("photosift:open-folder", handler);
  }, [handleOpen]);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
  }, []);

  // Welcome screen
  if (!projectInfo) {
    return (
      <div
        className="h-screen w-screen flex flex-col items-center justify-center bg-[var(--bg-primary)]"
        onDragOver={handleDragOver}
      >
        <h1 className="text-3xl font-light text-[var(--text-primary)] mb-2">PhotoSift</h1>
        <p className="text-[var(--text-secondary)] mb-6">
          Fast photo culling for photographers
        </p>
        <button
          onClick={handleOpen}
          disabled={isLoading}
          className="px-6 py-3 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors disabled:opacity-50"
        >
          {isLoading ? "Opening..." : "Open Folder"}
        </button>
        <p className="mt-4 text-xs text-[var(--text-secondary)]">
          or press Ctrl+O
        </p>
      </div>
    );
  }

  // Main culling view
  return (
    <div className="h-screen w-screen flex flex-col bg-[var(--bg-primary)]">
      <Toolbar />
      <div className="flex-1 relative overflow-hidden">
        <LoupeView />
        <MetadataOverlay />
        <ShortcutHints />
      </div>
      <Filmstrip />
      <RatingBar />
    </div>
  );
}

export default App;
