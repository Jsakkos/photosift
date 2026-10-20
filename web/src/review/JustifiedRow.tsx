import { useMemo, type ReactNode } from "react";

import { packRows, type JustifyItem } from "../lib/justify";
import { useMeasuredWidth } from "./useMeasuredWidth";

interface Props {
  items: readonly JustifyItem[];
  targetHeight: number;
  gap: number;
  className?: string;
  render: (id: number, width: number, height: number) => ReactNode;
}

/**
 * Fixed-height rows that fill the width: each photo gets exactly
 * `height * aspect` of room, so none is ever cropped, and the size is known
 * before any image loads (no layout shift).
 */
export function JustifiedRows({ items, targetHeight, gap, className, render }: Props) {
  const [ref, width] = useMeasuredWidth<HTMLDivElement>();
  const rows = useMemo(
    () => (width > 0 ? packRows(items, width, targetHeight, gap) : []),
    [items, width, targetHeight, gap],
  );
  return (
    <div ref={ref} className={className} style={{ display: "flex", flexDirection: "column", gap }}>
      {rows.map((row) => (
        <div
          key={row.items[0]?.id}
          className="jrow"
          style={{ display: "flex", gap, alignItems: "flex-start" }}
        >
          {row.items.map((item) => (
            <div key={item.id} style={{ flex: "none", width: item.width }}>
              {render(item.id, item.width, row.height)}
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}
