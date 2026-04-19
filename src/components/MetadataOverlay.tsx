import { useProjectStore } from "../stores/projectStore";

function orientationLabel(value: number | null | undefined): string | null {
  switch (value) {
    case 2:
      return "flipped horizontal";
    case 3:
      return "rotated 180\u00b0";
    case 4:
      return "flipped vertical";
    case 5:
      return "transposed";
    case 6:
      return "rotated 90\u00b0 CW";
    case 7:
      return "transversed";
    case 8:
      return "rotated 90\u00b0 CCW";
    default:
      return null;
  }
}

export function MetadataOverlay() {
  const { displayItems, currentIndex, showMetadata } = useProjectStore();
  if (!showMetadata) return null;

  const image = displayItems[currentIndex]?.image;
  if (!image) return null;

  const orientation = orientationLabel(image.orientation);

  const lines = [
    image.filename,
    image.captureTime,
    [
      image.focalLength ? `${image.focalLength}mm` : null,
      image.aperture ? `f/${image.aperture}` : null,
      image.shutterSpeed,
      image.iso ? `ISO ${image.iso}` : null,
    ]
      .filter(Boolean)
      .join("  \u00b7  ") || null,
    image.cameraModel,
    image.lens,
    orientation,
  ].filter(Boolean);

  return (
    <div className="absolute top-2 right-2 px-3 py-2 rounded bg-black/70 text-xs text-white/80 space-y-1 pointer-events-none">
      {lines.map((line, i) => (
        <div key={i}>{line}</div>
      ))}
    </div>
  );
}
