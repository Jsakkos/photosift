import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The server rejects any Host other than 127.0.0.1:<port> or localhost:<port>
// and any foreign Origin on non-GET/WebSocket requests, so the dev proxy must
// rewrite Host (changeOrigin). Vite's own origin (http://localhost:5173) is
// whitelisted by debug builds of the server.
const SERVER = "http://127.0.0.1:7420";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": { target: SERVER, changeOrigin: true },
      "/media": { target: SERVER, changeOrigin: true },
      "/ws": { target: SERVER, changeOrigin: true, ws: true },
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
