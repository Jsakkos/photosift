import { useEffect, useMemo, useState } from "react";
import { useProjectStore } from "../../stores/projectStore";
import { getBracketDecisionsForShoot } from "../../lib/bracketApi";
import { imageUrl } from "../../hooks/useImageLoader";
import type { BracketDecision } from "../../types";
import { BracketTree } from "./BracketTree";

type Modal =
  | { kind: "photo"; id: number }
  | { kind: "pair"; left: number; right: number }
  | null;

/// Retrospective view of every tournament bracket in the shoot — the
/// user's own Select picks and the Curator's derived ranking, side by
/// side per group. Read-only: clicking a frame opens it for a closer
/// look, clicking ⇄ opens a pair 2-up.
export function ReviewShell() {
  const currentShoot = useProjectStore((s) => s.currentShoot);
  const images = useProjectStore((s) => s.images);
  const groups = useProjectStore((s) => s.groups);

  const [decisions, setDecisions] = useState<BracketDecision[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [modal, setModal] = useState<Modal>(null);

  useEffect(() => {
    if (!currentShoot) return;
    let cancelled = false;
    setLoaded(false);
    getBracketDecisionsForShoot(currentShoot.id)
      .then((d) => {
        if (!cancelled) {
          setDecisions(d);
          setLoaded(true);
        }
      })
      .catch(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [currentShoot]);

  const filenameOf = useMemo(() => {
    const map = new Map<number, string>();
    for (const img of images) map.set(img.id, img.filename);
    return (id: number) => map.get(id) ?? `#${id}`;
  }, [images]);

  // Stable 1-based group ordinal for display, in the shoot's group order.
  const groupOrdinal = useMemo(() => {
    const map = new Map<number, number>();
    groups.forEach((g, i) => map.set(g.id, i + 1));
    return map;
  }, [groups]);

  // groupId → { user: BracketDecision[]; curator: BracketDecision[] }
  const byGroup = useMemo(() => {
    const map = new Map<
      number,
      { user: BracketDecision[]; curator: BracketDecision[] }
    >();
    for (const d of decisions) {
      const entry = map.get(d.groupId) ?? { user: [], curator: [] };
      if (d.source === "curator") entry.curator.push(d);
      else entry.user.push(d);
      map.set(d.groupId, entry);
    }
    return [...map.entries()].sort(
      (a, b) => (groupOrdinal.get(a[0]) ?? 0) - (groupOrdinal.get(b[0]) ?? 0),
    );
  }, [decisions, groupOrdinal]);

  return (
    <div data-testid="review-shell" className="flex-1 flex flex-col overflow-hidden">
      <div
        className="h-11 flex items-center px-4 gap-3 shrink-0 border-b"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
      >
        <span
          className="font-mono text-2xs uppercase tracking-[1px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          Review · tournament history
        </span>
        <span
          className="font-mono text-2xs tabular-nums"
          style={{ color: "var(--color-fg-mute)" }}
        >
          {byGroup.length} group{byGroup.length === 1 ? "" : "s"}
        </span>
      </div>

      <div className="flex-1 overflow-y-auto p-4" style={{ background: "var(--color-stage)" }}>
        {!loaded ? (
          <p className="text-sm" style={{ color: "var(--color-fg-dim)" }}>
            Loading…
          </p>
        ) : byGroup.length === 0 ? (
          <div className="max-w-md mx-auto mt-12 text-center flex flex-col gap-2">
            <p className="text-sm font-medium" style={{ color: "var(--color-fg)" }}>
              No tournament history yet
            </p>
            <p className="text-xs leading-relaxed" style={{ color: "var(--color-fg-dim)" }}>
              Run a tournament in Select (press <kbd>Tab</kbd> on a group) or
              run the Curator — each decision shows up here as a bracket you
              can walk back through.
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-6">
            {byGroup.map(([groupId, entry]) => (
              <section
                key={groupId}
                className="rounded-md p-3"
                style={{
                  background: "var(--color-bg)",
                  border: "1px solid var(--color-border)",
                }}
              >
                <h3
                  className="font-mono text-2xs uppercase tracking-[0.6px] mb-3"
                  style={{ color: "var(--color-fg-dim)" }}
                >
                  Group {groupOrdinal.get(groupId) ?? groupId}
                </h3>
                {entry.user.length > 0 && (
                  <BracketBlock
                    label="Your picks"
                    decisions={entry.user}
                    filenameOf={filenameOf}
                    onOpenPhoto={(id) => setModal({ kind: "photo", id })}
                    onOpenPair={(left, right) =>
                      setModal({ kind: "pair", left, right })
                    }
                  />
                )}
                {entry.curator.length > 0 && (
                  <BracketBlock
                    label="Curator ranking"
                    decisions={entry.curator}
                    filenameOf={filenameOf}
                    onOpenPhoto={(id) => setModal({ kind: "photo", id })}
                    onOpenPair={(left, right) =>
                      setModal({ kind: "pair", left, right })
                    }
                  />
                )}
              </section>
            ))}
          </div>
        )}
      </div>

      {modal && <ViewerModal modal={modal} onClose={() => setModal(null)} />}
    </div>
  );
}

function BracketBlock({
  label,
  decisions,
  filenameOf,
  onOpenPhoto,
  onOpenPair,
}: {
  label: string;
  decisions: BracketDecision[];
  filenameOf: (id: number) => string;
  onOpenPhoto: (id: number) => void;
  onOpenPair: (left: number, right: number) => void;
}) {
  return (
    <div className="mb-2 last:mb-0">
      <span
        className="block font-mono text-3xs uppercase tracking-[0.6px] mb-1.5"
        style={{ color: "var(--color-fg-mute)" }}
      >
        {label}
      </span>
      <BracketTree
        decisions={decisions}
        filenameOf={filenameOf}
        onOpenPhoto={onOpenPhoto}
        onOpenPair={onOpenPair}
      />
    </div>
  );
}

function ViewerModal({ modal, onClose }: { modal: NonNullable<Modal>; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-8"
      style={{ background: "rgba(0,0,0,0.85)" }}
      onClick={onClose}
    >
      <div
        className={`flex gap-3 ${modal.kind === "pair" ? "flex-row" : ""}`}
        onClick={(e) => e.stopPropagation()}
      >
        {modal.kind === "photo" ? (
          <img
            src={imageUrl(modal.id)}
            alt=""
            className="max-h-[85vh] max-w-[90vw] object-contain rounded"
            draggable={false}
          />
        ) : (
          <>
            <img
              src={imageUrl(modal.left)}
              alt=""
              className="max-h-[85vh] max-w-[45vw] object-contain rounded"
              draggable={false}
            />
            <img
              src={imageUrl(modal.right)}
              alt=""
              className="max-h-[85vh] max-w-[45vw] object-contain rounded"
              draggable={false}
            />
          </>
        )}
      </div>
    </div>
  );
}
