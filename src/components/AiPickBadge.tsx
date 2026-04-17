export function AiPickBadge() {
  return (
    <div
      className="absolute top-1.5 right-1.5 bg-[var(--accent)]/25 border border-[var(--accent)]/60 text-[var(--accent)] text-[9px] font-semibold px-1.5 py-0.5 rounded pointer-events-none"
      title="AI recommends this photo"
      aria-label="AI pick"
    >
      ★ AI
    </div>
  );
}
