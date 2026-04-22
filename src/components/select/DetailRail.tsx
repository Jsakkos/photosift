import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../../stores/projectStore";
import { useAiStore, sharpnessBadgeScore } from "../../stores/aiStore";
import { FaceThumb } from "../FaceThumb";
import { ExifChip, ScoreBar, Stars, ColorLabelRow } from "../primitives";
import type { Face, ImageEntry } from "../../types";

const RAIL_WIDTH = 220;
const FACE_SIZE = 86;

type Verdict = "keep" | "blink" | "blur";

function verdictFor(face: Face): Verdict {
  if (face.leftEyeOpen === 0 && face.rightEyeOpen === 0) return "blink";
  const eyeSharp = (face.leftEyeSharpness + face.rightEyeSharpness) / 2;
  if (eyeSharp < 0.3) return "blur";
  return "keep";
}

function verdictMeta(v: Verdict): { tone: string; symbol: string; label: string } {
  if (v === "keep") return { tone: "var(--color-success)", symbol: "✓", label: "keep" };
  if (v === "blink") return { tone: "var(--color-warning)", symbol: "◑", label: "blink" };
  return { tone: "var(--color-danger)", symbol: "⌀", label: "blur" };
}

function noteFor(_image: ImageEntry, faces: Face[] | null, rating: number): string {
  if (rating >= 4) return `Top-tier pick (${"★".repeat(rating)}).`;
  if (!faces || faces.length === 0) return "Frame-level pick; no faces analyzed.";
  const blinks = faces.filter((f) => verdictFor(f) === "blink").length;
  if (blinks > 0) return `${blinks} blink${blinks === 1 ? "" : "s"} in this frame.`;
  const topConf = Math.max(...faces.map((f) => f.detectionConfidence));
  return `Strongest face ${Math.round(topConf * 100)}%.`;
}

function FaceChip({ face, photoId }: { face: Face; photoId: number }) {
  const verdict = verdictFor(face);
  const meta = verdictMeta(verdict);
  const conf = Math.round(face.detectionConfidence * 100);
  return (
    <div className="flex flex-col gap-[6px]">
      <FaceThumb face={face} photoId={photoId} sizePx={FACE_SIZE} />
      <div className="flex items-center gap-[6px] font-mono text-[9px] leading-tight">
        <span
          className="inline-flex items-center gap-[3px] px-[5px] py-[2px] rounded-xs"
          style={{
            color: meta.tone,
            background: "rgba(255,255,255,0.04)",
            border: `1px solid ${meta.tone}`,
          }}
        >
          <span>{meta.symbol}</span>
          <span className="uppercase tracking-[0.5px]">{meta.label}</span>
        </span>
        <span style={{ color: "var(--color-fg-dim)" }}>{conf}%</span>
      </div>
    </div>
  );
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
      <div className="px-3 py-[10px] border-b" style={{ borderColor: "var(--color-border)" }}>
        <div className="flex items-center justify-between">
          <span
            className="text-[10px] uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-mute)" }}
          >
            Rating
          </span>
          <span
            className="font-mono text-[9px] uppercase tracking-[0.5px]"
            style={{ color: "var(--color-fg-mute)" }}
          >
            {rating > 0 ? `${rating}★ rated` : "unrated"}
          </span>
        </div>
        <div className="mt-[6px]">
          <Stars value={rating} size={14} />
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
        {disabled && (
          <div className="text-[11px] py-6 text-center" style={{ color: "var(--color-fg-mute)" }}>
            AI disabled.
          </div>
        )}
        {!disabled && !analyzedAt && (
          <div className="text-[11px] py-6 text-center" style={{ color: "var(--color-fg-mute)" }}>
            Not analyzed yet.
          </div>
        )}
        {!disabled && analyzedAt && faces !== null && faces.length > 0 && photoId !== null && (
          <div className="grid grid-cols-2 gap-2">
            {faces.map((f, i) => (
              <FaceChip key={`${photoId}-${i}`} face={f} photoId={photoId} />
            ))}
          </div>
        )}
        {!disabled && analyzedAt && faces !== null && faces.length === 0 && (
          <div className="text-[11px]" style={{ color: "var(--color-fg-dim)" }}>
            No faces detected. Sharpness {badge}/10.
          </div>
        )}

        {!disabled && analyzedAt && (
          <div className="flex flex-col gap-[6px]">
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

        {/* Color label row — presentational only until the schema adds a
            `photos.color_label` column + `set_color_label` Tauri command.
            Docs: see plan file, "Open questions / follow-ups" section. */}
        <div className="pt-2 border-t" style={{ borderColor: "var(--color-border)" }}>
          <div
            className="text-[10px] uppercase tracking-[0.6px] mb-2"
            style={{ color: "var(--color-fg-mute)" }}
          >
            Color label
          </div>
          <ColorLabelRow value={null} />
        </div>
      </div>

      <div
        className="shrink-0 px-3 py-[10px] border-t flex items-center justify-end"
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
