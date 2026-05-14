import { useEffect } from "react";
import type { SubjectSharpnessVerdict } from "../../types/benchmark";
import { SUBJECT_SHARPNESS_VERDICTS } from "../../types/benchmark";

interface Bindings {
  enabled: boolean;
  onPrevPhoto: () => void;
  onNextPhoto: () => void;
  onPrevFace: () => void;
  onNextFace: () => void;
  onToggleDetection: () => void;
  onToggleLandmark: () => void;
  onToggleLeftEye: () => void;
  onToggleRightEye: () => void;
  onToggleSmile: () => void;
  onToggleSpecies: () => void;
  onSharpnessVerdict: (v: SubjectSharpnessVerdict) => void;
}

/// Benchmark-only keyboard map. Kept local rather than threaded into
/// `useKeyboardNav.ts` because:
///   1. the benchmark page is dev-only and not part of the main view
///      switch (`useKeyboardNav.ts` keys off `currentView` from
///      projectStore, which this page never sets),
///   2. the binding set is small and self-contained,
///   3. keeping it local makes removing the temporary tool a one-folder
///      delete with no edits to the main keyboard hook.
///
/// Bindings (mirrored in the footer chip + per-button kbd labels):
///   `Y`        toggle detection-correct on current face
///   `P`        toggle landmark placement (eyes vs. eyebrows)
///   `L` / `R`  toggle left / right eye (hidden on mock provider)
///   `S`        toggle smile (hidden on mock provider)
///   `C`        toggle species (cat vs human)
///   `[` / `]`  prev / next face
///   `Space`    next photo
///   `Shift+Space` prev photo
///   `1`-`5`    sharpness verdicts in the order listed in
///              `SUBJECT_SHARPNESS_VERDICTS`
export function useBenchmarkKeyboard(b: Bindings) {
  useEffect(() => {
    if (!b.enabled) return;
    const handler = (e: KeyboardEvent) => {
      // Skip when the focus is in a form field — the notes textarea
      // and the set-name input both live on adjacent screens that may
      // be re-entered.
      const target = e.target as HTMLElement | null;
      if (
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.isContentEditable
      ) {
        return;
      }

      // Sharpness verdict 1-5 (digit row, ignored when modifier is held
      // so browser shortcuts still work).
      if (!e.ctrlKey && !e.metaKey && !e.altKey) {
        const digit = parseInt(e.key, 10);
        if (digit >= 1 && digit <= SUBJECT_SHARPNESS_VERDICTS.length) {
          e.preventDefault();
          b.onSharpnessVerdict(SUBJECT_SHARPNESS_VERDICTS[digit - 1]);
          return;
        }
      }

      switch (e.key) {
        case " ":
          e.preventDefault();
          if (e.shiftKey) b.onPrevPhoto();
          else b.onNextPhoto();
          return;
        case "[":
          e.preventDefault();
          b.onPrevFace();
          return;
        case "]":
          e.preventDefault();
          b.onNextFace();
          return;
      }

      // Case-insensitive letter bindings.
      const lower = e.key.toLowerCase();
      switch (lower) {
        case "y":
          e.preventDefault();
          b.onToggleDetection();
          break;
        case "p":
          e.preventDefault();
          b.onToggleLandmark();
          break;
        case "l":
          e.preventDefault();
          b.onToggleLeftEye();
          break;
        case "r":
          e.preventDefault();
          b.onToggleRightEye();
          break;
        case "s":
          e.preventDefault();
          b.onToggleSmile();
          break;
        case "c":
          e.preventDefault();
          b.onToggleSpecies();
          break;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [b]);
}
