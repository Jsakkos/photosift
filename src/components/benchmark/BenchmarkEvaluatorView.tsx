import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useBenchmarkStore } from "../../stores/benchmarkStore";
import { sharpnessBadgeScore, useAiStore } from "../../stores/aiStore";
import { imageUrl } from "../../hooks/useImageLoader";
import { BenchmarkFaceOverlay } from "./BenchmarkFaceOverlay";
import { EyeCrop } from "./EyeCrop";
import { useBenchmarkKeyboard } from "./useBenchmarkKeyboard";
import type { Face, SharpnessPercentiles } from "../../types";
import type {
  BenchmarkFaceJudgment,
  SharpnessSnapshot,
  SubjectSharpnessVerdict,
} from "../../types/benchmark";
import {
  SUBJECT_SHARPNESS_LABEL,
  SUBJECT_SHARPNESS_VERDICTS,
} from "../../types/benchmark";

interface ImageMetadata {
  id: number;
  cameraModel: string | null;
  sharpnessScore: number | null;
  aiAnalyzedAt: string | null;
}

interface Props {
  onClose: () => void;
  onOpenSummary: () => void;
}

/// Three-state cycle button used for each verdict. Null → green ✓ →
/// red ✕ → null, matching the keyboard `toggleFaceVerdict` cycle.
function VerdictButton({
  value,
  label,
  onClick,
  hotkey,
}: {
  value: boolean | null;
  label: string;
  onClick: () => void;
  hotkey?: string;
}) {
  const bg =
    value === true
      ? "var(--color-success, #22c55e)"
      : value === false
        ? "var(--color-danger, #ef4444)"
        : "var(--color-bg2)";
  const fg =
    value === null ? "var(--color-fg-mute)" : "var(--color-bg)";
  const text = value === true ? "✓" : value === false ? "✕" : "—";
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex items-center justify-between gap-2 px-2 py-1 rounded-sm text-[11px] font-medium w-full"
      style={{
        background: bg,
        color: fg,
        border: "1px solid var(--color-border)",
      }}
    >
      <span>{label}</span>
      <span className="flex items-center gap-1.5">
        {hotkey && (
          <kbd
            className="font-mono text-[9px] px-1 rounded-sm"
            style={{
              background: "rgba(0,0,0,0.2)",
              color: value === null ? "var(--color-fg-mute)" : "var(--color-bg)",
            }}
          >
            {hotkey}
          </kbd>
        )}
        <span className="font-mono text-[13px]">{text}</span>
      </span>
    </button>
  );
}

