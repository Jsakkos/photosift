import React from "react";
import ReactDOM from "react-dom/client";
import { HashRouter } from "react-router-dom";
import App from "./App";
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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </React.StrictMode>,
);
