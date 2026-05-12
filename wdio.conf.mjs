// WebdriverIO config that drives the built debug Tauri binary via
// tauri-driver. tauri-driver is a small intermediary that translates
// generic WebDriver requests into msedgedriver-compatible ones for
// WebView2 (the OS webview Tauri uses on Windows).
//
// Run: npx wdio run wdio.conf.mjs
//
// Prereqs:
//   - cargo install tauri-driver
//   - msedgedriver.exe matching the system Microsoft Edge version
//     (we keep one at .tools/msedgedriver.exe — see scripts in package.json)
//   - The built debug binary at src-tauri/target/debug/photosift.exe.
//     Build with: npm run tauri build -- --debug

import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const APP_BINARY = path.resolve(
  __dirname,
  "src-tauri/target/debug/photosift.exe",
);
const MSEDGEDRIVER = path.resolve(__dirname, ".tools/msedgedriver.exe");

let tauriDriver;

export const config = {
  runner: "local",
  specs: ["./tests/e2e/**/*.spec.mjs"],
  maxInstances: 1,
  // tauri-driver listens on 4444 by default; we connect WDIO to it.
  hostname: "127.0.0.1",
  port: 4444,
  capabilities: [
    {
      browserName: "wry",
      // Tauri's WebDriver server reads the binary path from this key.
      "tauri:options": {
        application: APP_BINARY,
      },
      // Raise the script-execution timeout from the WebDriver default
      // of 30s — copying 4 ~80MB RAFs + thumb/preview/pHash + group
      // clustering on a slow source drive can run several minutes.
      "wdio:enforceWebDriverClassic": true,
      timeouts: {
        script: 300_000,
      },
    },
  ],
  framework: "mocha",
  mochaOpts: {
    timeout: 180_000, // app startup + import flow takes time
  },
  reporters: ["spec"],
  logLevel: "warn",

  // Spawn tauri-driver before each session and kill it after.
  beforeSession() {
    tauriDriver = spawn(
      "tauri-driver",
      ["--native-driver", MSEDGEDRIVER],
      { stdio: ["ignore", "pipe", "pipe"] },
    );
    tauriDriver.stdout.on("data", (d) => process.stdout.write(`[tauri-driver] ${d}`));
    tauriDriver.stderr.on("data", (d) => process.stderr.write(`[tauri-driver] ${d}`));
  },
  afterSession() {
    tauriDriver?.kill();
  },
};
