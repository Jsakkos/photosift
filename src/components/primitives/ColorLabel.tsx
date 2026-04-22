export type ColorLabelValue = "red" | "yellow" | "green" | "blue" | "purple";

const COLOR_VAR: Record<ColorLabelValue, string> = {
  red: "var(--color-label-red)",
  yellow: "var(--color-label-yellow)",
  green: "var(--color-label-green)",
  blue: "var(--color-label-blue)",
  purple: "var(--color-label-purple)",
};

export const COLOR_LABEL_ORDER: ColorLabelValue[] = ["red", "yellow", "green", "blue", "purple"];

type ColorLabelChipProps = {
  color: ColorLabelValue;
  size?: number;
  className?: string;
};

export function ColorLabelChip({ color, size = 8, className }: ColorLabelChipProps) {
  return (
    <div
      className={`rounded-xs ${className ?? ""}`.trim()}
      style={{ width: size, height: size, background: COLOR_VAR[color] }}
    />
  );
}

type ColorLabelRowProps = {
  value: ColorLabelValue | null;
  onChange?: (value: ColorLabelValue | null) => void;
  size?: number;
  className?: string;
};

export function ColorLabelRow({ value, onChange, size = 12, className }: ColorLabelRowProps) {
  return (
    <div className={`inline-flex items-center gap-[6px] ${className ?? ""}`.trim()}>
      {COLOR_LABEL_ORDER.map((c) => {
        const selected = value === c;
        return (
          <button
            key={c}
            type="button"
            tabIndex={-1}
            aria-label={`Color label ${c}`}
            aria-pressed={selected}
            onClick={() => onChange?.(selected ? null : c)}
            className="p-0 border-0 bg-transparent cursor-pointer inline-flex items-center justify-center"
            style={{
              width: size + 4,
              height: size + 4,
              borderRadius: 3,
              outline: selected ? "1px solid rgba(255,255,255,0.9)" : "none",
              outlineOffset: 1,
            }}
          >
            <ColorLabelChip color={c} size={size} />
          </button>
        );
      })}
    </div>
  );
}