export function BenchmarkEvaluatorView({ onClose, onOpenSummary }: Props) {
  const currentSet = useBenchmarkStore((s) => s.currentSet);
  const currentPhotoIndex = useBenchmarkStore((s) => s.currentPhotoIndex);
  const currentFaceIndex = useBenchmarkStore((s) => s.currentFaceIndex);
  const dirty = useBenchmarkStore((s) => s.dirty);
  const isSaving = useBenchmarkStore((s) => s.isSaving);
  const saveError = useBenchmarkStore((s) => s.saveError);

  const setPhotoIndex = useBenchmarkStore((s) => s.setPhotoIndex);
  const setFaceIndex = useBenchmarkStore((s) => s.setFaceIndex);
  const ensureFaceJudgments = useBenchmarkStore((s) => s.ensureFaceJudgments);
  const toggleFaceVerdict = useBenchmarkStore((s) => s.toggleFaceVerdict);

  // Live provider kinds — if the user has not installed the ONNX
  // classifiers, the mock providers produce deterministic
  // not-real-signal output (alternating 0/1 for eyes, constant 0.5 for
  // smile). Judging those teaches us nothing about the real models, so
  // we hide those verdict rows entirely when on mock.
  const eyeProvider = useAiStore((s) => s.eyeProvider);
  const mouthProvider = useAiStore((s) => s.mouthProvider);
  const showEyeVerdict = eyeProvider === "onnx";
  const showSmileVerdict = mouthProvider === "onnx";
  const setMissedFaceCount = useBenchmarkStore((s) => s.setMissedFaceCount);
  const setSharpnessVerdict = useBenchmarkStore((s) => s.setSharpnessVerdict);
  const setPhotoNotes = useBenchmarkStore((s) => s.setPhotoNotes);
  const setSharpnessSnapshot = useBenchmarkStore((s) => s.setSharpnessSnapshot);
  const saveSet = useBenchmarkStore((s) => s.saveSet);

  const photo = currentSet?.photos[currentPhotoIndex] ?? null;
  const photoId = photo?.photoId ?? null;
  const shootId = photo?.shootId ?? null;

  const [faces, setFaces] = useState<Face[] | null>(null);
  const [imageMeta, setImageMeta] = useState<ImageMetadata | null>(null);
  const [percentilesByShoot, setPercentilesByShoot] = useState<
    Map<number, SharpnessPercentiles>
  >(new Map());
  const [naturalDims, setNaturalDims] = useState<{ w: number; h: number } | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  // Reset image-side state on photo change.
  useEffect(() => {
    setFaces(null);
    setImageMeta(null);
    setNaturalDims(null);
    setLoadError(null);
  }, [photoId]);

  // Fetch faces + image metadata for the current photo.
  useEffect(() => {
    if (photoId === null) return;
    let cancelled = false;
    Promise.all([
      invoke<Face[]>("get_faces_for_photo", { photoId }).catch(() => [] as Face[]),
      invoke<ImageMetadata & { capture_time?: string }>("get_image_metadata", {
        imageId: photoId,
      }).catch(() => null),
    ])
      .then(([f, m]) => {
        if (cancelled) return;
        setFaces(f);
        setImageMeta(m);
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [photoId]);

  // Lazy-load per-shoot sharpness percentiles. The set may span shoots,
  // so cache them by shoot id rather than refetching on every photo.
  useEffect(() => {
    if (shootId === null) return;
    if (percentilesByShoot.has(shootId)) return;
    invoke<SharpnessPercentiles>("get_shoot_sharpness_percentiles", { shootId })
      .then((p) => {
        setPercentilesByShoot((prev) => {
          const next = new Map(prev);
          next.set(shootId, p);
          return next;
        });
      })
      .catch(() => {
        // Non-fatal: badge falls back to a raw-score heuristic in
        // sharpnessBadgeScore when percentiles are missing.
      });
  }, [shootId, percentilesByShoot]);

  const percentiles = shootId !== null ? percentilesByShoot.get(shootId) ?? null : null;

  // Compute the AI-derived signals so we can a) display them and b)
  // snapshot them onto the photo record.
  const sharpnessSignals = useMemo<SharpnessSnapshot | null>(() => {
    if (!imageMeta || !faces) return null;
    const eyeValues = faces.flatMap((f) => [f.leftEyeSharpness, f.rightEyeSharpness]);
    const maxEye = eyeValues.length > 0 ? Math.max(...eyeValues) : null;
    const meanEye =
      eyeValues.length > 0 ? eyeValues.reduce((a, b) => a + b, 0) / eyeValues.length : null;
    const badge = sharpnessBadgeScore(imageMeta.sharpnessScore ?? null, percentiles);
    return {
      globalScore: imageMeta.sharpnessScore ?? null,
      maxEyeSharpness: maxEye,
      meanEyeSharpness: meanEye,
      aiSharpnessBadge1to10: imageMeta.sharpnessScore == null ? null : badge,
    };
  }, [imageMeta, faces, percentiles]);

  // Once faces + metadata are loaded, capture the snapshot. The store
  // ignores subsequent writes so the user's verdict refers to the AI
  // signals as they were when first viewed.
  useEffect(() => {
    if (!photo || !sharpnessSignals || !faces) return;
    if (photo.sharpnessSignalsSnapshot !== null) return;
    setSharpnessSnapshot(sharpnessSignals);
  }, [photo, sharpnessSignals, faces, setSharpnessSnapshot]);

  // Ensure the photo record has one judgment slot per detected face,
  // and snapshot the landmark coords as well as the bbox.
  useEffect(() => {
    if (!faces) return;
    const bboxes: [number, number, number, number][] = faces.map((f) => [
      f.bboxX,
      f.bboxY,
      f.bboxW,
      f.bboxH,
    ]);
    const leftEyes: [number, number][] = faces.map((f) => [f.leftEyeX, f.leftEyeY]);
    const rightEyes: [number, number][] = faces.map((f) => [
      f.rightEyeX,
      f.rightEyeY,
    ]);
    ensureFaceJudgments(faces.length, bboxes, leftEyes, rightEyes);
  }, [faces, ensureFaceJudgments]);

  // Auto-save after a short debounce on dirty state. Keeps work safe
  // without spamming disk on each keypress.
  useEffect(() => {
    if (!dirty) return;
    const t = setTimeout(() => {
      void saveSet();
    }, 600);
    return () => clearTimeout(t);
  }, [dirty, saveSet]);

  const handleImageLoad = useCallback((e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    if (img.naturalWidth > 0 && img.naturalHeight > 0) {
      setNaturalDims({ w: img.naturalWidth, h: img.naturalHeight });
    }
  }, []);

  // Keyboard nav: prev/next photo, prev/next face, three-state toggles,
  // sharpness verdicts.
  useBenchmarkKeyboard({
    enabled: photo !== null,
    onNextPhoto: () => setPhotoIndex(currentPhotoIndex + 1),
    onPrevPhoto: () => setPhotoIndex(currentPhotoIndex - 1),
    onNextFace: () => setFaceIndex(currentFaceIndex + 1),
    onPrevFace: () => setFaceIndex(Math.max(0, currentFaceIndex - 1)),
    onToggleDetection: () =>
      faces && faces.length > 0 && toggleFaceVerdict(currentFaceIndex, "detectionCorrect"),
    onToggleLandmark: () =>
      faces && faces.length > 0 && toggleFaceVerdict(currentFaceIndex, "landmarkCorrect"),
    onToggleLeftEye: () =>
      faces &&
      faces.length > 0 &&
      showEyeVerdict &&
      toggleFaceVerdict(currentFaceIndex, "leftEyeCorrect"),
    onToggleRightEye: () =>
      faces &&
      faces.length > 0 &&
      showEyeVerdict &&
      toggleFaceVerdict(currentFaceIndex, "rightEyeCorrect"),
    onToggleSmile: () =>
      faces &&
      faces.length > 0 &&
      showSmileVerdict &&
      toggleFaceVerdict(currentFaceIndex, "smileCorrect"),
    onToggleSpecies: () =>
      faces && faces.length > 0 && toggleFaceVerdict(currentFaceIndex, "speciesCorrect"),
    onSharpnessVerdict: (v) => setSharpnessVerdict(v),
  });

  if (!currentSet || !photo) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <p className="text-[12px]" style={{ color: "var(--color-fg-mute)" }}>
          No set loaded.
        </p>
      </div>
    );
  }

  const total = currentSet.photos.length;
  const judgmentsForPhoto = photo.faces;

  return (
    <div className="flex flex-col h-full" style={{ background: "var(--color-bg)" }}>
      <header
        className="shrink-0 flex items-center justify-between px-3 py-2 border-b"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={onClose}
            className="text-[11px] font-mono"
            style={{ color: "var(--color-fg-mute)" }}
          >
            ← Sets
          </button>
          <span className="text-[12px] font-medium" style={{ color: "var(--color-fg)" }}>
            {currentSet.set.name}
          </span>
          <span className="font-mono text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
            {currentPhotoIndex + 1} / {total}
          </span>
          {imageMeta?.cameraModel && (
            <span
              className="font-mono text-2xs px-1.5 py-0.5 rounded-sm"
              style={{
                background: "var(--color-bg2)",
                color: "var(--color-fg-mute)",
              }}
            >
              {imageMeta.cameraModel}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2 text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
          {dirty ? (
            <span>● unsaved</span>
          ) : isSaving ? (
            <span>saving…</span>
          ) : (
            <span>saved</span>
          )}
          {saveError && (
            <span style={{ color: "var(--color-danger)" }}>{saveError}</span>
          )}
          <button
            type="button"
            onClick={onOpenSummary}
            className="px-2 py-1 rounded-sm text-[11px]"
            style={{
              background: "var(--color-bg2)",
              color: "var(--color-fg)",
              border: "1px solid var(--color-border)",
            }}
          >
            Summary
          </button>
        </div>
      </header>

      <div className="flex flex-1 overflow-hidden">
        <main className="relative flex-1 bg-black overflow-hidden">
          {photoId !== null && (
            <img
              src={imageUrl(photoId)}
              alt=""
              className="w-full h-full object-contain"
              draggable={false}
              onLoad={handleImageLoad}
            />
          )}
          {faces && (
            <BenchmarkFaceOverlay
              faces={faces}
              judgments={judgmentsForPhoto}
              selectedFaceIndex={currentFaceIndex}
              naturalWidth={naturalDims?.w ?? null}
              naturalHeight={naturalDims?.h ?? null}
              onSelectFace={setFaceIndex}
            />
          )}
          {loadError && (
            <div
              className="absolute bottom-3 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-md text-xs"
              style={{
                background: "var(--color-bg2)",
                color: "var(--color-danger)",
                border: "1px solid var(--color-danger)",
              }}
            >
              {loadError}
            </div>
          )}
        </main>

        <aside
          className="shrink-0 flex flex-col overflow-hidden"
          style={{
            width: 320,
            background: "var(--color-bg)",
            borderLeft: "1px solid var(--color-border)",
          }}
        >
          <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-3">
            <section>
              <h2
                className="text-[11px] font-medium uppercase tracking-[0.5px] mb-2"
                style={{ color: "var(--color-fg-mute)" }}
              >
                Faces · {faces?.length ?? 0}
              </h2>
              {faces === null && (
                <p className="text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
                  Loading…
                </p>
              )}
              {faces && faces.length === 0 && (
                <p className="text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
                  YuNet detected no faces in this frame.
                </p>
              )}
              {faces &&
                faces.map((f, i) => {
                  const j =
                    judgmentsForPhoto.find((jj) => jj.faceIndex === i) ??
                    (null as BenchmarkFaceJudgment | null);
                  const selected = i === currentFaceIndex;
                  return (
                    <div
                      key={i}
                      className="rounded-md p-2 mb-2"
                      style={{
                        background: selected
                          ? "var(--color-bg3)"
                          : "var(--color-bg2)",
                        border: selected
                          ? "1px solid rgb(59, 130, 246)"
                          : "1px solid var(--color-border)",
                      }}
                      onClick={() => setFaceIndex(i)}
                    >
                      <div className="flex items-center justify-between mb-1.5">
                        <span
                          className="text-[12px] font-medium"
                          style={{ color: "var(--color-fg)" }}
                        >
                          Face #{i + 1}
                        </span>
                        <span
                          className="font-mono text-2xs"
                          style={{ color: "var(--color-fg-mute)" }}
                        >
                          {Math.round(f.detectionConfidence * 100)}% · {f.species}
                        </span>
                      </div>

                      {/* The exact pixel patches the eye classifier sees.
                          If you see eyebrow/skin here, the landmark is
                          wrong — judge `Landmark` below as ✕ regardless
                          of what the classifier said. */}
                      <div className="flex items-center gap-2 mb-1.5">
                        <EyeCrop
                          photoId={photoId!}
                          eyeXNormalized={f.leftEyeX}
                          eyeYNormalized={f.leftEyeY}
                          bboxWNormalized={f.bboxW}
                          sizePx={48}
                          label="Left-eye crop (15% of face width, what the classifier sees)"
                        />
                        <EyeCrop
                          photoId={photoId!}
                          eyeXNormalized={f.rightEyeX}
                          eyeYNormalized={f.rightEyeY}
                          bboxWNormalized={f.bboxW}
                          sizePx={48}
                          label="Right-eye crop"
                        />
                        <span
                          className="font-mono text-2xs leading-tight ml-1"
                          style={{ color: "var(--color-fg-mute)" }}
                        >
                          Eye crops
                          <br />
                          (15% face)
                        </span>
                      </div>

                      <div className="flex flex-col gap-1.5">
                        <VerdictButton
                          value={j?.detectionCorrect ?? null}
                          label="Detection"
                          hotkey={selected ? "Y" : undefined}
                          onClick={() => toggleFaceVerdict(i, "detectionCorrect")}
                        />
                        <VerdictButton
                          value={j?.landmarkCorrect ?? null}
                          label="Landmark on eyes"
                          hotkey={selected ? "P" : undefined}
                          onClick={() => toggleFaceVerdict(i, "landmarkCorrect")}
                        />
                        {showEyeVerdict ? (
                          <div className="grid grid-cols-2 gap-1.5">
                            <VerdictButton
                              value={j?.leftEyeCorrect ?? null}
                              label={`L eye ${f.leftEyeOpen ? "○ open" : "— closed"}`}
                              hotkey={selected ? "L" : undefined}
                              onClick={() => toggleFaceVerdict(i, "leftEyeCorrect")}
                            />
                            <VerdictButton
                              value={j?.rightEyeCorrect ?? null}
                              label={`R eye ${f.rightEyeOpen ? "○ open" : "— closed"}`}
                              hotkey={selected ? "R" : undefined}
                              onClick={() => toggleFaceVerdict(i, "rightEyeCorrect")}
                            />
                          </div>
                        ) : (
                          <div
                            className="text-[10px] italic px-2 py-1 rounded-sm"
                            style={{
                              background: "var(--color-bg)",
                              color: "var(--color-fg-mute)",
                              border: "1px dashed var(--color-border)",
                            }}
                          >
                            Eye state: <span className="font-mono">mock</span> — drop{" "}
                            <span className="font-mono">eye_state.onnx</span> into{" "}
                            <span className="font-mono">~/.photosift-dev/models/</span> and restart to benchmark.
                          </div>
                        )}
                        <div className="grid grid-cols-2 gap-1.5">
                          {showSmileVerdict ? (
                            <VerdictButton
                              value={j?.smileCorrect ?? null}
                              label={`Smile ${
                                f.smileScore == null
                                  ? "—"
                                  : Math.round(f.smileScore * 100) + "%"
                              }`}
                              hotkey={selected ? "S" : undefined}
                              onClick={() => toggleFaceVerdict(i, "smileCorrect")}
                            />
                          ) : (
                            <div
                              className="text-[10px] italic px-2 py-1 rounded-sm flex items-center"
                              style={{
                                background: "var(--color-bg)",
                                color: "var(--color-fg-mute)",
                                border: "1px dashed var(--color-border)",
                              }}
                            >
                              Smile: mock (0.5)
                            </div>
                          )}
                          <VerdictButton
                            value={j?.speciesCorrect ?? null}
                            label="Species"
                            hotkey={selected ? "C" : undefined}
                            onClick={() => toggleFaceVerdict(i, "speciesCorrect")}
                          />
                        </div>
                      </div>
                    </div>
                  );
                })}
            </section>

            <section>
              <h2
                className="text-[11px] font-medium uppercase tracking-[0.5px] mb-2"
                style={{ color: "var(--color-fg-mute)" }}
              >
                Missed faces
              </h2>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setMissedFaceCount(photo.missedFaceCount - 1)}
                  className="px-2 py-1 rounded-sm text-[11px]"
                  style={{
                    background: "var(--color-bg2)",
                    color: "var(--color-fg)",
                    border: "1px solid var(--color-border)",
                  }}
                >
                  −
                </button>
                <span
                  className="font-mono text-[12px] min-w-[2ch] text-center"
                  style={{ color: "var(--color-fg)" }}
                >
                  {photo.missedFaceCount}
                </span>
                <button
                  type="button"
                  onClick={() => setMissedFaceCount(photo.missedFaceCount + 1)}
                  className="px-2 py-1 rounded-sm text-[11px]"
                  style={{
                    background: "var(--color-bg2)",
                    color: "var(--color-fg)",
                    border: "1px solid var(--color-border)",
                  }}
                >
                  +
                </button>
                <span className="text-[10px]" style={{ color: "var(--color-fg-mute)" }}>
                  Faces YuNet didn't detect
                </span>
              </div>
            </section>

            <section>
              <h2
                className="text-[11px] font-medium uppercase tracking-[0.5px] mb-2"
                style={{ color: "var(--color-fg-mute)" }}
              >
                Sharpness verdict
              </h2>
              <div className="flex flex-wrap gap-1.5">
                {SUBJECT_SHARPNESS_VERDICTS.map((v, i) => {
                  const active = photo.subjectSharpnessVerdict === v;
                  return (
                    <button
                      key={v}
                      type="button"
                      onClick={() =>
                        setSharpnessVerdict(
                          active ? null : (v as SubjectSharpnessVerdict),
                        )
                      }
                      className="px-2 py-1 rounded-sm text-[11px] flex items-center gap-1"
                      style={{
                        background: active
                          ? "var(--color-accent)"
                          : "var(--color-bg2)",
                        color: active ? "var(--color-bg)" : "var(--color-fg)",
                        border: "1px solid var(--color-border)",
                      }}
                    >
                      <kbd
                        className="font-mono text-[9px] px-1 rounded-sm"
                        style={{
                          background: "rgba(0,0,0,0.15)",
                          color: active ? "var(--color-bg)" : "var(--color-fg-mute)",
                        }}
                      >
                        {i + 1}
                      </kbd>
                      {SUBJECT_SHARPNESS_LABEL[v]}
                    </button>
                  );
                })}
              </div>
              {sharpnessSignals && (
                <div
                  className="mt-2 font-mono text-[10px]"
                  style={{ color: "var(--color-fg-mute)" }}
                >
                  Global {sharpnessSignals.globalScore?.toFixed(1) ?? "—"} ·{" "}
                  Eye max {sharpnessSignals.maxEyeSharpness?.toFixed(1) ?? "—"} ·{" "}
                  Eye mean {sharpnessSignals.meanEyeSharpness?.toFixed(1) ?? "—"} ·{" "}
                  Badge {sharpnessSignals.aiSharpnessBadge1to10 ?? "—"}/10
                </div>
              )}
            </section>

            <section>
              <h2
                className="text-[11px] font-medium uppercase tracking-[0.5px] mb-2"
                style={{ color: "var(--color-fg-mute)" }}
              >
                Notes
              </h2>
              <textarea
                value={photo.notes}
                onChange={(e) => setPhotoNotes(e.target.value)}
                placeholder="Optional context"
                rows={2}
                className="w-full px-2 py-1 text-[11px] rounded-sm font-mono"
                style={{
                  background: "var(--color-bg2)",
                  color: "var(--color-fg)",
                  border: "1px solid var(--color-border)",
                  resize: "vertical",
                }}
              />
            </section>
          </div>

          <footer
            className="shrink-0 px-3 py-2 border-t flex items-center justify-between text-[10px]"
            style={{ borderColor: "var(--color-border)", color: "var(--color-fg-mute)" }}
          >
            <span>
              <kbd className="font-mono">[</kbd> <kbd className="font-mono">]</kbd> face ·{" "}
              <kbd className="font-mono">Space</kbd> next photo ·{" "}
              <kbd className="font-mono">⇧Space</kbd> prev
            </span>
          </footer>
        </aside>
      </div>
    </div>
  );
}
