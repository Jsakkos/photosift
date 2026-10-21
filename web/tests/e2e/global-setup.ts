import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";

const WEB_DIR = path.resolve(import.meta.dirname, "../..");
const SERVER_MANIFEST = path.resolve(WEB_DIR, "../server/Cargo.toml");

function run(command: string, args: string[]): void {
  const result = spawnSync(command, args, { cwd: WEB_DIR, stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed (${result.status ?? result.error})`);
  }
}

/** Builds the server (photosift + make-synthetic-card) and the web bundle once. */
export default function globalSetup(): void {
  run("cargo", ["build", "--manifest-path", SERVER_MANIFEST, "--bins"]);
  // Run vite through node directly: spawning the npm .cmd shim needs a shell.
  const viteBin = path.join(
    path.dirname(createRequire(import.meta.url).resolve("vite/package.json")),
    "bin",
    "vite.js",
  );
  run(process.execPath, [viteBin, "build"]);
}
