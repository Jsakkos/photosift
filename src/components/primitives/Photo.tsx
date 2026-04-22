import { memo, type CSSProperties, type ReactNode } from "react";
import type { ColorLabelValue } from "./ColorLabel";
import { ColorLabelChip } from "./ColorLabel";
import { Stars, type StarCount } from "./Stars";

export type Verdict = "keep" | "toss" | null;

type PhotoProps = {
  src: string | null;
  alt?: string;
  fit?: "cover" | "contain";

  placeholderSeed?: string;
  dim?: number;
  sharp?: number;

  verdict?: Verdict;
  rating?: StarCount;
  colorLabel?: ColorLabelValue | null;
  groupMember?: boolean;
  selected?: boolean;

  className?: string;
  style?: CSSProperties;
  onClick?: () => void;
  children?: ReactNode;
};

function placeholderHue(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) | 0;
  return ((h % 360) + 360) % 360;
}

function placeholderBackground(seed: string): string {
  const hue = placeholderHue(seed);
  const c1 = `oklch(${0.32 + (seed.length % 5) * 0.03} 0.02 ${hue})`;
  const c2 = `oklch(${0.48 + (seed.length % 3) * 0.04} 0.03 ${hue + 40})`;
  const c3 = `oklch(0.22 0.015 ${hue - 20})`;
  return `linear-gradient(135deg, ${c1} 0%, ${c2} 60%, ${c3} 100%)`;
}

function VerdictBadge({ verdict }: { verdict: Exclude<Verdict, null> }) {
  const bg = verdict === "keep" ? "var(--color-success)" : "var(--color-danger)";
  return (
    <div
      className="absolute top-1 right-1 w-3 h-3 flex items-center justify-center rounded-xs"
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

function PhotoInner({
  src,
  alt = "",
  fit = "cover",
  placeholderSeed,
  dim = 1,
  sharp = 1,
  verdict = null,
  rating,
  colorLabel,
  groupMember = false,
  selected = false,
  className,
  style,
  onClick,
  children,
}: PhotoProps) {
  const blur = sharp < 0.5 ? `blur(${(0.5 - sharp) * 3}px)` : undefined;
  const placeholderBg = src === null && placeholderSeed !== undefined
    ? placeholderBackground(placeholderSeed)
    : undefined;

  return (
    <div
      className={`relative overflow-hidden ${className ?? ""}`.trim()}
      style={{
        background: placeholderBg ?? "rgba(255,255,255,0.03)",
        opacity: dim,
        outline: selected ? "2px solid var(--color-accent-blue)" : undefined,
        outlineOffset: selected ? 1 : undefined,
        cursor: onClick ? "pointer" : undefined,
        ...style,
      }}
      onClick={onClick}
    >
      {src !== null && (
        <img
          src={src}
          alt={alt}
          draggable={false}
          loading="lazy"
          decoding="async"
          className="absolute inset-0 w-full h-full"
          style={{
            objectFit: fit,
            filter: blur,
            userSelect: "none",
          }}
        />
      )}

      {placeholderBg !== undefined && placeholderSeed !== undefined && (
        <div
          className="absolute inset-0 mix-blend-overlay pointer-events-none"
          style={{
            backgroundImage: `repeating-linear-gradient(${45 + (placeholderHue(placeholderSeed) % 60)}deg, rgba(255,255,255,0.04) 0 2px, transparent 2px 18px)`,
          }}
        />
      )}

      {groupMember && (
        <div className="absolute left-0 top-0 bottom-0 w-[2px]" style={{ background: "var(--color-accent)" }} />
      )}

      {verdict !== null && <VerdictBadge verdict={verdict} />}

      {rating !== undefined && rating > 0 && (
        <div className="absolute bottom-1 left-1 px-[4px] py-[2px] rounded-xs bg-black/55 backdrop-blur-sm">
          <Stars value={rating} size={9} />
        </div>
      )}

      {colorLabel && (
        <div className="absolute bottom-1 right-1">
          <ColorLabelChip color={colorLabel} size={8} />
        </div>
      )}

      {children}
    </div>
  );
}

export const Photo = memo(PhotoInner);
