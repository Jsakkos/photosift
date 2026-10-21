import { memo, useMemo } from "react";

import { thumbUrl } from "../api/client";
import { OUTCOME_LABEL, FILTER_ORDER, aspectOf, momentTitle, plural } from "./model";
import type { PhotoMap, ViewMoment } from "./model";
import { JustifiedRows } from "./JustifiedRow";
import { PhotoTile, type NoteHandler } from "./PhotoTile";
import { YourCallPair } from "./YourCallPair";

const LARGE_HEIGHT = 200;
const STRIP_HEIGHT = 72;
const GAP = 8;

interface Props {
  moment: ViewMoment;
  photos: PhotoMap;
  focusId: number | null;
  withDay: boolean;
  onSelect: (id: number) => void;
  onOpen: (id: number) => void;
  onNote: NoteHandler;
}

function summary(moment: ViewMoment): string {
  const parts = FILTER_ORDER.filter((o) => moment.byOutcome[o] > 0 && o !== "unreviewed").map(
    (o) => `${moment.byOutcome[o]} ${OUTCOME_LABEL[o].toLowerCase()}`,
  );
  if (moment.bloopers > 0) parts.push(plural(moment.bloopers, "blooper", "bloopers"));
  if (moment.byOutcome.unreviewed > 0) parts.push(`${moment.byOutcome.unreviewed} to decide`);
  return parts.join(" · ");
}

/** One moment: each burst's best frame large, its other frames in a dim strip. */
export const MomentSection = memo(function MomentSection({
  moment,
  photos,
  focusId,
  withDay,
  onSelect,
  onOpen,
  onNote,
}: Props) {
  const tops = useMemo(
    () =>
      moment.bursts.map((b) => ({ id: b.top, aspect: aspectOf(photos[String(b.top)]) })),
    [moment.bursts, photos],
  );
  const sizeOf = useMemo(() => {
    const sizes = new Map<number, number>();
    for (const b of moment.bursts) {
      sizes.set(b.top, b.rest.length + 1);
      for (const id of b.rest) sizes.set(id, b.rest.length + 1);
    }
    return sizes;
  }, [moment.bursts]);
  const hasStrip = moment.bursts.some((b) => b.rest.length > 0);

  return (
    <section className="moment" data-moment={moment.idx}>
      <header className="moment-head">
        <h3>{momentTitle(moment.start, withDay)}</h3>
        <span className="muted">
          {plural(moment.shots, "shot", "shots")}
          {summary(moment) && ` → ${summary(moment)}`}
        </span>
      </header>
      {moment.pairs.map((pair) => (
        <YourCallPair
          key={pair.id}
          pair={pair}
          photos={photos}
          focusId={focusId}
          onSelect={onSelect}
          onOpen={onOpen}
          onNote={onNote}
        />
      ))}
      <JustifiedRows
        items={tops}
        targetHeight={LARGE_HEIGHT}
        gap={GAP}
        render={(id, width, height) => {
          const photo = photos[String(id)];
          return photo ? (
            <PhotoTile
              photo={photo}
              kind="large"
              width={width}
              height={height}
              src={thumbUrl(id)}
              focused={focusId === id}
              burstSize={sizeOf.get(id)}
              onSelect={onSelect}
              onOpen={onOpen}
              onNote={onNote}
            />
          ) : null;
        }}
      />
      {hasStrip && (
        <div className="strip">
          {moment.bursts
            .filter((b) => b.rest.length > 0)
            .map((b) => (
              <div className="strip-burst" key={b.idx}>
                {b.rest.map((id) => {
                  const photo = photos[String(id)];
                  if (!photo) return null;
                  const width = aspectOf(photo) * STRIP_HEIGHT;
                  return (
                    <PhotoTile
                      key={id}
                      photo={photo}
                      kind="strip"
                      width={width}
                      height={STRIP_HEIGHT}
                      src={thumbUrl(id)}
                      focused={focusId === id}
                      lazy
                      burstSize={sizeOf.get(id)}
                      onSelect={onSelect}
                      onOpen={onOpen}
                    />
                  );
                })}
              </div>
            ))}
        </div>
      )}
    </section>
  );
});
