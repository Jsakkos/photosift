import { useProjectStore } from "../stores/projectStore";

export function FlagFlash() {
  const lastFlagAction = useProjectStore((s) => s.lastFlagAction);
  const clearFlagFlash = useProjectStore((s) => s.clearFlagFlash);

  if (!lastFlagAction) return null;

  return (
    <div
      key={lastFlagAction.timestamp}
      className="absolute inset-0 pointer-events-none z-10 animate-flag-flash"
      style={{ backgroundColor: lastFlagAction.color }}
      onAnimationEnd={clearFlagFlash}
    />
  );
}
