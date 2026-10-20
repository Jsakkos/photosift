import type { ReviewPhoto } from "../api/types";
import { OUTCOME_LABEL } from "./model";

export type TileKind = "large" | "strip" | "highlight" | "compare";

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
}: Props) {
  const attrs =
    kind === "highlight"
      ? { "data-highlight-id": photo.id }
      : kind === "compare"
        ? { "data-compare-id": photo.id }
        : { "data-photo-id": photo.id };
  const className = kind === "highlight" ? "hl" : kind === "compare" ? "compare-frame" : "tile";
  return (
    <div
      className={`${className} kind-${kind}`}
      {...attrs}
      data-outcome={photo.outcome}
      data-blooper={photo.blooper}
      data-focused={focused}
      data-large={kind === "large"}
      data-burst-size={burstSize}
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
        {photo.blooper && <span className="badge badge-blooper">blooper</span>}
        {photo.has_video && <span className="badge badge-video">video</span>}
      </div>
      {kind === "large" && <div className="why" data-slot="reason" />}
    </div>
  );
}
