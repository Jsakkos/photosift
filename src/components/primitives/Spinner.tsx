type SpinnerProps = {
  /// Pixel diameter. Default 14 (inline-with-text size).
  size?: number;
  /// Stroke width in px. Default 2.
  thickness?: number;
  className?: string;
  "aria-label"?: string;
};

/// Indeterminate loading spinner — a spinning ring in `currentColor`, so it
/// inherits the surrounding text colour (or a `text-*` utility on the caller).
/// Use for in-flight operations: connection tests, recluster/reanalyze runs,
/// image decodes.
///
/// Reduced-motion carve-out: the global `prefers-reduced-motion: reduce`
/// rule in globals.css would otherwise freeze this spinner at 0deg, which
/// destroys the "still working" signal. Per WCAG 2.3.3 (Animation from
/// Interactions), motion that is *essential* to convey ongoing state is
/// exempt — a static disc gives the user no indication of progress, so we
/// re-enable the animation here via an inline style with the same duration.
export function Spinner({
  size = 14,
  thickness = 2,
  className,
  "aria-label": ariaLabel = "Loading",
}: SpinnerProps) {
  return (
    <span
      role="status"
      aria-label={ariaLabel}
      className={`motion-essential inline-block animate-spin rounded-full border-current border-t-transparent align-[-0.125em] ${className ?? ""}`.trim()}
      style={{ width: size, height: size, borderWidth: thickness }}
    />
  );
}
