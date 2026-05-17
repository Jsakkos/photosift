import { useProjectStore } from "../../stores/projectStore";

/// Inline triage-stage verdict, rendered in the Triage tab's FacesRail.
/// Reads the on-import triage pass result from `triageJudgments` — the
/// "is this technically unusable?" call. This is deliberately *not* the
/// selection-stage `CuratorChip` (which answers "is this a keeper?" and
/// lives in the Select tab's DetailRail): showing the editorial verdict
/// here meant a frame the triage pass kept could display a contradictory
/// "reject — closed eyes". The triage pass auto-applies its rejects on
/// import, so there is no accept action — this chip is purely a note.
export function TriageNote() {
  const judgment = useProjectStore((s) => {
    const pid = s.displayItems[s.currentIndex]?.image.id;
    if (pid == null) return null;
    return s.triageJudgments.get(pid) ?? null;
  });

  if (!judgment) {
    return (
      <section
        aria-label="Triage AI — no verdict yet"
        className="rounded-sm px-2.5 py-2 text-[11px] leading-snug"
        style={{
          background: "var(--color-bg2)",
          border: "1px dashed var(--color-border)",
          color: "var(--color-fg-dim)",
        }}
      >
        <span
          className="font-mono text-2xs uppercase tracking-[1px] mr-1.5"
          style={{ color: "var(--color-fg-mute)" }}
        >
          Triage AI
        </span>
        No verdict — the on-import triage pass left this frame unjudged.
      </section>
    );
  }

  const reject = judgment.suggestedFlag === "reject";
  const tone = reject
    ? {
        accent: "var(--color-danger)",
        bar: "color-mix(in srgb, var(--color-danger) 55%, transparent)",
        label: "flagged",
      }
    : {
        accent: "var(--color-fg-dim)",
        bar: "rgba(255, 255, 255, 0.18)",
        label: "clear",
      };

  return (
    <section
      aria-label="Triage AI verdict"
      className="rounded-sm overflow-hidden"
      style={{
        background: "var(--color-bg2)",
        border: "1px solid var(--color-border)",
        borderLeft: `3px solid ${tone.bar}`,
      }}
    >
      <header
        className="px-2.5 py-1.5 flex items-center gap-2"
        style={{ borderBottom: "1px solid var(--color-border)" }}
      >
        <span
          className="font-mono text-2xs uppercase tracking-[1px]"
          style={{ color: tone.accent }}
          title="On-import triage pass — technical-defect screen"
        >
          Triage AI · {tone.label}
        </span>
        {reject && judgment.applied && (
          <span
            className="font-mono text-3xs uppercase tracking-[0.6px] px-[5px] py-[1px] rounded-xs"
            style={{
              color: "var(--color-fg-dim)",
              background: "var(--color-bg3)",
            }}
          >
            auto-rejected
          </span>
        )}
      </header>
      <p
        className="px-2.5 py-2 text-[11px] leading-[1.5]"
        style={{ color: "var(--color-fg)" }}
      >
        {judgment.reason}
      </p>
    </section>
  );
}
