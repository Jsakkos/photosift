import { IconBadge } from "./primitives";

/// Inline species tile for the FaceTile bottom-row. Renders only for
/// non-human detections (cats today). Humans are the default, so leaving
/// the slot empty keeps the portrait flow visually quiet.
export function AiSpeciesIcon({ species }: { species: string }) {
  if (species !== "cat") return null;
  return (
    <IconBadge
      variant="surface"
      className="pointer-events-auto"
      aria-label="Cat face"
      title={
        "On-device AI · Cat face (detected by Tiny-YOLOv3).\n" +
        "Bbox is heuristically cropped from the whole-cat detection.\n" +
        "No per-eye or smile score for cats today."
      }
    >
      <CatIcon />
    </IconBadge>
  );
}

function CatIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path
        d="M3 5 L5 9 L3 12 L7 11 L10 14 L13 11 L17 12 L15 9 L17 5 L13 8 L10 6 L7 8 Z"
        fill="currentColor"
        opacity="0.9"
      />
      <circle cx="8" cy="10" r="0.9" fill="var(--color-bg3)" />
      <circle cx="12" cy="10" r="0.9" fill="var(--color-bg3)" />
    </svg>
  );
}
