import { useRef, useState, useCallback } from "react";
import { useProjectStore } from "../stores/projectStore";
import { useImageLoader } from "../hooks/useImageLoader";
import { Kbd, Stars } from "./primitives";
import type { ImageEntry } from "../types";

type Side = "L" | "R";

type PanelProps = {
  side: Side;
  image: ImageEntry | null;
  url: string | null;
  picked: boolean;
  scale: number;
  imgStyle: React.CSSProperties;
};

function ScorePill({ label, value }: { label: string; value: number }) {
  const high = value >= 85;
  return (
    <span
      className="inline-flex items-center gap-[4px] px-[6px] py-[2px] rounded-xs font-mono text-[9px]"
      style={{
        background: "rgba(0,0,0,0.45)",
        color: high ? "var(--color-accent-2)" : "var(--color-fg-dim)",
        border: `1px solid ${high ? "var(--color-accent-2)" : "var(--color-border)"}`,
      }}
    >
      <span className="uppercase tracking-[0.5px]">{label}</span>
      <span className="tabular-nums">{value}</span>
    </span>
  );
}

function ComparePanel({ side, image, url, picked, scale, imgStyle }: PanelProps) {
  if (!image) return <div className="flex-1" style={{ background: "#0a0a0a" }} />;
  const rating = Math.max(0, Math.min(5, image.starRating)) as 0 | 1 | 2 | 3 | 4 | 5;
  const sharp = Math.round(image.sharpnessScore ?? 0);
  const face = Math.round((image.faceCount ?? 0) > 0 ? 90 : 0);
  const eye = (() => {
    const pairs = (image.faceCount ?? 0) * 2;
    if (pairs <= 0) return 0;
    return Math.round(((image.eyesOpenCount ?? 0) / pairs) * 100);
  })();
  const smile = Math.round((image.maxSmileScore ?? 0) * 100);

  return (
    <div
      className="flex-1 flex flex-col min-h-0 min-w-0"
      style={{ background: "#0a0a0a" }}
    >
      <div className="flex-1 relative overflow-hidden">
        <div
          className="absolute top-3 left-3 z-10 flex items-center gap-2 font-mono text-[10px] uppercase tracking-[1px]"
          style={{ color: picked ? "var(--color-success)" : "var(--color-fg-dim)" }}
        >
          <span>{side}</span>
          {picked && <span>✓ picked</span>}
        </div>
        {scale > 1 && (
          <div
            className="absolute top-3 left-1/2 -translate-x-1/2 z-10 font-mono text-[10px] px-2 py-[2px] rounded-xs"
            style={{ background: "rgba(0,0,0,0.55)", color: "rgba(255,255,255,0.7)" }}
          >
            {Math.round(scale * 100)}%
          </div>
        )}
        {url && (
          <div className="absolute inset-0 p-5 flex items-center justify-center">
            <img
              src={url}
              alt={image.filename}
              className="max-w-full max-h-full object-contain"
              style={imgStyle}
              draggable={false}
            />
          </div>
        )}
      </div>
      <div
        className="px-4 py-3 border-t flex items-center gap-3 flex-wrap"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg2)" }}
      >
        <span
          className="font-mono text-[11px] truncate max-w-[220px]"
          style={{ color: "var(--color-fg)" }}
          title={image.filepath}
        >
          {image.filename}
        </span>
        <Stars value={rating} size={11} />
        <div className="flex-1" />
        <ScorePill label="sharp" value={sharp} />
        <ScorePill label="face" value={face} />
        <ScorePill label="eye" value={eye} />
        <ScorePill label="smile" value={smile} />
      </div>
    </div>
  );
}

