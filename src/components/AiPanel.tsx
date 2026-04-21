import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../stores/projectStore";
import { useAiStore, sharpnessBadgeScore } from "../stores/aiStore";
import { FaceThumb } from "./FaceThumb";
import { AiSharpnessBadge } from "./AiSharpnessBadge";
import { AiEyeIcon } from "./AiEyeIcon";
import { AiSmileIcon } from "./AiSmileIcon";
import { AiSpeciesIcon } from "./AiSpeciesIcon";
import type { Face } from "../types";

const TILE_PX = 160;

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
  const mouthProvider = useAiStore((s) => s.mouthProvider);
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
  const showSmile = mouthProvider === "onnx";
  const hasFaces = !!faces && faces.length > 0;

  return (
    <div
      role="complementary"
      aria-label="AI analysis panel"
      className="flex flex-col gap-2 p-2 text-[12px] text-[var(--text-primary)] overflow-y-auto"
    >
      {hasFaces && (
        <div
          className={`grid gap-2 ${faces!.length === 1 ? "grid-cols-1" : "grid-cols-2"}`}
        >
          {faces!.map((f, i) => (
            <FaceTile
              key={i}
              face={f}
              photoId={photoId}
              sharpnessScore={badge}
              showEyes={showEyes}
              showSmile={showSmile}
            />
          ))}
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
  showSmile,
}: {
  face: Face;
  photoId: number;
  sharpnessScore: number;
  showEyes: boolean;
  showSmile: boolean;
}) {
  return (
    <div className="relative" style={{ width: TILE_PX, height: TILE_PX }}>
      <FaceThumb face={face} photoId={photoId} sizePx={TILE_PX} />
      {/* All per-face badges in a single bottom row. Gradient backdrop
          keeps them readable on bright face crops. Each badge is
          self-sized (24x24 square, or wider for the sharpness pill).
          Tooltips fire on hover per badge. */}
      <div className="absolute bottom-0 left-0 right-0 px-1.5 pb-1.5 pt-4 bg-gradient-to-t from-black/75 via-black/40 to-transparent flex items-end gap-1">
        {showEyes && face.species === "human" && (
          <AiEyeIcon
            leftOpen={face.leftEyeOpen === 1}
            rightOpen={face.rightEyeOpen === 1}
          />
        )}
        {showSmile && face.species === "human" && (
          <AiSmileIcon smileScore={face.smileScore} />
        )}
        <AiSpeciesIcon species={face.species} />
        <div className="flex-1" />
        <AiSharpnessBadge score={sharpnessScore} />
      </div>
    </div>
  );
}
