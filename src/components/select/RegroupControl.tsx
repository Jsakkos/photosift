import { useState, useRef, useEffect } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";

/// Inline regroup control for the Select view. Grouping clusters similar
/// frames so the tournament can compare them 2-up; a single pHash
/// hamming-distance threshold drives it. This panel lets the user retune
/// that threshold for the shoot in front of them — without leaving the
/// cull or touching the global default. Higher = looser grouping.
export function RegroupControl() {
  const currentShoot = useProjectStore((s) => s.currentShoot);
  const refetchGroups = useProjectStore((s) => s.refetchGroups);
  const setToast = useProjectStore((s) => s.setToast);
  const settings = useSettingsStore((s) => s.settings);
  const reclusterShootWith = useSettingsStore((s) => s.reclusterShootWith);

  const [open, setOpen] = useState(false);
  const [threshold, setThreshold] = useState(settings.groupThreshold);
  const [busy, setBusy] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  // Re-seed the slider from the live settings each time the panel opens
  // so it always reflects the current grouping baseline.
  useEffect(() => {
    if (open) {
      setThreshold(settings.groupThreshold);
    }
  }, [open, settings.groupThreshold]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open]);

  if (!currentShoot) return null;

  const apply = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const count = await reclusterShootWith(
        currentShoot.id,
        threshold,
        settings.groupTimeWindowS,
      );
      // Group ids are rebuilt — any in-flight tournament and the
      // per-floor visited set now reference stale ids.
      useProjectStore.setState({
        selectBracket: null,
        selectBracketSuppressedForGroup: null,
        selectVisitedAtFloor: new Set<number>(),
      });
      await refetchGroups();
      setToast(`Regrouped into ${count} group${count === 1 ? "" : "s"}`);
      setOpen(false);
    } catch (e) {
      setToast(`Couldn't regroup — ${String(e)}`, "error");
    }
    setBusy(false);
  };

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        tabIndex={-1}
        onClick={() => setOpen((v) => !v)}
        aria-pressed={open}
        aria-label="Regroup this shoot"
        title="Retune burst grouping for this shoot"
        className="inline-flex items-center gap-1.5 font-mono text-2xs uppercase tracking-[0.6px] px-1.5 py-[3px] rounded-xs bg-transparent border-0 cursor-pointer"
        style={{ color: open ? "var(--color-fg-dim)" : "var(--color-fg-mute)" }}
      >
        <span>regroup</span>
      </button>
      {open && (
        <div
          className="absolute right-0 top-full mt-1 z-30 w-64 p-3 rounded-md flex flex-col gap-3"
          style={{
            background: "var(--color-bg2)",
            border: "1px solid var(--color-border)",
            boxShadow: "0 8px 24px rgba(0,0,0,0.45)",
          }}
        >
          <p className="text-3xs leading-snug" style={{ color: "var(--color-fg-mute)" }}>
            Higher = looser grouping: more frames pulled into each cluster
            for 2-up comparison. Applies to this shoot only.
          </p>
          <label className="flex flex-col gap-1">
            <span className="flex items-center justify-between">
              <span
                className="font-mono text-2xs uppercase tracking-[0.6px]"
                style={{ color: "var(--color-fg-dim)" }}
              >
                grouping similarity
              </span>
              <span
                className="font-mono text-2xs tabular-nums"
                style={{ color: "var(--color-fg)" }}
              >
                {threshold}
              </span>
            </span>
            <input
              type="range"
              min={0}
              max={48}
              value={threshold}
              tabIndex={-1}
              onChange={(e) => setThreshold(Number(e.target.value))}
              className="w-full cursor-pointer"
              style={{ accentColor: "var(--color-accent)" }}
            />
            <span className="text-3xs" style={{ color: "var(--color-fg-mute)" }}>
              ≤ this pHash distance = same group
            </span>
          </label>
          <button
            type="button"
            tabIndex={-1}
            onClick={apply}
            disabled={busy}
            className="self-end px-3 py-1 rounded-xs font-mono text-2xs uppercase tracking-[0.6px] cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
            style={{
              background: "var(--color-accent)",
              color: "var(--color-on-accent)",
              border: 0,
            }}
          >
            {busy ? "Regrouping…" : "Regroup"}
          </button>
        </div>
      )}
    </div>
  );
}
