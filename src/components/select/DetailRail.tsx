import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../../stores/projectStore";
import { useAiStore, sharpnessBadgeScore } from "../../stores/aiStore";
import { FaceChip } from "../FaceChip";
import { CuratorChip } from "../CuratorChip";
import { ExifChip, ScoreBar, Stars } from "../primitives";
import { verdictFor } from "../../lib/faceVerdict";
import type { Face, ImageEntry } from "../../types";

const RAIL_WIDTH = 330;
const FACE_SIZE = 120;

function noteFor(_image: ImageEntry, faces: Face[] | null, rating: number): string {
  if (rating >= 4) return `Top-tier pick (${"★".repeat(rating)}).`;
  if (!faces || faces.length === 0) return "Frame-level pick; no faces analyzed.";
  const blinks = faces.filter((f) => verdictFor(f) === "blink").length;
  if (blinks > 0) return `${blinks} blink${blinks === 1 ? "" : "s"} in this frame.`;
  const topConf = Math.max(...faces.map((f) => f.detectionConfidence));
  return `Strongest face ${Math.round(topConf * 100)}%.`;
}

export function DetailRail() {
  const current = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  const provider = useAiStore((s) => s.provider);
  const eyeProvider = useAiStore((s) => s.eyeProvider);
  const mouthProvider = useAiStore((s) => s.mouthProvider);
  const percentiles = useAiStore((s) => s.percentiles);
  const [faces, setFaces] = useState<Face[] | null>(null);

  const image = current?.image ?? null;
  const photoId = image?.id ?? null;
  const analyzedAt = image?.aiAnalyzedAt ?? null;
  const rating = Math.max(0, Math.min(5, image?.starRating ?? 0)) as 0 | 1 | 2 | 3 | 4 | 5;

  useEffect(() => {
    if (photoId === null || !analyzedAt) {
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
  }, [photoId, analyzedAt]);

  const sharpness = image?.sharpnessScore ?? 0;
  const badge = useMemo(() => sharpnessBadgeScore(sharpness, percentiles), [sharpness, percentiles]);

  if (!image) return null;

  const disabled = provider === "disabled";
  const showEyes = eyeProvider === "onnx";
  const showSmile = mouthProvider === "onnx";

  const topFaceConfidence =
    faces && faces.length > 0
      ? Math.round(Math.max(...faces.map((f) => f.detectionConfidence)) * 100)
      : 0;
  const openEyeCount = image.eyesOpenCount ?? 0;
  const totalEyePairs = (image.faceCount ?? 0) * 2;
  const eyeScore = totalEyePairs > 0 ? Math.round((openEyeCount / totalEyePairs) * 100) : 0;
  const smileScore = Math.round((image.maxSmileScore ?? 0) * 100);

  return (
    <aside
      role="complementary"
      aria-label="Photo detail"
      className="shrink-0 flex flex-col overflow-hidden"
      style={{
        width: RAIL_WIDTH,
        background: "var(--color-bg)",
        borderLeft: "1px solid var(--color-border)",
      }}
    >
      <div className="px-3 py-2.5 border-b" style={{ borderColor: "var(--color-border)" }}>
        <div className="flex items-center justify-between">
          <span
            className="text-2xs uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-mute)" }}
          >
            Rating
          </span>
          <span
            className="font-mono text-3xs uppercase tracking-[0.5px]"
            style={{ color: "var(--color-fg-mute)" }}
          >
            {rating > 0 ? `${rating}★ rated` : "unrated"}
          </span>
        </div>
        <div className="mt-1.5">
          <Stars value={rating} size={14} />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
        {disabled && (
          <div className="text-[11px] py-6 text-center" style={{ color: "var(--color-fg-mute)" }}>
            On-device AI disabled — enable it in Settings.
          </div>
        )}
        {!disabled && !analyzedAt && (
          <div className="text-[11px] py-6 text-center" style={{ color: "var(--color-fg-mute)" }}>
            Not analyzed by on-device AI yet.
          </div>
        )}
        {!disabled && analyzedAt && faces !== null && faces.length > 0 && photoId !== null && (
          <div className="grid grid-cols-2 gap-2">
            {faces.map((f, i) => (
              <FaceChip
                key={`${photoId}-${i}`}
                face={f}
                photoId={photoId}
                sizePx={FACE_SIZE}
                showEyes={showEyes}
                showSmile={showSmile}
              />
            ))}
          </div>
        )}
        {!disabled && analyzedAt && faces !== null && faces.length === 0 && (
          <div className="text-[11px]" style={{ color: "var(--color-fg-dim)" }}>
            No faces detected. Sharpness {badge}/10.
          </div>
        )}

        {!disabled && analyzedAt && (
          <div className="flex flex-col gap-1.5">
            <ScoreBar label="sharp" value={Math.round(sharpness)} tone="accent-2" />
            <ScoreBar label="face" value={topFaceConfidence} tone="accent-2" />
            {showEyes && <ScoreBar label="eye" value={eyeScore} tone="warning" />}
            {showSmile && <ScoreBar label="smile" value={smileScore} tone="accent" />}
          </div>
        )}

        {!disabled && analyzedAt && faces !== null && (
          <div
            className="text-[11px] leading-snug pt-2 border-t"
            style={{ color: "var(--color-fg-dim)", borderColor: "var(--color-border)" }}
          >
            {noteFor(image, faces, rating)}
          </div>
        )}

        <CuratorChip />
      </div>

      <div
        className="shrink-0 px-3 py-2.5 border-t flex items-center justify-end"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
      >
        <ExifChip
          shutter={image.shutterSpeed}
          fstop={image.aperture}
          iso={image.iso}
          focal={image.focalLength ? `${image.focalLength}mm` : null}
        />
      </div>
    </aside>
  );
}
