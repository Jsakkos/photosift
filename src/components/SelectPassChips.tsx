import { useMemo } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";

/// Inspired by the Lightroom "smart collection per star tier" workflow —
/// each chip is a virtual filter corresponding to a pass. Clicking `2★+`
/// tells the store "show me only the photos I promoted to at least 2★,"
/// which is Pass 3. Counts update live from the store's `images`.
///
/// Rendered only when `currentView === "select"`; the Toolbar gates it.
export function SelectPassChips() {
  const images = useProjectStore((s) => s.images);
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const setSelectMinStar = useProjectStore((s) => s.setSelectMinStar);
  const selectRequiresPick = useSettingsStore(
    (s) => s.settings.selectRequiresPick ?? false,
  );

  const counts = useMemo(() => {
    const eligible = images.filter((img) =>
      selectRequiresPick ? img.flag === "pick" : img.flag !== "reject",
    );
    // counts[n] = number of eligible photos with starRating >= n.
    const out = [0, 0, 0, 0, 0, 0];
    for (const img of eligible) {
      const r = Math.max(0, Math.min(5, img.starRating));
      for (let n = 0; n <= r; n++) out[n]++;
    }
    return out;
  }, [images, selectRequiresPick]);

  const tiers: { floor: number; label: string }[] = [
    { floor: 0, label: "All" },
    { floor: 1, label: "1★+" },
    { floor: 2, label: "2★+" },
    { floor: 3, label: "3★+" },
    { floor: 4, label: "4★+" },
    { floor: 5, label: "5★" },
  ];

  return (
    <div className="px-4 py-1.5 bg-[var(--bg-tertiary)] border-b border-white/5 flex items-center gap-2 text-xs">
      <span className="text-[var(--text-secondary)] font-medium uppercase tracking-wide text-[10px]">
        Pass
      </span>
      <div className="flex items-center gap-1">
        {tiers.map(({ floor, label }) => {
          const active = selectMinStar === floor;
          const count = counts[floor];
          return (
            <button
              key={floor}
              type="button"
              onClick={() => setSelectMinStar(floor)}
              aria-pressed={active}
              title={`Show photos rated ${label}. ${count} ${count === 1 ? "photo" : "photos"}.`}
              className={`px-2 py-0.5 rounded transition-colors ${
                active
                  ? "bg-[var(--accent)] text-white font-semibold"
                  : "bg-[var(--bg-primary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-white/10"
              }`}
            >
              {label}
              <span
                className={`ml-1 text-[10px] ${active ? "text-white/80" : "text-[var(--text-secondary)]/60"}`}
              >
                {count}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
