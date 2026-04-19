import { useEffect } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ShootListPage } from "./pages/ShootListPage";
import { CullPage } from "./pages/CullPage";
import { SettingsDialog } from "./components/SettingsDialog";
import { Toast } from "./components/Toast";
import { useSettingsStore } from "./stores/settingsStore";
import { useAiStore } from "./stores/aiStore";
import { useProjectStore } from "./stores/projectStore";
import type { AiProgressEvent, AiStatusResponse } from "./types";

function useAiListener() {
  const handleProgress = useAiStore((s) => s.handleProgress);
  const setStatus = useAiStore((s) => s.setStatus);
  const setProvider = useAiStore((s) => s.setProvider);
  const patchImageAiData = useProjectStore((s) => s.patchImageAiData);
  const fetchPercentiles = useAiStore((s) => s.fetchPercentiles);

  useEffect(() => {
    // Fetch initial provider status so the UI knows if AI is available.
    invoke<AiStatusResponse>("get_ai_status")
      .then((s) => setStatus(s))
      .catch(() => setProvider("disabled"));

    // Debounce percentile refetches so a long analysis run doesn't issue
    // one SQL query per photo. 500ms is snappy enough that the badge
    // values stay calibrated as the worker chews through the backlog.
    let percentileTimer: ReturnType<typeof setTimeout> | null = null;
    const schedulePercentiles = () => {
      if (percentileTimer) clearTimeout(percentileTimer);
      percentileTimer = setTimeout(() => {
        const shootId = useProjectStore.getState().currentShoot?.id;
        if (shootId) fetchPercentiles(shootId).catch(() => {});
        percentileTimer = null;
      }, 500);
    };

    // Subscribe to ai-progress events emitted by the worker for each photo.
    const unlisten = listen<AiProgressEvent>("ai-progress", (event) => {
      handleProgress(event.payload);
      // On success, pull the updated AI fields for this photo so the
      // loupe panel, AI-pick badge, and sort can see them immediately.
      if (event.payload.ok) {
        patchImageAiData(event.payload.photoId);
        schedulePercentiles();
      }
    });
    return () => {
      if (percentileTimer) clearTimeout(percentileTimer);
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [handleProgress, setStatus, setProvider, patchImageAiData, fetchPercentiles]);
}

function App() {
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  useAiListener();

  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  return (
    <>
      <Routes>
        <Route path="/" element={<Navigate to="/shoots" replace />} />
        <Route path="/shoots" element={<ShootListPage />} />
        <Route path="/shoots/:id" element={<CullPage />} />
      </Routes>
      <SettingsDialog />
      <Toast />
    </>
  );
}

export default App;
