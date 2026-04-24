import { FaceThumb } from "./FaceThumb";
import { verdictFor, verdictMeta } from "../lib/faceVerdict";
import type { Face } from "../types";

function EyeMark({
  open,
  sharpness,
  label,
}: {
  open: boolean;
  sharpness: number;
  label: string;
}) {
  // Data comes through on a 0-100 scale in practice (the 0-1 doc comment
  // on the Face type is stale). Show more precision for values < 1 to
  // stay legible if that ever changes upstream.
  const formatted = sharpness < 1 ? sharpness.toFixed(2) : Math.round(sharpness).toString();
  return (
    <span className="inline-flex items-center gap-[3px]">
      <span style={{ color: "var(--color-fg-mute)" }}>{label}</span>
      <span style={{ color: open ? "var(--color-success)" : "var(--color-danger)" }}>
        {open ? "●" : "○"}
      </span>
      <span style={{ opacity: 0.7 }}>{formatted}</span>
    </span>
  );
}

export function FaceChip({
  face,
  photoId,
  sizePx,
  showEyes = false,
  showSmile = false,
}: {
  face: Face;
  photoId: number;
  sizePx: number;
  showEyes?: boolean;
  showSmile?: boolean;
}) {
  const verdict = verdictFor(face);
  const meta = verdictMeta(verdict);
  const conf = Math.round(face.detectionConfidence * 100);
  const smile =
    face.smileScore !== null && face.smileScore !== undefined
      ? Math.round(face.smileScore * 100)
      : null;
  const isCat = face.species === "cat";

  return (
    <div className="flex flex-col gap-[5px]">
      <FaceThumb face={face} photoId={photoId} sizePx={sizePx} />
      <div className="flex items-center gap-[6px] font-mono text-[9px] leading-tight flex-wrap">
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
        {isCat && (
          <span style={{ color: "var(--color-fg-mute)" }} aria-label="cat">
            🐾
          </span>
        )}
      </div>
      {showEyes && (
        <div
          className="flex items-center gap-[10px] font-mono text-[9px] leading-tight"
          style={{ color: "var(--color-fg-dim)" }}
        >
          <EyeMark
            open={face.leftEyeOpen === 1}
            sharpness={face.leftEyeSharpness}
            label="L"
          />
          <EyeMark
            open={face.rightEyeOpen === 1}
            sharpness={face.rightEyeSharpness}
            label="R"
          />
        </div>
      )}
      {showSmile && smile !== null && (
        <div
          className="font-mono text-[9px] leading-tight"
          style={{ color: "var(--color-fg-dim)" }}
        >
          smile {smile}%
        </div>
      )}
    </div>
  );
}
