import { sharpnessBandColor } from "../stores/aiStore";

/// Single-digit (1-10) sharpness pill. Inline (unpositioned) so FaceTile
/// can lay it out alongside the other bottom-row badges.
/// Color band: 8-10 green, 4-7 yellow, 1-3 red — Narrative Select palette.
export function AiSharpnessBadge({ score }: { score: number }) {
  const band = sharpnessBandColor(score);
  const bg =
    band === "green"
      ? "bg-green-500/90"
      : band === "yellow"
        ? "bg-yellow-500/90"
        : "bg-red-500/90";
  const bandLabel = band === "green" ? "sharp" : band === "yellow" ? "soft" : "blurry";
  const title =
    `Sharpness ${score}/10 (${bandLabel})\n` +
    `Laplacian variance percentile within this shoot.\n` +
    `Green 8-10 · Yellow 4-7 · Red 1-3.`;
  return (
    <div
      className={`${bg} text-white text-[11px] font-semibold leading-none rounded px-2 h-6 flex items-center justify-center min-w-[26px] text-center pointer-events-auto shadow-sm`}
      aria-label={`Sharpness ${score} of 10`}
      title={title}
    >
      {score}
    </div>
  );
}
