import { useProjectStore } from "../stores/projectStore";
import type { CullView } from "../types";

interface HintBit {
  kind: "text" | "key";
  value: string;
}

const HINTS: Record<CullView, HintBit[]> = {
  triage: [
    { kind: "text", value: "First pass — review every photo." },
    { kind: "key", value: "P" },
    { kind: "text", value: "keep · " },
    { kind: "key", value: "X" },
    { kind: "text", value: "reject · " },
    { kind: "key", value: "Space" },
    { kind: "text", value: "skip uncertain shots. Groups auto-expand so you decide each frame." },
  ],
  select: [
    { kind: "text", value: "Iterative rating — rate each keeper and raise the pass floor to narrow down. " },
    { kind: "key", value: "1" },
    { kind: "text", value: "–" },
    { kind: "key", value: "5" },
    { kind: "text", value: "rate · " },
    { kind: "key", value: "[" },
    { kind: "text", value: " / " },
    { kind: "key", value: "]" },
    { kind: "text", value: "step pass · " },
    { kind: "key", value: "Tab" },
    { kind: "text", value: "2-up compare · " },
    { kind: "key", value: "X" },
    { kind: "text", value: "reject." },
  ],
  route: [
    { kind: "text", value: "Send each pick to its destination." },
    { kind: "key", value: "E" },
    { kind: "text", value: "edit in Capture One / DxO · " },
    { kind: "key", value: "D" },
    { kind: "text", value: "publish direct (JPEG export)." },
  ],
};

export function ViewHint() {
  const currentView = useProjectStore((s) => s.currentView);
  const bits = HINTS[currentView];

  return (
    <div className="px-4 py-1.5 bg-[var(--bg-tertiary)] text-[var(--text-secondary)] text-xs border-b border-white/5 flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
      {bits.map((bit, i) =>
        bit.kind === "key" ? (
          <kbd
            key={i}
            className="px-1.5 py-0.5 rounded bg-[var(--bg-primary)] text-[var(--text-primary)] font-mono text-[10px] border border-white/10"
          >
            {bit.value}
          </kbd>
        ) : (
          <span key={i}>{bit.value}</span>
        ),
      )}
    </div>
  );
}
