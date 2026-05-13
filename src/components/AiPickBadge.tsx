import { Badge } from "./primitives";

export function AiPickBadge() {
  return (
    <Badge
      pos="tr"
      tone="accent"
      variant="soft"
      className="font-semibold pointer-events-auto"
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
