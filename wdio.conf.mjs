// WebdriverIO config that drives the built debug Tauri binary via
// tauri-driver. tauri-driver is a small intermediary that translates
// generic WebDriver requests into msedgedriver-compatible ones for
// WebView2 (the OS webview Tauri uses on Windows).
//
// Run:
//   npm run test:e2e              -> raf-import IPC regression
//   npm run test:e2e:screenshots  -> screenshot suite (issue #20)
//
// Prereqs:
//   - cargo install tauri-driver
//   - msedgedriver.exe matching the *WebView2 Runtime* version — NOT the
//     standalone Edge browser, which can be a different release. Kept at
//     .tools/msedgedriver.exe; CI resolves it in .github/workflows/screenshots.yml.
//   - The built debug binary at src-tauri/target/debug/photosift.exe.
//     Build with: npm run test:e2e:build

import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Prefer debug; fall back to release. CI uses --debug.
const DEBUG_BINARY = path.resolve(
  __dirname,
  "src-tauri/target/debug/photosift.exe",
);
const RELEASE_BINARY = path.resolve(
  __dirname,
  "src-tauri/target/release/photosift.exe",
);
const APP_BINARY = existsSync(DEBUG_BINARY) ? DEBUG_BINARY : RELEASE_BINARY;
const MSEDGEDRIVER = path.resolve(__dirname, ".tools/msedgedriver.exe");

const FIXTURES_DIR = path.resolve(__dirname, "tests/e2e/fixtures/img");
const SNAPSHOTS_DIR = path.resolve(__dirname, "tests/e2e/__snapshots__");
const ACTUAL_DIR = path.resolve(__dirname, "tests/e2e/__screenshots__");
const DIFF_DIR = path.resolve(__dirname, "tests/e2e/__diff__");

// Screenshot CI needs:
//   - PHOTOSIFT_HOME pointing at a throwaway state dir (so we don't
//     wreck the developer's library), and
//   - PHOTOSIFT_E2E_FIXTURES pointing at the committed JPEGs the
//     debug-only seeder reads.
// Local devs run tests against the same env. CI sets PHOTOSIFT_HOME via
// the workflow file; we default it to a worktree-local sandbox so a
// manual `npm run test:e2e:screenshots` is safe.
const DEFAULT_HOME = path.resolve(__dirname, ".photosift-ci-local");
const RUNTIME_ENV = {
  ...process.env,
  PHOTOSIFT_HOME: process.env.PHOTOSIFT_HOME ?? DEFAULT_HOME,
  PHOTOSIFT_E2E_FIXTURES: process.env.PHOTOSIFT_E2E_FIXTURES ?? FIXTURES_DIR,
};

let tauriDriver;

export const config = {
  runner: "local",
  specs: ["./tests/e2e/**/*.spec.mjs"],
  maxInstances: 1,
  hostname: "127.0.0.1",
  port: 4444,
  capabilities: [
    {
      browserName: "wry",
      "tauri:options": {
        application: APP_BINARY,
      },
      "wdio:enforceWebDriverClassic": true,
      timeouts: {
        script: 300_000,
      },
    },
  ],
  framework: "mocha",
  mochaOpts: {
    timeout: 180_000,
  },
  reporters: ["spec"],
  logLevel: "warn",

  // @wdio/visual-service plugs in a `browser.checkScreen("tag")` matcher
  // that compares against a baseline PNG under `baselineFolder`. On
  // failure it writes a diff PNG under `diffFolder`. Per-platform
  // subdirs are mandatory — Windows DirectWrite and Linux freetype
  // produce fundamentally different pixels even with AA disabled.
  services: [
    [
      "visual",
      {
        baselineFolder: path.join(SNAPSHOTS_DIR, process.platform),
        screenshotPath: ACTUAL_DIR,
        formatImageName: "{tag}",
        savePerInstance: false,
        autoSaveBaseline: process.env.WDIO_UPDATE_BASELINES === "1",
        // Modest threshold: WebView2 + DirectWrite text rendering can
        // shift a few subpixels even with antialiasing pinned. Tighten
        // later if false-negatives become a problem.
        compareOptions: {
          blockOutSideBar: false,
          ignoreAntialiasing: true,
          ignoreColors: false,
        },
        diffFolder: DIFF_DIR,
      },
    ],
  ],

  beforeSession() {
    tauriDriver = spawn("tauri-driver", ["--native-driver", MSEDGEDRIVER], {
      stdio: ["ignore", "pipe", "pipe"],
      env: RUNTIME_ENV,
    });
    tauriDriver.stdout.on("data", (d) =>
      process.stdout.write(`[tauri-driver] ${d}`),
    );
    tauriDriver.stderr.on("data", (d) =>
      process.stderr.write(`[tauri-driver] ${d}`),
    );
  },
  afterSession() {
    tauriDriver?.kill();
  },
};
