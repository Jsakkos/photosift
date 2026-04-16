import { useProjectStore } from "../stores/projectStore";

export function MetadataOverlay() {
  const { images, currentIndex, showMetadata } = useProjectStore();
  if (!showMetadata) return null;

  const image = images[currentIndex];
  if (!image) return null;

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
  ].filter(Boolean);

  return (
    <div className="absolute top-2 right-2 px-3 py-2 rounded bg-black/70 text-xs text-white/80 space-y-1 pointer-events-none">
      {lines.map((line, i) => (
        <div key={i}>{line}</div>
      ))}
    </div>
  );
}
