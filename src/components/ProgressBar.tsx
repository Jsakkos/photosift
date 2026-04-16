import { useProjectStore } from "../stores/projectStore";

export function ProgressBar() {
  const images = useProjectStore((s) => s.images);
  const total = images.length;
  if (total === 0) return null;

  const picks = images.filter((i) => i.flag === "pick").length;
  const rejects = images.filter((i) => i.flag === "reject").length;

  const pickPct = (picks / total) * 100;
  const rejectPct = (rejects / total) * 100;

  return (
    <div className="h-[3px] bg-[var(--bg-tertiary)] flex">
      <div
        className="bg-green-500 transition-all duration-300"
        style={{ width: `${pickPct}%` }}
      />
      <div
        className="bg-red-500 transition-all duration-300"
        style={{ width: `${rejectPct}%` }}
      />
    </div>
  );
}
