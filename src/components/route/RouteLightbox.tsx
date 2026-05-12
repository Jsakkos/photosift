import { useEffect } from "react";
import { imageUrl } from "../../hooks/useImageLoader";

interface Props {
  photoId: number | null;
  filename?: string;
  onClose: () => void;
}

/// Modal photo viewer for Route. Tapping ⤢ on a pick tile opens the
/// embedded preview at fit-screen so the user can confirm a route
/// decision without leaving Route. Esc, the close button, or clicking
/// the backdrop dismisses. Zoom/pan stay scoped to LoupeView (Triage/
/// Select); this is a quick-look overlay, not a second loupe.
export function RouteLightbox({ photoId, filename, onClose }: Props) {
  useEffect(() => {
    if (photoId === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [photoId, onClose]);

  if (photoId === null) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={filename ?? "Photo preview"}
      onClick={onClose}
      className="fixed inset-0 z-[200] flex items-center justify-center cursor-zoom-out"
      style={{ background: "rgba(0,0,0,0.85)", backdropFilter: "blur(4px)" }}
    >
      <img
        src={imageUrl(photoId)}
        alt={filename ?? ""}
        className="max-w-[95vw] max-h-[92vh] object-contain"
        onClick={(e) => e.stopPropagation()}
      />
      {filename && (
        <div
          className="absolute bottom-6 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md font-mono text-[11px] tabular-nums"
          style={{
            background: "rgba(0,0,0,0.7)",
            color: "rgba(255,255,255,0.95)",
          }}
        >
          {filename}
        </div>
      )}
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        aria-label="Close preview"
        className="absolute top-4 right-4 w-9 h-9 rounded-full text-xl leading-none flex items-center justify-center cursor-pointer border-0"
        style={{
          background: "rgba(0,0,0,0.6)",
          color: "rgba(255,255,255,0.95)",
        }}
      >
        ×
      </button>
    </div>
  );
}
