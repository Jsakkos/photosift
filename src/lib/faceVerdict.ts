import type { Face } from "../types";

export type FaceVerdict = "keep" | "blink" | "blur";

export function verdictFor(face: Face): FaceVerdict {
  if (face.leftEyeOpen === 0 && face.rightEyeOpen === 0) return "blink";
  const eyeSharp = (face.leftEyeSharpness + face.rightEyeSharpness) / 2;
  if (eyeSharp < 0.3) return "blur";
  return "keep";
}

export function verdictMeta(v: FaceVerdict): {
  label: string;
  tone: string;
  symbol: string;
} {
  if (v === "keep")
    return { label: "keep", tone: "var(--color-success)", symbol: "✓" };
  if (v === "blink")
    return { label: "blink", tone: "var(--color-warning)", symbol: "◑" };
  return { label: "blur", tone: "var(--color-danger)", symbol: "⌀" };
}
