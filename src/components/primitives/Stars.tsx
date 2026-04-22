import { memo } from "react";

export type StarCount = 0 | 1 | 2 | 3 | 4 | 5;

type StarsProps = {
  value: StarCount;
  max?: number;
  size?: number;
  color?: string;
  className?: string;
};

function StarsInner({ value, max = 5, size = 11, color = "var(--color-star)", className }: StarsProps) {
  return (
    <div className={`inline-flex items-center gap-[1.5px] ${className ?? ""}`.trim()}>
      {Array.from({ length: max }).map((_, i) => {
        const filled = i < value;
        return (
          <svg
            key={i}
            width={size}
            height={size}
            viewBox="0 0 16 16"
            fill={filled ? color : "none"}
            stroke={filled ? color : "rgba(255,255,255,0.35)"}
            strokeWidth={1.2}
          >
            <path
              d="M8 1.5l1.95 4.17 4.55.46-3.42 3.12.97 4.5L8 11.45l-4.05 2.3.97-4.5L1.5 6.13l4.55-.46z"
              strokeLinejoin="round"
            />
          </svg>
        );
      })}
    </div>
  );
}

export const Stars = memo(StarsInner);
