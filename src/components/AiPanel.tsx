import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../stores/projectStore";
import { useAiStore } from "../stores/aiStore";
import { thumbUrl } from "../hooks/useImageLoader";
import type { Face } from "../types";

interface Props {
  photoId: number;
  visible: boolean;
}

export function AiPanel({ photoId, visible }: Props) {
  const image = useProjectStore((s) =>
    s.images.find((i) => i.id === photoId) ?? null,
  );
  const provider = useAiStore((s) => s.provider);
  const [faces, setFaces] = useState<Face[] | null>(null);

  useEffect(() => {
    if (!visible || !image?.aiAnalyzedAt) {
      setFaces(null);
      return;
    }
    let cancelled = false;
    invoke<Face[]>("get_faces_for_photo", { photoId })
      .then((f) => {
        if (!cancelled) setFaces(f);
      })
      .catch(() => {
        if (!cancelled) setFaces([]);
      });
    return () => {
      cancelled = true;
    };
  }, [photoId, visible, image?.aiAnalyzedAt]);

  if (provider === "disabled") return null;
  if (!visible) return null;
  if (!image?.aiAnalyzedAt) return null;

  const faceCount = image.faceCount ?? 0;
  // If the photo has zero faces AND the panel wasn't force-opened, hide.
  // The caller's `visible` prop already mixes force-open in, so reaching
  // here with faceCount=0 means the user explicitly pressed F.
  const sharpness = image.sharpnessScore ?? 0;

  return (
    <div
      role="complementary"
      aria-label="AI analysis panel"
      className="absolute top-3 right-3 w-[220px] bg-[rgba(20,20,20,0.92)] border border-white/10 rounded-md p-2 backdrop-blur-sm text-[11px] text-[var(--text-primary)] z-10 pointer-events-auto"
    >
      <div className="flex items-center justify-between mb-2 text-[10px] uppercase tracking-wider text-[var(--text-secondary)]">
        <span>
          {faceCount} {faceCount === 1 ? "face" : "faces"}
        </span>
        <span>sharp {Math.round(sharpness)}</span>
      </div>
      {faces && faces.length === 0 && faceCount === 0 && (
        <div className="text-[10px] text-[var(--text-secondary)] py-1">
          No faces detected.
        </div>
      )}
      {faces &&
        faces.slice(0, 3).map((f, i) => (
          <FaceRow key={i} face={f} photoId={photoId} />
        ))}
      {faces && faces.length > 3 && (
        <div className="text-[10px] text-[var(--text-secondary)] mt-1">
          +{faces.length - 3} more
        </div>
      )}
    </div>
  );
}

function FaceRow({ face, photoId }: { face: Face; photoId: number }) {
  const lOpen = face.leftEyeOpen === 1;
  const rOpen = face.rightEyeOpen === 1;
  const borderClass = (open: boolean) =>
    open ? "border-green-500" : "border-dashed border-red-500";

  return (
    <div className="flex items-center gap-2 mb-1.5 bg-black/30 rounded p-1">
      <div className="w-10 h-10 relative overflow-hidden rounded bg-black/40 flex-shrink-0">
        <img
          src={thumbUrl(photoId)}
          alt=""
          aria-hidden="true"
          className="absolute object-none"
          style={{
            width: `${100 / face.bboxW}%`,
            height: `${100 / face.bboxH}%`,
            left: `${(-face.bboxX * 100) / face.bboxW}%`,
            top: `${(-face.bboxY * 100) / face.bboxH}%`,
          }}
        />
      </div>
      <div className="flex-1">
        <div className="flex gap-1">
          <div
            className={`w-5 h-3 rounded-sm border ${borderClass(lOpen)} bg-black/30`}
            aria-label={lOpen ? "Left eye open" : "Left eye closed"}
          />
          <div
            className={`w-5 h-3 rounded-sm border ${borderClass(rOpen)} bg-black/30`}
            aria-label={rOpen ? "Right eye open" : "Right eye closed"}
          />
        </div>
        <div className="text-[9px] text-[var(--text-secondary)] mt-0.5">
          {Math.round(face.leftEyeSharpness)} · {Math.round(face.rightEyeSharpness)}
        </div>
      </div>
    </div>
  );
}
