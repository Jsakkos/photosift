import { useMemo } from "react";

import { previewUrl } from "../api/client";
import { packRows } from "../lib/justify";
import { aspectOf, type PhotoMap } from "./model";
import { PhotoTile } from "./PhotoTile";
import { useMeasuredWidth } from "./useMeasuredWidth";

const GAP = 12;
const MAX_HEIGHT_FRACTION = 0.78;

interface Props {
  ids: readonly number[];
  photos: PhotoMap;
  focusId: number | null;
  onSelect: (id: number) => void;
}

/** The biggest uniform row height at which all frames fit the window without scrolling. */
function fit(items: { id: number; aspect: number }[], width: number, available: number) {
  let target = available;
  let rows = packRows(items, width, target, GAP);
  for (let i = 0; i < 24; i += 1) {
    const total = rows.reduce((sum, r) => sum + r.height, 0) + GAP * (rows.length - 1);
    if (total <= available || target < 60) break;
    target *= 0.9;
    rows = packRows(items, width, target, GAP);
  }
  return rows;
}

/** A burst's frames side by side, each whole, to choose between near-duplicates. */
export function Compare({ ids, photos, focusId, onSelect }: Props) {
  const [ref, width] = useMeasuredWidth<HTMLDivElement>();
  const items = useMemo(
    () => ids.map((id) => ({ id, aspect: aspectOf(photos[String(id)]) })),
    [ids, photos],
  );
  const available = Math.round(window.innerHeight * MAX_HEIGHT_FRACTION);
  const rows = useMemo(
    () => (width > 0 ? fit(items, width, available) : []),
    [items, width, available],
  );
  return (
    <div className="compare" data-testid="compare" role="dialog" aria-label="Compare this burst">
      <div className="compare-body" ref={ref}>
        {rows.map((row) => (
          <div key={row.items[0]?.id} className="compare-row" style={{ gap: GAP }}>
            {row.items.map((item) => {
              const photo = photos[String(item.id)];
              return photo ? (
                <PhotoTile
                  key={item.id}
                  photo={photo}
                  kind="compare"
                  width={item.width}
                  height={row.height}
                  src={previewUrl(item.id)}
                  focused={focusId === item.id}
                  onSelect={onSelect}
                />
              ) : null;
            })}
          </div>
        ))}
      </div>
      <div className="loupe-keys muted">
        ← → move between frames · A album · L library · R archive · X delete? · B blooper · Esc close
      </div>
    </div>
  );
}
