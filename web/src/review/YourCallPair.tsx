import { thumbUrl } from "../api/client";
import { JustifiedRows } from "./JustifiedRow";
import { aspectOf, type PhotoMap, type ViewPair } from "./model";
import { PhotoTile } from "./PhotoTile";
import type { NoteHandler } from "./PhotoTile";

const PAIR_HEIGHT = 260;
const GAP = 12;

interface Props {
  pair: ViewPair;
  photos: PhotoMap;
  focusId: number | null;
  onSelect: (id: number) => void;
  onOpen: (id: number) => void;
  onNote: NoteHandler;
}

/** Two frames the helper could not rank, whole and side by side, with what it weighed. */
export function YourCallPair({ pair, photos, focusId, onSelect, onOpen, onNote }: Props) {
  const items = [pair.a, pair.b].map((id) => ({ id, aspect: aspectOf(photos[String(id)]) }));
  return (
    <div className="your-call" data-your-call-id={pair.id}>
      <h4>Your call: one or the other?</h4>
      <JustifiedRows
        items={items}
        targetHeight={PAIR_HEIGHT}
        gap={GAP}
        render={(id, width, height) => {
          const photo = photos[String(id)];
          return photo ? (
            <PhotoTile
              photo={photo}
              kind="pair"
              width={width}
              height={height}
              src={thumbUrl(id)}
              focused={focusId === id}
              onSelect={onSelect}
              onOpen={onOpen}
              onNote={onNote}
            />
          ) : null;
        }}
      />
      <p className="tradeoff">{pair.tradeoff}</p>
    </div>
  );
}
