/** One photo to place; `aspect` is displayed width / height. */
export interface JustifyItem {
  id: number;
  aspect: number;
}

export interface PackedRow {
  height: number;
  items: { id: number; width: number }[];
}

/** A row that would have to grow past this many times the target stays natural. */
const MAX_STRETCH = 1.4;
/** A shoot that fits on one row fills it, unless that would blow the photos up past this. */
const MAX_SOLE_ROW_STRETCH = 2;
const MIN_ASPECT = 0.05;

/**
 * Packs photos into rows like a printed contact sheet: every row but the last
 * is scaled so it fills `containerWidth` exactly, and every width is
 * `height * aspect`, so nothing is ever cropped. Rows come out near
 * `targetHeight`; a photo wider than the container on its own gets its own row,
 * and a last row keeps the target height rather than stretching (unless it is
 * the only row and fills the width with at most a 2x enlargement).
 */
export function packRows(
  items: readonly JustifyItem[],
  containerWidth: number,
  targetHeight: number,
  gap: number,
): PackedRow[] {
  const rows: PackedRow[] = [];
  let cur: JustifyItem[] = [];
  let curAspect = 0;

  const natural = (row: JustifyItem[]): PackedRow => ({
    height: targetHeight,
    items: row.map((i) => ({ id: i.id, width: i.aspect * targetHeight })),
  });
  const filled = (row: JustifyItem[], aspectSum: number): PackedRow => {
    const height = (containerWidth - gap * (row.length - 1)) / aspectSum;
    return {
      height,
      items: row.map((i) => ({ id: i.id, width: i.aspect * height })),
    };
  };
  const closeShort = () => {
    if (cur.length === 0) return;
    const stretched = filled(cur, curAspect);
    rows.push(stretched.height <= targetHeight * MAX_STRETCH ? stretched : natural(cur));
    cur = [];
    curAspect = 0;
  };

  for (const raw of items) {
    const item = { id: raw.id, aspect: Math.max(raw.aspect, MIN_ASPECT) };
    if (containerWidth <= 0) {
      rows.push(natural([item]));
      continue;
    }
    if (item.aspect * targetHeight >= containerWidth) {
      closeShort();
      rows.push(filled([item], item.aspect));
      continue;
    }
    const naturalWidth = (curAspect + item.aspect) * targetHeight + gap * cur.length;
    if (naturalWidth < containerWidth) {
      cur.push(item);
      curAspect += item.aspect;
      continue;
    }
    // Adding the item overshoots: keep it only if that row ends up closer to
    // the target height than the row without it would.
    const withAspect = curAspect + item.aspect;
    const heightWith = (containerWidth - gap * cur.length) / withAspect;
    const heightWithout = (containerWidth - gap * (cur.length - 1)) / curAspect;
    if (targetHeight / heightWith <= heightWithout / targetHeight) {
      cur.push(item);
      rows.push(filled(cur, withAspect));
      cur = [];
      curAspect = 0;
    } else {
      rows.push(filled(cur, curAspect));
      cur = [item];
      curAspect = item.aspect;
    }
  }

  if (cur.length > 0) {
    const fit = (containerWidth - gap * (cur.length - 1)) / curAspect;
    const soleRow = rows.length === 0 && fit <= targetHeight * MAX_SOLE_ROW_STRETCH;
    const height = soleRow ? fit : Math.min(targetHeight, fit);
    rows.push({
      height,
      items: cur.map((i) => ({ id: i.id, width: i.aspect * height })),
    });
  }
  return rows;
}