export function ComparisonView() {
  const comparisonPinnedId = useProjectStore((s) => s.comparisonPinnedId);
  const comparisonCyclingId = useProjectStore((s) => s.comparisonCyclingId);
  const comparisonGroupMembers = useProjectStore((s) => s.comparisonGroupMembers);
  const images = useProjectStore((s) => s.images);
  const groups = useProjectStore((s) => s.groups);

  const pinnedImage = images.find((i) => i.id === comparisonPinnedId) ?? null;
  const cyclingImage = images.find((i) => i.id === comparisonCyclingId) ?? null;

  const { displayUrl: pinnedUrl } = useImageLoader(comparisonPinnedId);
  const { displayUrl: cyclingUrl } = useImageLoader(comparisonCyclingId);

  const [transform, setTransform] = useState({ x: 0, y: 0, scale: 1 });
  const isDragging = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });

  const availableMembers = comparisonGroupMembers.filter((id) => {
    const img = images.find((i) => i.id === id);
    return img && img.flag !== "reject";
  });
  const cyclingIdx = availableMembers.indexOf(comparisonCyclingId!);

  const groupOrdinal = (() => {
    if (!pinnedImage) return 0;
    const ordered = [...groups].sort((a, b) => a.id - b.id);
    const gid = ordered.find((g) => g.members.some((m) => m.photoId === pinnedImage.id))?.id;
    if (gid === undefined) return 0;
    return ordered.findIndex((g) => g.id === gid) + 1;
  })();

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setTransform((t) => ({
      ...t,
      scale: Math.max(0.5, Math.min(10, t.scale * delta)),
    }));
  }, []);

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (transform.scale <= 1) return;
      isDragging.current = true;
      dragStart.current = { x: e.clientX - transform.x, y: e.clientY - transform.y };
    },
    [transform],
  );

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging.current) return;
    setTransform((t) => ({
      ...t,
      x: e.clientX - dragStart.current.x,
      y: e.clientY - dragStart.current.y,
    }));
  }, []);

  const handleMouseUp = useCallback(() => {
    isDragging.current = false;
  }, []);

  const imgStyle: React.CSSProperties = {
    transform: `scale(${transform.scale}) translate(${transform.x / transform.scale}px, ${transform.y / transform.scale}px)`,
    transformOrigin: "center center",
    cursor: transform.scale > 1 ? "grab" : "default",
  };

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div
        className="h-10 flex items-center px-4 gap-3 shrink-0 border-b"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
      >
        <span className="font-mono text-[11px]" style={{ color: "var(--color-fg)" }}>
          2-up compare
        </span>
        {groupOrdinal > 0 && (
          <span
            className="font-mono text-[10px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            · Group G{groupOrdinal} · {cyclingIdx + 1}/{availableMembers.length}
          </span>
        )}
        <span
          className="font-mono text-[10px]"
          style={{ color: "var(--color-fg-mute)" }}
        >
          · locked zoom {Math.round(transform.scale * 100)}%
        </span>
        <div className="flex-1" />
        <div className="flex items-center gap-[8px]">
          <Kbd>1</Kbd>
          <span
            className="font-mono text-[10px] uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            pick L
          </span>
        </div>
        <div className="flex items-center gap-[8px]">
          <Kbd>2</Kbd>
          <span
            className="font-mono text-[10px] uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            pick R
          </span>
        </div>
        <div className="flex items-center gap-[8px]">
          <Kbd>Esc</Kbd>
          <span
            className="font-mono text-[10px] uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            exit
          </span>
        </div>
      </div>

      <div
        className="flex-1 grid min-h-0"
        style={{
          gridTemplateColumns: "1fr 1fr",
          gap: 2,
          background: "var(--color-border)",
        }}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        <ComparePanel
          side="L"
          image={pinnedImage}
          url={pinnedUrl}
          picked
          scale={transform.scale}
          imgStyle={imgStyle}
        />
        <ComparePanel
          side="R"
          image={cyclingImage}
          url={cyclingUrl}
          picked={false}
          scale={transform.scale}
          imgStyle={imgStyle}
        />
      </div>

      <div
        className="h-10 flex items-center px-4 gap-4 shrink-0 border-t"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
      >
        <span className="font-mono text-[10px]" style={{ color: "var(--color-fg-dim)" }}>
          Winner promoted · pan + zoom synchronised
        </span>
        <div className="flex-1" />
        <span className="font-mono text-[10px]" style={{ color: "var(--color-fg-mute)" }}>
          ◀ ▶ cycle right panel
        </span>
      </div>
    </div>
  );
}
