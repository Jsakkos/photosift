import { useMemo } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { LoupeView } from "../LoupeView";
import { MetadataOverlay } from "../MetadataOverlay";
import { HeatmapOverlay } from "../HeatmapOverlay";
import { Kbd } from "../primitives";
import { AllStrip } from "./AllStrip";
import { TriageGroupStrip } from "./TriageGroupStrip";
import { FacesRail } from "./FacesRail";

function HeatmapHost() {
  const currentItem = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  if (!currentItem) return null;
  return <HeatmapOverlay photoId={currentItem.image.id} />;
}

function TopBar() {
  const current = useProjectStore((s) => s.displayItems[s.currentIndex] ?? null);
  const total = useProjectStore((s) => s.displayItems.length);
  const index = useProjectStore((s) => s.currentIndex);
  const showAllStrip = useProjectStore((s) => s.showAllStrip);
  const showFaces = useProjectStore((s) => s.showFaces);
  const heatmapOn = useProjectStore((s) => s.heatmapOn);
  const toggleAllStrip = useProjectStore((s) => s.toggleAllStrip);
  const toggleFaces = useProjectStore((s) => s.toggleFaces);
  const toggleHeatmap = useProjectStore((s) => s.toggleHeatmap);

  const image = current?.image;

  return (
    <div
      className="h-10 flex items-center px-4 gap-3 shrink-0 border-b"
      style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
    >
      <span
        className="font-mono text-[11px] truncate max-w-[260px]"
        style={{ color: "var(--color-fg)" }}
        title={image?.filepath ?? ""}
      >
        {image?.filename ?? "—"}
      </span>
      <div className="flex-1" />
      <button
        type="button"
        tabIndex={-1}
        onClick={toggleAllStrip}
        className="inline-flex items-center gap-[6px] font-mono text-[10px] uppercase tracking-[0.6px] px-[6px] py-[3px] rounded-xs bg-transparent border-0 cursor-pointer"
        style={{ color: showAllStrip ? "var(--color-fg-dim)" : "var(--color-fg-mute)" }}
      >
        <Kbd>T</Kbd>
        <span>strip</span>
      </button>
      <button
        type="button"
        tabIndex={-1}
        onClick={toggleFaces}
        className="inline-flex items-center gap-[6px] font-mono text-[10px] uppercase tracking-[0.6px] px-[6px] py-[3px] rounded-xs bg-transparent border-0 cursor-pointer"
        style={{ color: showFaces ? "var(--color-fg-dim)" : "var(--color-fg-mute)" }}
      >
        <Kbd>F</Kbd>
        <span>faces</span>
      </button>
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
        {total === 0 ? "0 / 0" : `${index + 1} / ${total}`}
      </span>
    </div>
  );
}

function BottomBar() {
  const images = useProjectStore((s) => s.images);

  const counts = useMemo(() => {
    let kept = 0;
    let tossed = 0;
    let remaining = 0;
    for (const img of images) {
      if (img.flag === "pick") kept++;
      else if (img.flag === "reject") tossed++;
      else remaining++;
    }
    return { kept, tossed, remaining };
  }, [images]);

  const ShortcutChip = ({ kbd, label }: { kbd: string; label: string }) => (
    <span className="inline-flex items-center gap-[6px]">
      <Kbd>{kbd}</Kbd>
      <span
        className="font-mono text-[10px] uppercase tracking-[0.6px]"
        style={{ color: "var(--color-fg-dim)" }}
      >
        {label}
      </span>
    </span>
  );

  return (
    <div
      className="h-10 flex items-center px-4 gap-4 shrink-0 border-t"
      style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
    >
      <ShortcutChip kbd="P" label="keep" />
      <ShortcutChip kbd="X" label="toss" />
      <ShortcutChip kbd="Space" label="skip" />
      <ShortcutChip kbd="⇧ P" label="keep group" />
      <ShortcutChip kbd="Z" label="undo" />
      <ShortcutChip kbd="Tab" label="2-up" />
      <div className="flex-1" />
      <span className="font-mono text-[10px] tabular-nums" style={{ color: "var(--color-success)" }}>
        ✓ {counts.kept}
      </span>
      <span className="font-mono text-[10px] tabular-nums" style={{ color: "var(--color-danger)" }}>
        ✕ {counts.tossed}
      </span>
      <span
        className="font-mono text-[10px] tabular-nums"
        style={{ color: "var(--color-fg-dim)" }}
      >
        · {counts.remaining} left
      </span>
    </div>
  );
}

function VerdictHint({ side, label, kbd, tone }: { side: "left" | "right"; label: string; kbd: string; tone: string }) {
  return (
    <div
      className="pointer-events-none absolute top-1/2 -translate-y-1/2 flex items-center gap-[10px]"
      style={{
        [side]: 16,
        padding: "10px 14px",
        borderRadius: 4,
        background: "rgba(21,21,21,0.72)",
        backdropFilter: "blur(8px)",
        border: `1px solid ${tone}`,
        color: tone,
      } as React.CSSProperties}
    >
      <Kbd>{kbd}</Kbd>
      <span className="font-mono text-[10px] uppercase tracking-[1px]">{label}</span>
    </div>
  );
}

export function TriageShell() {
  const showAllStrip = useProjectStore((s) => s.showAllStrip);
  const showFaces = useProjectStore((s) => s.showFaces);

  return (
    <div className="flex-1 flex overflow-hidden">
      {showAllStrip && <AllStrip />}
      <TriageGroupStrip />
      <div className="flex-1 flex flex-col min-w-0">
        <TopBar />
        <div className="flex-1 relative overflow-hidden" style={{ background: "#0c0c0c" }}>
          <LoupeView />
          <HeatmapHost />
          <MetadataOverlay />
          <VerdictHint side="left" label="keep" kbd="P" tone="var(--color-success)" />
          <VerdictHint side="right" label="toss" kbd="X" tone="var(--color-danger)" />
        </div>
        <BottomBar />
      </div>
      {showFaces && <FacesRail />}
    </div>
  );
}
