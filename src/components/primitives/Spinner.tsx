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
      className={`inline-block animate-spin rounded-full border-current border-t-transparent align-[-0.125em] ${className ?? ""}`.trim()}
      style={{ width: size, height: size, borderWidth: thickness }}
    />
  );
}
