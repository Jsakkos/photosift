import { useProjectStore } from "../stores/projectStore";
import { humanizeCuratorReason } from "../lib/curatorText";
import { Kbd } from "./primitives";

/// Inline Curator-suggestion panel rendered inside the FacesRail. Reads
/// the judgment from the project-store map (bulk-loaded at loadShoot,
/// kept in sync via patchCuratorJudgment after accept/override). When
/// no judgment exists for the current photo, renders a slim empty-state
/// hint pointing the user back to the Library (where Curator runs are
/// kicked off) — this is the "empty state names the right subsystem"
/// requirement from #17. Press `.` in Triage to accept.
export function CuratorChip() {
  const judgment = useProjectStore((s) => {
    const pid = s.displayItems[s.currentIndex]?.image.id;
    if (pid == null) return null;
    return s.curatorJudgments.get(pid) ?? null;
  });
  const images = useProjectStore((s) => s.images);

  if (!judgment) {
    return (
      <section
        aria-label="Curator — no judgment yet"
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
          Curator
        </span>
        No judgment yet — run from the Library card.
      </section>
    );
  }

  const tone = (() => {
    switch (judgment.suggestedFlag) {
      case "pick":
        return {
          accent: "var(--color-success)",
          bar: "color-mix(in srgb, var(--color-success) 55%, transparent)",
          label: "keep",
        };
      case "reject":
        return {
          accent: "var(--color-danger)",
          bar: "color-mix(in srgb, var(--color-danger) 55%, transparent)",
          label: "toss",
        };
      case "keep":
      default:
        return {
          accent: "var(--color-fg-dim)",
          bar: "rgba(255, 255, 255, 0.18)",
          label: "no opinion",
        };
    }
  })();

  const handled = judgment.userAction != null;

  return (
    <section
      aria-label={`Curator suggestion (${providerLabel(judgment.provider)})`}
      className="rounded-sm overflow-hidden"
      style={{
        background: "var(--color-bg2)",
        border: "1px solid var(--color-border)",
        borderLeft: `3px solid ${tone.bar}`,
        opacity: handled ? 0.65 : 1,
      }}
    >
      <header
        className="px-2.5 py-1.5 flex items-center justify-between gap-2"
        style={{ borderBottom: "1px solid var(--color-border)" }}
      >
        <div className="flex items-center gap-2 min-w-0">
          <span
            className="font-mono text-2xs uppercase tracking-[1px]"
            style={{ color: tone.accent }}
            title={`Curator (${providerLabel(judgment.provider)} · ${judgment.model})`}
          >
            Curator · {tone.label}
          </span>
          <span
            title={`${providerLabel(judgment.provider)} · ${judgment.model}`}
            className="font-mono text-3xs uppercase tracking-[0.6px] px-[5px] py-[1px] rounded-xs"
            style={{
              color: "var(--color-fg-dim)",
              background: "var(--color-bg3)",
            }}
          >
            {providerInitial(judgment.provider)}
          </span>
          {handled ? (
            <span
              className="font-mono text-3xs uppercase tracking-[0.6px] px-[5px] py-[1px] rounded-xs"
              style={{
                color: "var(--color-fg-dim)",
                background: "var(--color-bg3)",
              }}
            >
              {judgment.userAction}
            </span>
          ) : judgment.suggestedFlag !== "keep" ? (
            <span
              className="inline-flex items-center gap-1 font-mono text-2xs"
              style={{ color: "var(--color-fg-dim)" }}
            >
              <Kbd>.</Kbd>
              <span>accept</span>
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span
            className="font-mono text-2xs tabular-nums"
            style={{ color: "var(--color-fg-dim)" }}
            title="composition / aesthetic (0-10) · cluster rank"
          >
            {judgment.composition}/{judgment.aesthetic}
            {judgment.clusterRank != null && ` · #${judgment.clusterRank}`}
          </span>
        </div>
      </header>
      <p
        className="px-2.5 py-2 text-[11px] leading-[1.5]"
        style={{ color: "var(--color-fg)" }}
      >
        {humanizeCuratorReason(
          judgment.reason,
          (id) => images.find((i) => i.id === id)?.filename ?? null,
        )}
      </p>
    </section>
  );
}

function providerInitial(provider: string): string {
  switch (provider) {
    case "anthropic":
      return "A";
    case "gemini":
      return "G";
    case "local":
      return "L";
    default:
      return "?";
  }
}

function providerLabel(provider: string): string {
  switch (provider) {
    case "anthropic":
      return "Anthropic";
    case "gemini":
      return "Gemini";
    case "local":
      return "Local";
    default:
      return provider || "unknown";
  }
}
