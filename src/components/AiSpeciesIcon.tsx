/// Tiny indicator at the top-right of each face tile that distinguishes
/// human faces from cat faces. Only renders for cats — the human case
/// is the default so leaving the tile unadorned keeps the normal
/// portrait flow visually quiet.
export function AiSpeciesIcon({ species }: { species: string }) {
  if (species !== "cat") return null;
  return (
    <div
      className="absolute top-1 right-1 bg-[var(--bg-secondary)]/85 rounded w-5 h-5 flex items-center justify-center pointer-events-none shadow-sm"
      aria-label="Cat face"
      title="Cat face"
    >
      <CatIcon />
    </div>
  );
}

function CatIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path
        d="M3 5 L5 9 L3 12 L7 11 L10 14 L13 11 L17 12 L15 9 L17 5 L13 8 L10 6 L7 8 Z"
        fill="currentColor"
        opacity="0.9"
      />
      <circle cx="8" cy="10" r="0.9" fill="#fff" />
      <circle cx="12" cy="10" r="0.9" fill="#fff" />
    </svg>
  );
}
