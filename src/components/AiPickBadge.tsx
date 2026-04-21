export function AiPickBadge() {
  return (
    <div
      className="absolute top-1.5 right-1.5 bg-[var(--accent)]/25 border border-[var(--accent)]/60 text-[var(--accent)] text-[9px] font-semibold px-1.5 py-0.5 rounded pointer-events-none"
      title={
        "AI-recommended pick from this group.\n" +
        "Score = sharpness × (1 + eyes_open) × (1 + 0.5 × smile).\n" +
        "Shift+A accepts as group cover."
      }
      aria-label="AI pick"
    >
      ★ AI
    </div>
  );
}
