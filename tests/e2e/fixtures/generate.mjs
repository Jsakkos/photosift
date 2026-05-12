// Deterministic JPEG fixture generator for screenshot CI.
//
// Run with `npm run fixtures:generate` after touching this script. Commit
// the resulting JPEGs under tests/e2e/fixtures/img/ — they're consumed at
// runtime by the testing.rs seeder (copies preview/thumb files into the
// shoot's cache dir).
//
// The images are not real photography. They're solid-color rectangles
// with a high-contrast label, sized to roughly match a D750 preview so
// the UI's downsampling math behaves normally. Each one has a stable
// hue derived from its index so a diff between two fixtures is obvious
// in baselines.

import sharp from "sharp";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = resolve(__dirname, "img");

const PREVIEW_W = 1600;
const PREVIEW_H = 1067;

// 12 visually-distinct hues stepped around the wheel.
const SAMPLES = Array.from({ length: 12 }, (_, i) => ({
  index: i + 1,
  hue: (i * 30) % 360,
}));

function svgFor(index, hue, w, h) {
  const bg = `hsl(${hue} 55% 35%)`;
  const fg = `hsl(${hue} 35% 88%)`;
  return Buffer.from(`<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="${bg}" stop-opacity="1"/>
      <stop offset="100%" stop-color="hsl(${(hue + 40) % 360} 55% 18%)" stop-opacity="1"/>
    </linearGradient>
  </defs>
  <rect width="${w}" height="${h}" fill="url(#g)"/>
  <text x="50%" y="50%" font-family="monospace" font-size="${Math.round(h * 0.28)}"
        font-weight="700" fill="${fg}" text-anchor="middle" dominant-baseline="middle"
        letter-spacing="${Math.round(h * 0.01)}">
    ${String(index).padStart(2, "0")}
  </text>
  <text x="50%" y="${h * 0.85}" font-family="monospace" font-size="${Math.round(h * 0.04)}"
        fill="${fg}" fill-opacity="0.6" text-anchor="middle">
    photosift.fixture
  </text>
</svg>`);
}

async function main() {
  await mkdir(OUT_DIR, { recursive: true });
  for (const { index, hue } of SAMPLES) {
    const filename = `sample_${String(index).padStart(2, "0")}.jpg`;
    const out = resolve(OUT_DIR, filename);
    const svg = svgFor(index, hue, PREVIEW_W, PREVIEW_H);
    const buf = await sharp(svg)
      .jpeg({ quality: 80, mozjpeg: false, chromaSubsampling: "4:2:0" })
      .toBuffer();
    await writeFile(out, buf);
    process.stdout.write(`  wrote ${filename} (${buf.length} bytes)\n`);
  }
  process.stdout.write(`done — ${SAMPLES.length} fixtures in ${OUT_DIR}\n`);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
