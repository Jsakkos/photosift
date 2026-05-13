import { sharpnessBandColor } from "../stores/aiStore";
import type { BadgeTone } from "./primitives";
import { surfaceStyleFor } from "./primitives/Badge";

/// Single-digit (1-10) sharpness pill. Inline (unpositioned) so FaceTile
/// can lay it out alongside the other bottom-row badges. 24px tall to match
/// the icon tiles; variable width.
/// Color band: 8-10 success, 4-7 warning, 1-3 danger — Narrative Select palette.
export function AiSharpnessBadge({ score }: { score: number }) {
  const band = sharpnessBandColor(score);
  const tone: BadgeTone =
    band === "green" ? "success" : band === "yellow" ? "warning" : "danger";
  const bandLabel = band === "green" ? "sharp" : band === "yellow" ? "soft" : "blurry";
  const title =
    `On-device AI · Sharpness ${score}/10 (${bandLabel})\n` +
    `Laplacian variance percentile within this shoot.\n` +
    `Green 8-10 · Yellow 4-7 · Red 1-3.`;
  return (
    <span
      className="inline-flex items-center justify-center h-6 min-w-6 px-1.5 rounded-xs font-mono text-[11px] font-medium leading-none pointer-events-auto"
      style={surfaceStyleFor(tone, "solid")}
      aria-label={`Sharpness ${score} of 10`}
      title={title}
    >
      {score}
    </span>
  );
}
