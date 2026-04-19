/// Compact eye-state indicator for a face tile. Combines the per-eye
/// flags into one of three states: both open, both closed, or partial
/// (one open / one closed). Rendered as an inline SVG so it scales
/// crisply at icon size without a font dependency.
///
/// Only meaningful when backed by a real classifier — callers should
/// gate on `eyeProvider === "onnx"` to avoid surfacing mock values.
export function AiEyeIcon({
  leftOpen,
  rightOpen,
}: {
  leftOpen: boolean;
  rightOpen: boolean;
}) {
  const state: "open" | "closed" | "partial" =
    leftOpen && rightOpen
      ? "open"
      : !leftOpen && !rightOpen
        ? "closed"
        : "partial";

  const tint =
    state === "open"
      ? "bg-green-500/85 text-white"
      : state === "closed"
        ? "bg-red-500/85 text-white"
        : "bg-yellow-500/85 text-white";

  const label =
    state === "open"
      ? "Both eyes open"
      : state === "closed"
        ? "Both eyes closed"
        : "One eye open";

  return (
    <div
      className={`absolute bottom-1 left-1 ${tint} rounded w-5 h-5 flex items-center justify-center pointer-events-none shadow-sm`}
      aria-label={label}
      title={label}
    >
      {state === "open" && <EyeOpenIcon />}
      {state === "closed" && <EyeClosedIcon />}
      {state === "partial" && <EyePartialIcon />}
    </div>
  );
}

function EyeOpenIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path
        d="M1 10 C4 4, 16 4, 19 10 C16 16, 4 16, 1 10 Z"
        stroke="currentColor"
        strokeWidth="1.5"
        fill="none"
      />
      <circle cx="10" cy="10" r="3" fill="currentColor" />
    </svg>
  );
}

function EyeClosedIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path
        d="M1 12 C5 7, 15 7, 19 12"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        fill="none"
      />
    </svg>
  );
}

function EyePartialIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path
        d="M1 10 C4 6, 16 6, 19 10"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        fill="none"
      />
      <circle cx="10" cy="9" r="2" fill="currentColor" />
    </svg>
  );
}
