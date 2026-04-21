/// A tiny (6px) eye-state dot for filmstrip thumbnails.
///
/// Unlike `AiEyeIcon` (which is per-face, sized for the AI panel), this
/// summarises the *whole photo*: green if every detected eye is open,
/// red if none, yellow if mixed. Null when there are no faces or the
/// photo hasn't been analyzed — a grayed eye would look like an error.
///
/// Callers should gate on `eyeProvider === "onnx"` so the mock
/// alternating-0-1 values don't leak through.

interface EyeStatusBadgeProps {
  faceCount: number | null | undefined;
  eyesOpenCount: number | null | undefined;
}

export function EyeStatusBadge({
  faceCount,
  eyesOpenCount,
}: EyeStatusBadgeProps) {
  if (
    typeof faceCount !== "number" ||
    typeof eyesOpenCount !== "number" ||
    faceCount <= 0
  ) {
    return null;
  }

  const totalEyes = faceCount * 2;
  let tint: string;
  let state: "open" | "closed" | "mixed";
  let label: string;
  if (eyesOpenCount >= totalEyes) {
    tint = "bg-emerald-500/90 text-white";
    state = "open";
    label = "All eyes open";
  } else if (eyesOpenCount <= 0) {
    tint = "bg-red-500/90 text-white";
    state = "closed";
    label = "All eyes closed";
  } else {
    tint = "bg-yellow-500/90 text-white";
    state = "mixed";
    label = `${eyesOpenCount}/${totalEyes} eyes open`;
  }

  const title = `${label}\nWhole-photo eye summary across all detected faces.\nGreen = all open · Yellow = mixed · Red = all closed.\nPer-face detail in the AI panel.`;

  return (
    <div
      className={`absolute bottom-1 left-1 ${tint} rounded w-5 h-5 flex items-center justify-center pointer-events-auto shadow-sm`}
      aria-label={label}
      title={title}
    >
      {state === "open" && <EyeOpenGlyph />}
      {state === "closed" && <EyeClosedGlyph />}
      {state === "mixed" && <EyeMixedGlyph />}
    </div>
  );
}

function EyeOpenGlyph() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path d="M1 10 C4 4, 16 4, 19 10 C16 16, 4 16, 1 10 Z" stroke="currentColor" strokeWidth="1.5" fill="none" />
      <circle cx="10" cy="10" r="3" fill="currentColor" />
    </svg>
  );
}

function EyeClosedGlyph() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path d="M1 12 C5 7, 15 7, 19 12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" fill="none" />
    </svg>
  );
}

function EyeMixedGlyph() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path d="M1 10 C4 6, 16 6, 19 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" fill="none" />
      <circle cx="10" cy="9" r="2" fill="currentColor" />
    </svg>
  );
}
