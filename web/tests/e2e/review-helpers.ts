import type { ShootSummary } from "../../src/api/types";
import { test, type CardFile, type TestServer } from "./fixtures";

const CAMERA = "TestCam One";
// A 400-photo shoot at 1200 px takes ~3 min to import and enrich on a CI runner.
const SETUP_MIN_MS = 120_000;
const SETUP_MS_PER_PHOTO = 1_500;

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

function clock(base: Date, addSeconds: number): string {
  const t = new Date(base.getTime() + addSeconds * 1000);
  return (
    `${t.getFullYear()}-${pad(t.getMonth() + 1)}-${pad(t.getDate())}` +
    `T${pad(t.getHours())}:${pad(t.getMinutes())}:${pad(t.getSeconds())}`
  );
}

/**
 * A card of near-duplicate bursts: frames inside a burst are 2 s apart and share
 * a pattern, bursts are 20 s apart (past the 10 s burst window), moments are an
 * hour apart. Every third burst is a portrait.
 */
export function burstCard(momentBursts: number[][], opts: { size?: [number, number] } = {}): CardFile[] {
  const [lw, lh] = opts.size ?? [640, 480];
  const files: CardFile[] = [];
  let n = 0;
  let burstNo = 0;
  momentBursts.forEach((bursts, m) => {
    const start = new Date(2024, 4, 1, 10 + m, 0, 0);
    let t = 0;
    for (const size of bursts) {
      const portrait = burstNo % 3 === 2;
      const pattern = burstNo % 2 === 0 ? "blocks" : "stripes";
      for (let k = 0; k < size; k += 1) {
        n += 1;
        files.push({
          name: `IMG_${String(n).padStart(4, "0")}.jpg`,
          time: clock(start, t),
          camera: CAMERA,
          w: portrait ? lh : lw,
          h: portrait ? lw : lh,
          pattern,
        });
        t += 2;
      }
      t += 20;
      burstNo += 1;
    }
  });
  return files;
}

// 40 photos: three moments, 8 + 8 + 6 bursts, a mix of landscape and portrait.
export const FORTY = burstCard([
  [3, 1, 2, 1, 3, 2, 1, 1],
  [2, 1, 3, 1, 1, 2, 3, 1],
  [4, 1, 1, 2, 1, 3],
]);

/**
 * Imports and enriches a shoot on its own time budget. On a shared CI runner the import
 * can take most of a 30 s test timeout while another worker enriches the 400-photo shoot,
 * so the caller's timeout is extended for the setup and the test keeps its full budget
 * after it.
 */
export async function importShoot(server: TestServer, files: CardFile[]): Promise<number> {
  const info = test.info();
  const testBudget = info.timeout;
  const setupBudget = Math.max(SETUP_MIN_MS, files.length * SETUP_MS_PER_PHOTO);
  const started = Date.now();
  info.setTimeout(testBudget + setupBudget);
  const card = await server.makeCard(files);
  await server.importCard(card);
  let shoot: ShootSummary | undefined;
  while (!shoot?.enriched && Date.now() - started < setupBudget) {
    await new Promise((r) => setTimeout(r, 250));
    const list = (await (await fetch(`${server.baseUrl}/api/shoots`)).json()) as ShootSummary[];
    shoot = list.find((s) => s.counts.total === files.length);
  }
  if (!shoot?.enriched) throw new Error("the shoot never finished importing and enriching");
  info.setTimeout(testBudget + (Date.now() - started));
  return shoot.id;
}
