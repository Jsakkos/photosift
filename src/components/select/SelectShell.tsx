import { useMemo } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { LoupeView } from "../LoupeView";
import { MetadataOverlay } from "../MetadataOverlay";
import { HeatmapOverlay } from "../HeatmapOverlay";
import { Kbd, Stars } from "../primitives";
import { StarGroupedStrip } from "./StarGroupedStrip";
import { RatingPeerStrip } from "./RatingPeerStrip";
import { DetailRail } from "./DetailRail";

function HeatmapHost() {
  const current = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  if (!current) return null;
  return <HeatmapOverlay photoId={current.image.id} />;
}

const PASS_TIERS: { floor: number; label: string }[] = [
  { floor: 0, label: "all" },
  { floor: 1, label: "★≥1" },
  { floor: 2, label: "★≥2" },
  { floor: 3, label: "★≥3" },
  { floor: 4, label: "★≥4" },
  { floor: 5, label: "★≥5" },
];

function PassPills() {
  const images = useProjectStore((s) => s.images);
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const setSelectMinStar = useProjectStore((s) => s.setSelectMinStar);
  const selectRequiresPick = useSettingsStore((s) => s.settings.selectRequiresPick ?? false);

  const counts = useMemo(() => {
    const eligible = images.filter((img) =>
      selectRequiresPick ? img.flag === "pick" : img.flag !== "reject",
    );
    const out = [0, 0, 0, 0, 0, 0];
    for (const img of eligible) {
      const r = Math.max(0, Math.min(5, img.starRating));
      for (let n = 0; n <= r; n++) out[n]++;
    }
    return out;
  }, [images, selectRequiresPick]);

  return (
    <div
      className="inline-flex items-center gap-[1px] rounded-md p-[2px]"
      style={{ background: "var(--color-bg3)" }}
    >
      {PASS_TIERS.map((tier) => {
        const active = tier.floor === selectMinStar;
        return (
          <button
            key={tier.floor}
            type="button"
            tabIndex={-1}
            aria-pressed={active}
            onClick={() => setSelectMinStar(tier.floor)}
            className="px-[8px] py-[3px] rounded-xs font-mono text-[10px] tabular-nums flex items-center gap-[6px] bg-transparent border-0 cursor-pointer"
            style={{
              color: active ? "#1a1a1a" : "var(--color-fg-dim)",
              background: active ? "var(--color-accent)" : "transparent",
              fontWeight: active ? 600 : 400,
            }}
          >
            <span>{tier.label}</span>
            <span style={{ opacity: 0.7 }}>{counts[tier.floor]}</span>
          </button>
        );
      })}
    </div>
  );
}

function TopBar() {
  const current = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  const total = useProjectStore((s) => s.displayItems.length);
  const index = useProjectStore((s) => s.currentIndex);
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const heatmapOn = useProjectStore((s) => s.heatmapOn);
  const toggleHeatmap = useProjectStore((s) => s.toggleHeatmap);

  const image = current?.image;
  const passNumber = selectMinStar + 1;

  return (
    <div
      className="h-11 flex items-center px-4 gap-3 shrink-0 border-b"
      style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
    >
      <span
        className="font-mono text-[11px] truncate max-w-[240px]"
        style={{ color: "var(--color-fg)" }}
        title={image?.filepath ?? ""}
      >
        {image?.filename ?? "—"}
      </span>
      <div className="flex-1" />
      <PassPills />
      <button
        type="button"
        tabIndex={-1}
        onClick={toggleHeatmap}
        className="inline-flex items-center gap-[6px] font-mono text-[10px] uppercase tracking-[0.6px] px-[6px] py-[3px] rounded-xs bg-transparent border-0 cursor-pointer"
        style={{ color: heatmapOn ? "var(--color-fg-dim)" : "var(--color-fg-mute)" }}
      >
        <Kbd>H</Kbd>
        <span>heat</span>
      </button>
      <span
        className="font-mono text-[10px] tabular-nums pl-3 ml-1 border-l"
        style={{ color: "var(--color-fg-dim)", borderColor: "var(--color-border)" }}
      >
        Pass {passNumber} · {total === 0 ? "0 / 0" : `${index + 1} / ${total}`}
      </span>
    </div>
  );
}

