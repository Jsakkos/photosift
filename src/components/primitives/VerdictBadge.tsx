export type Verdict = "keep" | "toss" | null;

/// Tiny 12px corner badge that signals a photo's keep/toss decision in
/// the same visual language across every thumbnail surface (Photo,
/// GridThumb, etc.). Positions absolutely in the top-right of its
/// containing positioned ancestor; null verdict renders nothing.
export function VerdictBadge({ verdict }: { verdict: Verdict }) {
  if (verdict === null) return null;
  const bg = verdict === "keep" ? "var(--color-success)" : "var(--color-danger)";
  return (
    <div
      className="absolute top-1 right-1 w-3 h-3 flex items-center justify-center rounded-xs pointer-events-none"
      style={{ background: bg }}
      aria-label={verdict === "keep" ? "keep" : "toss"}
    >
      {verdict === "keep" ? (
        <svg width="8" height="8" viewBox="0 0 8 8" fill="none">
          <path d="M1.5 4.2l1.7 1.6L6.5 2.2" stroke="white" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      ) : (
        <svg width="8" height="8" viewBox="0 0 8 8" fill="none">
          <path d="M1.8 1.8l4.4 4.4M6.2 1.8L1.8 6.2" stroke="white" strokeWidth="1.3" strokeLinecap="round" />
        </svg>
      )}
    </div>
  );
}
