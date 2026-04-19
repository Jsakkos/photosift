import { sharpnessBandColor } from "../stores/aiStore";

/// Single-digit (1-10) sharpness pill overlay. Sized for bottom-right
/// corner of a 160px face tile. Color band: 8-10 green, 4-7 yellow, 1-3
/// red — matches the Narrative Select reference.
export function AiSharpnessBadge({ score }: { score: number }) {
  const band = sharpnessBandColor(score);
  const bg =
    band === "green"
      ? "bg-green-500/85"
      : band === "yellow"
        ? "bg-yellow-500/85"
        : "bg-red-500/85";
  return (
    <div
      className={`absolute bottom-1 right-1 ${bg} text-white text-[11px] font-semibold leading-none rounded px-1.5 py-1 min-w-[24px] text-center pointer-events-none shadow-sm`}
      aria-label={`Sharpness ${score} of 10`}
      title={`Sharpness ${score}/10 (relative to this shoot)`}
    >
      {score}
    </div>
  );
}
