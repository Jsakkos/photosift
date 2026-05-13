import type { CSSProperties, ReactNode } from "react";

/// Shared chip / icon-tile primitive. Every small status marker in the app â€”
/// AI verdicts, sharpness pills, destination tags, star overlays, EXIF â€” should
/// route through here so radius / padding / font / corner-offset stay uniform.
/// Geometry follows the design handoff: 2px radius for chips, 1px hairline
/// borders, JetBrains Mono, fixed corner offset (4px) for overlay badges.

export type BadgeTone =
  | "success"
  | "danger"
  | "warning"
  | "accent"
  | "accent-2"
  | "star"
  | "neutral";

/// `solid` â€” filled tone, near-black text. `soft` â€” tinted tone + hairline,
/// tone text. `glass` â€” translucent black for over-photo overlays, tone text.
/// `surface` â€” neutral elevated chip (bg3 + hairline), ignores `tone`.
export type BadgeVariant = "solid" | "soft" | "glass" | "surface";

export type BadgePos = "tl" | "tr" | "bl" | "br";

const TONE_COLOR: Record<BadgeTone, string> = {
  success: "var(--color-success)",
  danger: "var(--color-danger)",
  warning: "var(--color-warning)",
  accent: "var(--color-accent)",
  "accent-2": "var(--color-accent-2)",
  star: "var(--color-star)",
  neutral: "var(--color-fg)",
};

// Foreground for `solid`. The design-system tones are pastels, so near-black
// reads better on all of them than white (the old Tailwind-palette badges used
// white because `green-500` etc. are far more saturated).
const SOLID_FG = "var(--color-on-accent)";

const POS_CLASS: Record<BadgePos, string> = {
  tl: "absolute top-1 left-1",
  tr: "absolute top-1 right-1",
  bl: "absolute bottom-1 left-1",
  br: "absolute bottom-1 right-1",
};

/// Exported for the rare badge that needs bespoke geometry but the standard
/// tone/variant surface (e.g. the variable-width sharpness pill).
export function surfaceStyleFor(tone: BadgeTone, variant: BadgeVariant): CSSProperties {
  const c = TONE_COLOR[tone];
  switch (variant) {
    case "solid":
      return { background: c, color: SOLID_FG };
    case "soft":
      return {
        background: `color-mix(in srgb, ${c} 22%, transparent)`,
        color: c,
        border: `1px solid color-mix(in srgb, ${c} 50%, transparent)`,
      };
    case "glass":
      return { background: "rgba(0,0,0,0.6)", color: c, backdropFilter: "blur(2px)" };
    case "surface":
      return {
        background: "var(--color-bg3)",
        color: "var(--color-fg)",
        border: "1px solid var(--color-border)",
      };
  }
}

type BadgeProps = {
  tone?: BadgeTone;
  variant?: BadgeVariant;
  /// `xs` â‰ˆ 9px text (overlay chips), `sm` â‰ˆ 10px text (inline chips).
  size?: "xs" | "sm";
  pos?: BadgePos;
  className?: string;
  title?: string;
  "aria-label"?: string;
  "aria-hidden"?: boolean;
  children: ReactNode;
};

export function Badge({
  tone = "neutral",
  variant = "solid",
  size = "xs",
  pos,
  className,
  title,
  children,
  ...aria
}: BadgeProps) {
  const sizeClass =
    size === "xs" ? "text-3xs px-[5px] py-0.5" : "text-2xs px-1.5 py-[3px]";
  return (
    <span
      className={[
        "inline-flex items-center justify-center gap-[3px]",
        "font-mono leading-none whitespace-nowrap rounded-xs",
        sizeClass,
        pos ? POS_CLASS[pos] : "",
        className ?? "",
      ]
        .join(" ")
        .trim()}
      style={surfaceStyleFor(tone, variant)}
      title={title}
      {...aria}
    >
      {children}
    </span>
  );
}

type IconBadgeProps = {
  tone?: BadgeTone;
  variant?: BadgeVariant;
  /// `sm` = 20px tile (whole-photo summary dots), `md` = 24px tile (face-tile badges).
  size?: "sm" | "md";
  pos?: BadgePos;
  className?: string;
  title?: string;
  "aria-label"?: string;
  children: ReactNode;
};

export function IconBadge({
  tone = "neutral",
  variant = "solid",
  size = "md",
  pos,
  className,
  title,
  children,
  ...aria
}: IconBadgeProps) {
  const sizeClass = size === "sm" ? "w-5 h-5" : "w-6 h-6";
  return (
    <span
      className={[
        "inline-flex items-center justify-center shrink-0 rounded-xs",
        sizeClass,
        pos ? POS_CLASS[pos] : "",
        className ?? "",
      ]
        .join(" ")
        .trim()}
      style={surfaceStyleFor(tone, variant)}
      title={title}
      {...aria}
    >
      {children}
    </span>
  );
}