function RatingCard({
  value,
  active,
  onClick,
  clearCard = false,
}: {
  value: number;
  active: boolean;
  onClick: () => void;
  clearCard?: boolean;
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      onClick={onClick}
      className="flex items-center gap-[8px] px-[10px] py-[8px] rounded-md border bg-transparent cursor-pointer"
      style={{
        background: active ? "rgba(212,165,116,0.12)" : "rgba(21,21,21,0.72)",
        borderColor: active ? "var(--color-accent)" : "var(--color-border)",
        color: active ? "var(--color-accent)" : "var(--color-fg-dim)",
        backdropFilter: "blur(8px)",
        minWidth: 80,
      }}
      aria-pressed={active}
    >
      <Kbd>{clearCard ? "0" : String(value)}</Kbd>
      {clearCard ? (
        <span className="font-mono text-[10px] uppercase tracking-[0.6px]">clear</span>
      ) : (
        <Stars value={value as 1 | 2 | 3 | 4 | 5} size={11} />
      )}
    </button>
  );
}

function RatingColumn() {
  const current = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  const setRating = useProjectStore((s) => s.setRating);

  if (!current) return null;
  const rating = Math.max(0, Math.min(5, current.image.starRating));

  const handle = (n: number) => {
    if (n === 0) {
      void setRating(0);
    } else {
      // Toggle: re-pressing the current rating clears it, matching Lightroom.
      void setRating(rating === n ? 0 : n);
    }
  };

  return (
    <div
      className="pointer-events-auto absolute top-1/2 -translate-y-1/2 left-6 flex flex-col gap-[8px]"
      aria-label="Rating keys"
    >
      <div
        className="mb-[2px] px-[10px] py-[6px] rounded-md flex items-center gap-2"
        style={{
          background: "rgba(21,21,21,0.72)",
          border: "1px solid var(--color-border)",
          backdropFilter: "blur(8px)",
        }}
      >
        <Stars value={rating as 0 | 1 | 2 | 3 | 4 | 5} size={14} />
        <span
          className="font-mono text-[9px] uppercase tracking-[0.6px]"
          style={{ color: "var(--color-fg-mute)" }}
        >
          current
        </span>
      </div>
      {[1, 2, 3, 4, 5].map((n) => (
        <RatingCard key={n} value={n} active={rating === n} onClick={() => handle(n)} />
      ))}
      <RatingCard value={0} active={rating === 0} onClick={() => handle(0)} clearCard />
    </div>
  );
}

function CompareNarrowHints() {
  return (
    <div
      className="pointer-events-none absolute top-1/2 -translate-y-1/2 right-6 flex flex-col gap-[10px] items-end"
      aria-label="Compare and narrow-pass hints"
    >
      <div
        className="flex items-center gap-[8px] px-[12px] py-[8px] rounded-md"
        style={{
          background: "rgba(21,21,21,0.72)",
          border: "1px solid var(--color-accent-2)",
          color: "var(--color-accent-2)",
          backdropFilter: "blur(8px)",
        }}
      >
        <Kbd>Tab</Kbd>
        <span className="font-mono text-[10px] uppercase tracking-[1px]">compare</span>
      </div>
      <div
        className="flex items-center gap-[6px] px-[10px] py-[6px] rounded-md"
        style={{
          background: "rgba(21,21,21,0.72)",
          border: "1px solid var(--color-border)",
          backdropFilter: "blur(8px)",
          color: "var(--color-fg-dim)",
        }}
      >
        <Kbd>[</Kbd>
        <Kbd>]</Kbd>
        <span className="font-mono text-[10px] uppercase tracking-[0.6px]">narrow</span>
      </div>
    </div>
  );
}

function BottomBar() {
  return (
    <div
      className="h-10 flex items-center px-4 gap-4 shrink-0 border-t"
      style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
    >
      {[
        { kbd: "1–5", label: "rate" },
        { kbd: "0", label: "clear" },
        { kbd: "Tab", label: "2-up" },
        { kbd: "[", label: "narrow" },
        { kbd: "]", label: "widen" },
        { kbd: "G", label: "grid" },
      ].map((s) => (
        <span key={s.label} className="inline-flex items-center gap-[6px]">
          <Kbd>{s.kbd}</Kbd>
          <span
            className="font-mono text-[10px] uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {s.label}
          </span>
        </span>
      ))}
    </div>
  );
}

export function SelectShell() {
  const showAllStrip = useProjectStore((s) => s.showAllStrip);
  const showFaces = useProjectStore((s) => s.showFaces);

  return (
    <div className="flex-1 flex overflow-hidden">
      {showAllStrip && <StarGroupedStrip />}
      <RatingPeerStrip />
      <div className="flex-1 flex flex-col min-w-0">
        <TopBar />
        <div className="flex-1 relative overflow-hidden" style={{ background: "#0c0c0c" }}>
          <LoupeView />
          <HeatmapHost />
          <MetadataOverlay />
          <RatingColumn />
          <CompareNarrowHints />
        </div>
        <BottomBar />
      </div>
      {showFaces && <DetailRail />}
    </div>
  );
}
