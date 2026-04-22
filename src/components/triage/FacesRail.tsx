import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../../stores/projectStore";
import { useAiStore, sharpnessBadgeScore } from "../../stores/aiStore";
import { FaceThumb } from "../FaceThumb";
import { ExifChip, ScoreBar } from "../primitives";
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

function verdictMeta(v: Verdict): { label: string; tone: string; symbol: string } {
  if (v === "keep") return { label: "keep", tone: "var(--color-success)", symbol: "✓" };
  if (v === "blink") return { label: "blink", tone: "var(--color-warning)", symbol: "◑" };
  return { label: "blur", tone: "var(--color-danger)", symbol: "⌀" };
}

function noteFor(image: ImageEntry, faces: Face[] | null): string {
  if (!faces) return "";
  if (faces.length === 0) return "No faces in this frame.";
  const blinks = faces.filter((f) => verdictFor(f) === "blink").length;
  const blurs = faces.filter((f) => verdictFor(f) === "blur").length;
  const sharp = image.sharpnessScore ?? 0;
  const bits: string[] = [];
  if (sharp >= 80) bits.push(`Sharp frame (${Math.round(sharp)}/100)`);
  else if (sharp > 0) bits.push(`Sharpness ${Math.round(sharp)}/100`);
  if (blinks > 0) bits.push(`${blinks} blink${blinks === 1 ? "" : "s"} detected`);
  if (blurs > 0) bits.push(`${blurs} soft face${blurs === 1 ? "" : "s"}`);
  return bits.join(" · ");
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

export function FacesRail() {
  const currentImage = useProjectStore((s) => {
    const item = s.displayItems[s.currentIndex];
    return item?.image ?? null;
  });
  const provider = useAiStore((s) => s.provider);
  const eyeProvider = useAiStore((s) => s.eyeProvider);
  const mouthProvider = useAiStore((s) => s.mouthProvider);
  const percentiles = useAiStore((s) => s.percentiles);
  const [faces, setFaces] = useState<Face[] | null>(null);

  const photoId = currentImage?.id ?? null;
  const analyzedAt = currentImage?.aiAnalyzedAt ?? null;

  useEffect(() => {
    if (photoId === null) {
      setFaces(null);
      return;
    }
    if (!analyzedAt) {
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

  const sharpness = currentImage?.sharpnessScore ?? 0;
  const badge = useMemo(() => sharpnessBadgeScore(sharpness, percentiles), [sharpness, percentiles]);

  if (!currentImage) return null;

  const disabled = provider === "disabled";
  const showEyes = eyeProvider === "onnx";
  const showSmile = mouthProvider === "onnx";

  const topFaceConfidence =
    faces && faces.length > 0
      ? Math.round(Math.max(...faces.map((f) => f.detectionConfidence)) * 100)
      : 0;
  const openEyeCount = currentImage.eyesOpenCount ?? 0;
  const totalEyePairs = (currentImage.faceCount ?? 0) * 2;
  const eyeScore = totalEyePairs > 0 ? Math.round((openEyeCount / totalEyePairs) * 100) : 0;
  const smileScore = Math.round((currentImage.maxSmileScore ?? 0) * 100);

  return (
    <aside
      role="complementary"
      aria-label="Faces and AI scores"
      className="shrink-0 flex flex-col overflow-hidden"
      style={{
        width: RAIL_WIDTH,
        background: "var(--color-bg)",
        borderLeft: "1px solid var(--color-border)",
      }}
    >
      <div className="px-3 py-[10px] border-b" style={{ borderColor: "var(--color-border)" }}>
        <div className="text-[11px] font-medium" style={{ color: "var(--color-fg)" }}>
          Faces
          {faces !== null && (
            <span
              className="ml-[6px] font-mono text-[10px]"
              style={{ color: "var(--color-fg-mute)" }}
            >
              · {faces.length} detected
            </span>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
        {disabled && (
          <div className="text-[11px] py-8 text-center" style={{ color: "var(--color-fg-mute)" }}>
            AI provider disabled.
          </div>
        )}
        {!disabled && !analyzedAt && (
          <div className="text-[11px] py-8 text-center" style={{ color: "var(--color-fg-mute)" }}>
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
            {noteFor(currentImage, faces)}
          </div>
        )}
      </div>

      <div
        className="shrink-0 px-3 py-[10px] border-t flex items-center justify-end"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
      >
        <ExifChip
          shutter={currentImage.shutterSpeed}
          fstop={currentImage.aperture}
          iso={currentImage.iso}
          focal={currentImage.focalLength ? `${currentImage.focalLength}mm` : null}
        />
      </div>
    </aside>
  );
}
