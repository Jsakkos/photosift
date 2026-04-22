type ScoreTone = "accent" | "accent-2" | "success" | "warning" | "danger";

const toneToColor: Record<ScoreTone, string> = {
  accent: "var(--color-accent)",
  "accent-2": "var(--color-accent-2)",
  success: "var(--color-success)",
  warning: "var(--color-warning)",
  danger: "var(--color-danger)",
};

type ScoreBarProps = {
  label: string;
  value: number;
  max?: number;
  tone?: ScoreTone;
  className?: string;
};

export function ScoreBar({ label, value, max = 100, tone = "accent-2", className }: ScoreBarProps) {
  const pct = Math.max(0, Math.min(100, (value / max) * 100));
  return (
    <div
      className={`flex items-center gap-2 font-mono text-[10px] ${className ?? ""}`.trim()}
    >
      <span className="w-12 uppercase tracking-[0.5px] text-fg-dim">{label}</span>
      <div className="flex-1 h-[3px] rounded-[1px] overflow-hidden bg-[rgba(255,255,255,0.08)]">
        <div className="h-full" style={{ width: `${pct}%`, background: toneToColor[tone] }} />
      </div>
      <span className="w-6 text-right tabular-nums text-[rgba(230,225,218,0.7)]">{value}</span>
    </div>
  );
}
