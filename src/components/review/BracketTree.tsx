import { thumbUrl } from "../../hooks/useImageLoader";
import type { BracketDecision } from "../../types";

const THUMB_W = 84;
const THUMB_H = 56;

type Props = {
  /// All decisions for one (group, source) bracket.
  decisions: BracketDecision[];
  filenameOf: (photoId: number) => string;
  onOpenPhoto: (photoId: number) => void;
  onOpenPair: (left: number, right: number) => void;
};

/// Renders one tournament bracket as rounds-of-columns. Each pair node
/// shows the two contenders with the winner ringed; clicking a thumbnail
/// opens it, clicking the ⇄ button opens the pair side-by-side.
export function BracketTree({
  decisions,
  filenameOf,
  onOpenPhoto,
  onOpenPair,
}: Props) {
  const byRound = new Map<number, BracketDecision[]>();
  for (const d of decisions) {
    const arr = byRound.get(d.roundIndex) ?? [];
    arr.push(d);
    byRound.set(d.roundIndex, arr);
  }
  const rounds = [...byRound.entries()].sort((a, b) => a[0] - b[0]);
  for (const [, arr] of rounds) arr.sort((a, b) => a.pairIndex - b.pairIndex);

  return (
    <div className="flex gap-5 overflow-x-auto pb-1">
      {rounds.map(([roundIdx, pairs]) => (
        <div key={roundIdx} className="flex flex-col gap-3 shrink-0">
          <span
            className="font-mono text-3xs uppercase tracking-[0.6px]"
            style={{ color: "var(--color-fg-mute)" }}
          >
            Round {roundIdx + 1}
          </span>
          {pairs.map((p) => (
            <PairNode
              key={p.pairIndex}
              d={p}
              filenameOf={filenameOf}
              onOpenPhoto={onOpenPhoto}
              onOpenPair={onOpenPair}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

function PairNode({
  d,
  filenameOf,
  onOpenPhoto,
  onOpenPair,
}: {
  d: BracketDecision;
  filenameOf: (photoId: number) => string;
  onOpenPhoto: (photoId: number) => void;
  onOpenPair: (left: number, right: number) => void;
}) {
  const leftWon = d.decision === "L" || d.decision === "both" || d.decision === "bye";
  const rightWon = d.decision === "R" || d.decision === "both";

  return (
    <div
      className="flex flex-col gap-1 p-1.5 rounded-md"
      style={{ background: "var(--color-bg2)", border: "1px solid var(--color-border)" }}
    >
      <Thumb
        photoId={d.leftPhotoId}
        won={leftWon}
        label={filenameOf(d.leftPhotoId)}
        onClick={() => onOpenPhoto(d.leftPhotoId)}
      />
      {d.rightPhotoId !== null ? (
        <>
          <button
            type="button"
            tabIndex={-1}
            onClick={() => onOpenPair(d.leftPhotoId, d.rightPhotoId as number)}
            title="Compare this pair side-by-side"
            className="self-center font-mono text-3xs px-1.5 rounded-xs cursor-pointer border-0"
            style={{ background: "var(--color-bg3)", color: "var(--color-fg-dim)" }}
          >
            ⇄ vs
          </button>
          <Thumb
            photoId={d.rightPhotoId}
            won={rightWon}
            label={filenameOf(d.rightPhotoId)}
            onClick={() => onOpenPhoto(d.rightPhotoId as number)}
          />
        </>
      ) : (
        <span
          className="self-center font-mono text-3xs uppercase tracking-[0.6px]"
          style={{ color: "var(--color-fg-mute)" }}
        >
          bye
        </span>
      )}
    </div>
  );
}

function Thumb({
  photoId,
  won,
  label,
  onClick,
}: {
  photoId: number;
  won: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      onClick={onClick}
      title={`${label}${won ? " — winner" : ""}`}
      className="relative block rounded-xs overflow-hidden cursor-pointer p-0 border-0"
      style={{
        width: THUMB_W,
        height: THUMB_H,
        outline: won ? "2px solid var(--color-accent)" : "1px solid var(--color-border)",
        outlineOffset: won ? -2 : -1,
        opacity: won ? 1 : 0.6,
      }}
    >
      <img
        src={thumbUrl(photoId)}
        alt={label}
        className="w-full h-full object-cover"
        draggable={false}
      />
      {won && (
        <span
          className="absolute top-0.5 right-0.5 w-3 h-3 rounded-full flex items-center justify-center text-[8px] font-bold"
          style={{ background: "var(--color-accent)", color: "var(--color-on-accent)" }}
          aria-hidden="true"
        >
          ✓
        </span>
      )}
    </button>
  );
}
