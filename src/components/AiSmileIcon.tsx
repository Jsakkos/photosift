/// Compact smile indicator for a face tile. Buckets the 0.0–1.0 smile
/// score from the mouth classifier into three states: smiling (green),
/// neutral (yellow), not smiling (red). Null score is rendered as neutral
/// with an opacity drop so the tile still shows "we tried but couldn't
/// classify" rather than going blank.
///
/// Gate on `mouthProvider === "onnx"` at the call site — the mock/1-class
/// variants all return a flat 0.5 that would light up yellow for every
/// face regardless of actual expression.
///
/// Sized and unpositioned for FaceTile's bottom badge row.
export function AiSmileIcon({ smileScore }: { smileScore: number | null }) {
  const state: "smile" | "neutral" | "frown" =
    smileScore == null
      ? "neutral"
      : smileScore >= 0.6
        ? "smile"
        : smileScore <= 0.4
          ? "frown"
          : "neutral";

  const tint =
    state === "smile"
      ? "bg-green-500/90 text-white"
      : state === "frown"
        ? "bg-red-500/90 text-white"
        : "bg-yellow-500/90 text-white";

  const label =
    smileScore == null
      ? "Smile unknown"
      : `Smile confidence ${(smileScore * 100).toFixed(0)}%`;
  const title =
    smileScore == null
      ? "Smile unknown — mouth classifier couldn't score this face."
      : `${label}\nHappy-class probability from the mouth classifier.\nGreen ≥60% · Yellow 40-60% · Red ≤40%.\nMultiplies into AI pick score at half weight.`;

  const opacity = smileScore == null ? "opacity-60" : "";

  return (
    <div
      className={`${tint} ${opacity} rounded w-6 h-6 flex items-center justify-center pointer-events-auto shadow-sm`}
      aria-label={label}
      title={title}
    >
      {state === "smile" && <SmileIcon />}
      {state === "neutral" && <NeutralIcon />}
      {state === "frown" && <FrownIcon />}
    </div>
  );
}

function SmileIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <circle cx="7" cy="8" r="1.3" fill="currentColor" />
      <circle cx="13" cy="8" r="1.3" fill="currentColor" />
      <path
        d="M5 12 C7 15, 13 15, 15 12"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        fill="none"
      />
    </svg>
  );
}

function NeutralIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <circle cx="7" cy="8" r="1.3" fill="currentColor" />
      <circle cx="13" cy="8" r="1.3" fill="currentColor" />
      <path
        d="M6 13 L14 13"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
      />
    </svg>
  );
}

function FrownIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <circle cx="7" cy="8" r="1.3" fill="currentColor" />
      <circle cx="13" cy="8" r="1.3" fill="currentColor" />
      <path
        d="M5 14 C7 11, 13 11, 15 14"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        fill="none"
      />
    </svg>
  );
}
