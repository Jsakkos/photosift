import { useEffect } from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import { ShootListPage } from "./pages/ShootListPage";
import { CullPage } from "./pages/CullPage";
import { SettingsDialog } from "./components/SettingsDialog";
import { Toast } from "./components/Toast";
import { useSettingsStore } from "./stores/settingsStore";

function App() {
  const loadSettings = useSettingsStore((s) => s.loadSettings);

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
