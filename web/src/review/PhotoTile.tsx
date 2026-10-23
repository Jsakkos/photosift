import type { Criterion, ReviewPhoto } from "../api/types";
import { OUTCOME_LABEL } from "./model";
import { OverrideWhy } from "./OverrideWhy";

export type TileKind = "large" | "strip" | "highlight" | "compare" | "pair";

export type NoteHandler = (photoId: number, criterion: Criterion, note?: string) => void;

interface Props {
  photo: ReviewPhoto;
  kind: TileKind;
  width: number;
  height: number;
  src: string;
  focused: boolean;
  lazy?: boolean;
  /** Frames in this photo's burst; only exposed as `data-burst-size` for tests. */
  burstSize?: number;
  onSelect: (id: number) => void;
  onOpen?: (id: number) => void;
  onNote?: NoteHandler;
}

/** One photo, shown whole in a box of exactly its own aspect ratio. */
export function PhotoTile({
  photo,
  kind,
  width,
  height,
  src,
  focused,
  lazy,
  burstSize,
  onSelect,
  onOpen,
  onNote,
}: Props) {
  const attrs =
    kind === "highlight"
      ? { "data-highlight-id": photo.id }
      : kind === "compare"
        ? { "data-compare-id": photo.id }
        : { "data-photo-id": photo.id };
  const className = kind === "highlight" ? "hl" : kind === "compare" ? "compare-frame" : "tile";
  const proposal = photo.proposal;
  const suggesting = proposal !== null && photo.outcome === "unreviewed";
  const overridden = proposal !== null && photo.outcome !== "unreviewed" && photo.outcome !== proposal.outcome;
  const showsReason = kind === "large" || kind === "highlight" || kind === "pair";
  return (
    <div
      className={`${className} kind-${kind}`}
      {...attrs}
      data-outcome={photo.outcome}
      data-blooper={photo.blooper}
      data-focused={focused}
      data-large={kind === "large"}
      data-burst-size={burstSize}
      // Strips stay compact: the reason is a tooltip there.
      title={kind === "strip" && proposal ? proposal.reason : undefined}
      style={{ width }}
    >
      <div
        className="frame"
        style={{ width, height }}
        onClick={() => onSelect(photo.id)}
        onDoubleClick={() => onOpen?.(photo.id)}
      >
        <img
          src={src}
          alt=""
          width={Math.round(width)}
          height={Math.round(height)}
          style={{ width, height }}
          loading={lazy ? "lazy" : "eager"}
          decoding="async"
          draggable={false}
        />
        {photo.outcome !== "unreviewed" && (
          <span className={`badge badge-${photo.outcome}`}>{OUTCOME_LABEL[photo.outcome]}</span>
        )}
        {suggesting && (
          <span className="badge badge-suggest" data-suggested={proposal.outcome}>
            suggest: {OUTCOME_LABEL[proposal.outcome]}
            {proposal.blooper ? " + blooper" : ""}
          </span>
        )}
        {photo.blooper && <span className="badge badge-blooper">blooper</span>}
        {photo.has_video && <span className="badge badge-video">video</span>}
      </div>
      {showsReason && (
        <div className="why" data-slot="reason" data-overridden={overridden || undefined}>
          {proposal && (overridden ? <s>{proposal.reason}</s> : proposal.reason)}
        </div>
      )}
      {showsReason && overridden && onNote && (
        <OverrideWhy photoId={photo.id} chosen={proposal.criterion} onNote={onNote} />
      )}
    </div>
  );
}
