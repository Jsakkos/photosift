import { useMemo } from "react";

import { previewUrl } from "../api/client";
import { JustifiedRows } from "./JustifiedRow";
import { aspectOf, type PhotoMap } from "./model";
import { PhotoTile, type NoteHandler } from "./PhotoTile";

interface Props {
  ids: readonly number[];
  photos: PhotoMap;
  focusId: number | null;
  onSelect: (id: number) => void;
  onOpen: (id: number) => void;
  onNote: NoteHandler;
}

/** The strongest shots, large and loaded eagerly so the first seconds feel good. */
export function Highlights({ ids, photos, focusId, onSelect, onOpen, onNote }: Props) {
  const items = useMemo(
    () => ids.map((id) => ({ id, aspect: aspectOf(photos[String(id)]) })),
    [ids, photos],
  );
  if (ids.length === 0) return null;
  return (
    <section className="highlights" aria-label="Highlights">
      <h2>Highlights</h2>
      <JustifiedRows
        items={items}
        targetHeight={300}
        gap={10}
        render={(id, width, height) => {
          const photo = photos[String(id)];
          return photo ? (
            <PhotoTile
              photo={photo}
              kind="highlight"
              width={width}
              height={height}
              src={previewUrl(id)}
              focused={focusId === id}
              onSelect={onSelect}
              onOpen={onOpen}
              onNote={onNote}
            />
          ) : null;
        }}
      />
    </section>
  );
}
