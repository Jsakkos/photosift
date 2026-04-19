import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../stores/projectStore";
import { useAiStore, sharpnessBadgeScore } from "../stores/aiStore";
import { FaceThumb } from "./FaceThumb";
import { AiSharpnessBadge } from "./AiSharpnessBadge";
import { AiEyeIcon } from "./AiEyeIcon";
import type { Face } from "../types";

const TILE_PX = 160;
const MAX_VISIBLE = 6;

interface Props {
  photoId: number;
  visible: boolean;
}

export function AiPanel({ photoId, visible }: Props) {
  const image = useProjectStore((s) =>
    s.images.find((i) => i.id === photoId) ?? null,
  );
  const provider = useAiStore((s) => s.provider);
  const eyeProvider = useAiStore((s) => s.eyeProvider);
  const percentiles = useAiStore((s) => s.percentiles);
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

  const badge = sharpnessBadgeScore(image.sharpnessScore, percentiles);
  const showEyes = eyeProvider === "onnx";
  const visibleFaces = faces ? faces.slice(0, MAX_VISIBLE) : [];
  const overflow = faces ? Math.max(0, faces.length - MAX_VISIBLE) : 0;
  const hasFaces = !!faces && faces.length > 0;

  return (
    <div
      role="complementary"
      aria-label="AI analysis panel"
      className="flex flex-col gap-2 p-2 text-[12px] text-[var(--text-primary)] overflow-y-auto"
    >
      {hasFaces && (
        <div
          className={`grid gap-2 ${
            visibleFaces.length + (overflow > 0 ? 1 : 0) === 1
              ? "grid-cols-1"
              : "grid-cols-2"
          }`}
        >
          {visibleFaces.map((f, i) => (
            <FaceTile
              key={i}
              face={f}
              photoId={photoId}
              sharpnessScore={badge}
              showEyes={showEyes}
            />
          ))}
          {overflow > 0 && (
            <div
              className="flex items-center justify-center rounded bg-black/40 text-[var(--text-secondary)] text-[11px]"
              style={{ width: TILE_PX, height: TILE_PX }}
              aria-label={`${overflow} more faces not shown`}
            >
              +{overflow} more
            </div>
          )}
        </div>
      )}
      {!hasFaces && (
        <div className="text-center text-[var(--text-secondary)] text-[11px] py-8">
          No faces detected
          <div className="mt-1 text-[10px]">Sharpness {badge}/10</div>
        </div>
      )}
    </div>
  );
}

function FaceTile({
  face,
  photoId,
  sharpnessScore,
  showEyes,
}: {
  face: Face;
  photoId: number;
  sharpnessScore: number;
  showEyes: boolean;
}) {
  return (
    <div className="relative" style={{ width: TILE_PX, height: TILE_PX }}>
      <FaceThumb face={face} photoId={photoId} sizePx={TILE_PX} />
      {showEyes && (
        <AiEyeIcon
          leftOpen={face.leftEyeOpen === 1}
          rightOpen={face.rightEyeOpen === 1}
        />
      )}
      <AiSharpnessBadge score={sharpnessScore} />
    </div>
  );
}
