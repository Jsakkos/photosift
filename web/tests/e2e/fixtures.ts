import { execFile, spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { test as base, expect } from "@playwright/test";

import type { CardScan, ImportRequest, ImportStarted } from "../../src/api/types";

const run = promisify(execFile);

const REPO_ROOT = path.resolve(import.meta.dirname, "../../..");
const TARGET_DIR = process.env.CARGO_TARGET_DIR ?? path.join(REPO_ROOT, "server", "target");
const EXE = process.platform === "win32" ? ".exe" : "";
const SERVER_BIN = path.join(TARGET_DIR, "debug", `photosift${EXE}`);
const CARD_BIN = path.join(TARGET_DIR, "debug", `make-synthetic-card${EXE}`);
const FAKE_CLAUDE_BIN = path.join(TARGET_DIR, "debug", `fake-claude${EXE}`);
const WEB_DIST = path.join(REPO_ROOT, "web", "dist");

/** One synthetic JPEG; `time` is `YYYY-MM-DDTHH:MM:SS`, `camera` becomes EXIF Make. */
export interface CardFile {
  name: string;
  time: string;
  camera: string;
  w?: number;
  h?: number;
  orientation?: number;
  /** Same pattern and size on consecutive frames makes them a near-duplicate burst. */
  pattern?: "blocks" | "stripes";
}

/** A `fake-claude` script: see the header of server/src/bin/fake-claude.rs. */
export type ClaudeScript = Record<string, unknown>;

export interface TestServer {
  baseUrl: string;
  port: number;
  /** The server's data root (PHOTOSIFT_HOME). */
  home: string;
  /** Where imports land; empty at the start of every test. */
  libraryRoot: string;
  /** Replaces the fake claude script in place; the next helper run reads it. */
  setClaudeScript(script: ClaudeScript): Promise<void>;
  /** Writes synthetic JPEGs into a fresh directory and returns its path. */
  makeCard(files: CardFile[]): Promise<string>;
  /** Scans `cardDir` through the API, so the server caches it for the import page. */
  scanCard(cardDir: string): Promise<CardScan>;
  /** Scans `cardDir` and imports every proposed event, through the API. */
  importCard(cardDir: string): Promise<{ scan: CardScan; jobId: string }>;
}

async function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = net.createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const address = probe.address();
      probe.close(() => {
        if (address && typeof address === "object") resolve(address.port);
        else reject(new Error("no port"));
      });
    });
  });
}

async function waitForHealth(baseUrl: string, proc: ChildProcess, logs: () => string): Promise<void> {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (proc.exitCode !== null) {
      throw new Error(`photosift exited with ${proc.exitCode}\n${logs()}`);
    }
    try {
      const res = await fetch(`${baseUrl}/api/health`);
      if (res.ok) return;
    } catch {
      // Not listening yet.
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`photosift did not become healthy\n${logs()}`);
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST ${url} -> ${res.status} ${await res.text()}`);
  return (await res.json()) as T;
}

async function startServer(
  claudeScript: ClaudeScript | null,
): Promise<{ server: TestServer; stop: () => Promise<void> }> {
  const home = await fs.mkdtemp(path.join(os.tmpdir(), "photosift-e2e-"));
  const libraryRoot = path.join(home, "library");
  await fs.mkdir(libraryRoot);
  const port = await freePort();
  // TOML literal strings (single quotes) keep Windows backslashes verbatim.
  const scriptPath = path.join(home, "fake-claude.json");
  // The helper is off unless a test brings a script: an e2e import must never
  // start the real `claude`.
  const transport = claudeScript ? "cli" : "off";
  await fs.writeFile(
    path.join(home, "settings.toml"),
    `library_root = '${libraryRoot}'\nport = ${port}\nhelper_transport = '${transport}'\n`,
  );
  if (claudeScript) await fs.writeFile(scriptPath, JSON.stringify(claudeScript));
  const claudeEnv = claudeScript
    ? { PHOTOSIFT_CLAUDE_BIN: FAKE_CLAUDE_BIN, FAKE_CLAUDE_SCRIPT: scriptPath }
    : {};

  let output = "";
  const proc = spawn(SERVER_BIN, [], {
    env: { ...process.env, PHOTOSIFT_HOME: home, PHOTOSIFT_WEB_DIST: WEB_DIST, ...claudeEnv },
    stdio: ["ignore", "pipe", "pipe"],
  });
  proc.stdout?.on("data", (d: Buffer) => (output += d.toString()));
  proc.stderr?.on("data", (d: Buffer) => (output += d.toString()));
  const baseUrl = `http://127.0.0.1:${port}`;

  const stop = async () => {
    if (proc.exitCode === null) {
      const exited = new Promise((resolve) => proc.once("exit", resolve));
      proc.kill();
      await exited;
    }
    await fs.rm(home, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  };

  try {
    await waitForHealth(baseUrl, proc, () => output);
  } catch (e) {
    await stop();
    throw e;
  }

  let cards = 0;
  const server: TestServer = {
    baseUrl,
    port,
    home,
    libraryRoot,
    async setClaudeScript(script) {
      await fs.writeFile(scriptPath, JSON.stringify(script));
    },
    async makeCard(files) {
      cards += 1;
      const dir = path.join(home, `card-${cards}`);
      // A file, not an argument: 400 entries overflow the Windows command line.
      const spec = path.join(home, `card-${cards}.json`);
      await fs.writeFile(spec, JSON.stringify(files));
      await run(CARD_BIN, [dir, `@${spec}`]);
      return dir;
    },
    async scanCard(cardDir) {
      return postJson<CardScan>(`${baseUrl}/api/scan`, { source: cardDir });
    },
    async importCard(cardDir) {
      const scan = await postJson<CardScan>(`${baseUrl}/api/scan`, { source: cardDir });
      const request: ImportRequest = {
        scan_id: scan.scan_id,
        events: scan.events.map((e) => ({
          item_idxs: e.item_idxs,
          kind: e.kind,
          name: e.suggested_name || "Test event",
        })),
      };
      const { job_id } = await postJson<ImportStarted>(`${baseUrl}/api/import`, request);
      return { scan, jobId: job_id };
    },
  };
  return { server, stop };
}

export const test = base.extend<{ server: TestServer; claudeScript: ClaudeScript | null }>({
  claudeScript: [null, { option: true }],
  server: async ({ claudeScript }, use) => {
    const { server, stop } = await startServer(claudeScript);
    try {
      await use(server);
    } finally {
      await stop();
    }
  },
  baseURL: async ({ server }, use) => {
    await use(server.baseUrl);
  },
});

export { expect };
