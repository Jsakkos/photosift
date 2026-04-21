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
  let bg: string;
  let label: string;
  if (eyesOpenCount >= totalEyes) {
    bg = "bg-emerald-500";
    label = "All eyes open";
  } else if (eyesOpenCount <= 0) {
    bg = "bg-red-500";
    label = "All eyes closed";
  } else {
    bg = "bg-yellow-500";
    label = `${eyesOpenCount}/${totalEyes} eyes open`;
  }

  const title = `${label}\nWhole-photo eye summary (green all open · yellow mixed · red all closed).\nPer-face detail in the AI panel.`;
  return (
    <div
      className={`absolute bottom-1 left-1 w-2 h-2 rounded-full ring-1 ring-black/40 ${bg} pointer-events-none`}
      aria-label={label}
      title={title}
    />
  );
}
