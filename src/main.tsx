import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter } from "react-router-dom";
import App from "./App";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "./stores/projectStore";
import { useSettingsStore } from "./stores/settingsStore";
import { useShootListStore } from "./stores/shootListStore";
import "@fontsource/jetbrains-mono/400.css";
import "@fontsource/jetbrains-mono/500.css";
import "./styles/globals.css";

// Dev-only: register the tauri-plugin-mcp webview-side listeners so
// Claude Code can drive execute_js, query_page, click-by-ref, etc.
// The Rust plugin emits Tauri events; without these listeners the
// bridge times out. Fire-and-forget — don't block app boot.
if (import.meta.env.DEV) {
  import("tauri-plugin-mcp")
    .then(({ setupPluginListeners }) => setupPluginListeners())
    .catch((err) => console.error("tauri-plugin-mcp setup failed:", err));
}

// E2E hook: surface the Zustand stores and Tauri `invoke` so the
// screenshot-CI specs can drive commands without dealing with Vite's
// bare-module-specifier resolution inside eval contexts. Always
// exposed because `tauri build --debug` produces a `vite build`
// (mode=production) yet still needs this for screenshot tests. The
// store hooks are already in the bundle anyway, so exposing a ref
// adds zero weight.
(window as unknown as { __PHOTOSIFT__: unknown }).__PHOTOSIFT__ = {
  invoke,
  projectStore: useProjectStore,
  settingsStore: useSettingsStore,
  shootListStore: useShootListStore,
};

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </React.StrictMode>,
);
