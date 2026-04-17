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
import type { AiProgressEvent, AiProviderStatus } from "./types";

function useAiListener() {
  const handleProgress = useAiStore((s) => s.handleProgress);
  const setProvider = useAiStore((s) => s.setProvider);

  useEffect(() => {
    // Fetch initial provider status so the UI knows if AI is available.
    invoke<{ provider: AiProviderStatus }>("get_ai_status")
      .then((s) => setProvider(s.provider))
      .catch(() => setProvider("disabled"));

    // Subscribe to ai-progress events emitted by the worker for each photo.
    const unlisten = listen<AiProgressEvent>("ai-progress", (event) => {
      handleProgress(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [handleProgress, setProvider]);
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
