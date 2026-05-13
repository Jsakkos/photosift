import { Badge, type BadgePos } from "./primitives";

/// AI pick stamp shown on a thumbnail. Defaults to the top-right corner;
/// callers can override placement via `pos` when other top-right badges
/// (e.g. the verdict square) need to claim that slot — pass `pos={undefined}`
/// and supply an absolute-positioning className instead.
export function AiPickBadge({
  pos = "tr",
  className,
}: {
  pos?: BadgePos;
  className?: string;
} = {}) {
  return (
    <Badge
      pos={pos}
      tone="accent"
      variant="soft"
      className={`font-semibold pointer-events-auto ${className ?? ""}`.trim()}
      title={
        "On-device AI pick from this group.\n" +
        "Score = sharpness × (1 + eyes_open) × (1 + 0.5 × smile).\n" +
        "Shift+A accepts as group cover."
      }
      aria-label="On-device AI pick"
    >
      ★ AI
    </Badge>
  );
}
