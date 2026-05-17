import { memo, type CSSProperties, type ReactNode } from "react";
import type { ColorLabelValue } from "./ColorLabel";
import { ColorLabelChip } from "./ColorLabel";
import type { StarCount } from "./Stars";
import { Badge } from "./Badge";
import { VerdictBadge, type Verdict } from "./VerdictBadge";

/// The routing destinations that get a corner tag on a thumbnail.
/// `"publish_direct"` is the DB value; both it and `"export"` render as
/// the same "→ Exp" chip.
export type PhotoDestination = "edit" | "export" | "publish_direct" | null;

type PhotoProps = {
  src: string | null;
  alt?: string;
  fit?: "cover" | "contain";

  placeholderSeed?: string;
  dim?: number;
  sharp?: number;

  verdict?: Verdict;
  colorLabel?: ColorLabelValue | null;
  groupMember?: boolean;
  /// Cluster-indicator edge color. When set, the thumbnail gets a colored
  /// left stripe identifying which group it belongs to (Select filmstrip).
  /// Takes precedence over the plain `groupMember` accent stripe.
  groupColor?: string;
  selected?: boolean;
  /// Star rating overlay (bottom-left). 0 / undefined renders nothing.
  stars?: StarCount;
  /// Routing destination tag (bottom-right). "unrouted" callers pass null.
  destination?: PhotoDestination;

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

function PhotoInner({
  src,
  alt = "",
  fit = "cover",
  placeholderSeed,
  dim = 1,
  sharp = 1,
  verdict = null,
  colorLabel,
  groupMember = false,
  groupColor,
  selected = false,
  stars = 0,
  destination = null,
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

      {(groupMember || groupColor !== undefined) && (
        <div
          className="absolute left-0 top-0 bottom-0 w-[3px]"
          style={{ background: groupColor ?? "var(--color-accent)" }}
        />
      )}

      <VerdictBadge verdict={verdict} />

      {stars > 0 && (
        <Badge
          pos="bl"
          tone="star"
          variant="glass"
          className="tracking-[1px]"
          aria-label={`${stars} star${stars === 1 ? "" : "s"}`}
        >
          {"★".repeat(stars)}
        </Badge>
      )}

      {destination !== null && (
        <Badge
          pos="br"
          tone={destination === "edit" ? "accent-2" : "accent"}
          variant="glass"
          className="font-semibold"
          aria-label={destination === "edit" ? "Route: Capture One" : "Route: Export"}
        >
          {destination === "edit" ? "→ C1" : "→ Exp"}
        </Badge>
      )}

      {/* colorLabel and destination both claim bottom-right; in practice a
          thumbnail carries one or the other (color labels are a Select-pass
          affordance, destination tags appear in Route). */}
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
