import { describe, expect, it } from "vitest";

import { packRows } from "./justify";

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

describe("packRows", () => {
  it("returns no rows for no items", () => {
    expect(packRows([], 1000, 200, 6)).toEqual([]);
  });

  it("fits three photos on one row that fills the width", () => {
    const items = [
      { id: 1, aspect: 1.5 },
      { id: 2, aspect: 0.667 },
      { id: 3, aspect: 1.5 },
    ];
    const rows = packRows(items, 1000, 200, 8);
    expect(rows).toHaveLength(1);
    const row = rows[0]!;
    expect(sum(row.items.map((i) => i.width))).toBeCloseTo(1000 - 2 * 8, 0);
    row.items.forEach((item, i) => {
      expect(item.width / row.height).toBeCloseTo(items[i]!.aspect, 2);
    });
  });

  it("fills every row but the last exactly and keeps the last at the target height", () => {
    const items = Array.from({ length: 23 }, (_, i) => ({
      id: i,
      aspect: i % 3 === 0 ? 0.75 : 1.5,
    }));
    const rows = packRows(items, 900, 180, 4);
    expect(rows.length).toBeGreaterThan(2);
    for (const row of rows.slice(0, -1)) {
      const gaps = (row.items.length - 1) * 4;
      expect(Math.abs(sum(row.items.map((i) => i.width)) + gaps - 900)).toBeLessThanOrEqual(1);
    }
    const last = rows[rows.length - 1]!;
    expect(last.height).toBeLessThanOrEqual(180 + 0.001);
    expect(sum(last.items.map((i) => i.width)) + (last.items.length - 1) * 4).toBeLessThanOrEqual(901);
  });

  it("never crops: every width is height times aspect, and every item appears once in order", () => {
    const items = Array.from({ length: 40 }, (_, i) => ({
      id: 100 + i,
      aspect: [1.5, 0.667, 1.333, 0.75, 1][i % 5]!,
    }));
    const rows = packRows(items, 1234, 220, 6);
    const flat = rows.flatMap((r) => r.items.map((i) => ({ ...i, h: r.height })));
    expect(flat.map((i) => i.id)).toEqual(items.map((i) => i.id));
    flat.forEach((f, i) => expect(f.width / f.h).toBeCloseTo(items[i]!.aspect, 2));
  });

  it("gives a single very wide panorama its own row", () => {
    const rows = packRows(
      [
        { id: 1, aspect: 1.5 },
        { id: 2, aspect: 6 },
        { id: 3, aspect: 1.5 },
      ],
      1000,
      200,
      6,
    );
    const pano = rows.find((r) => r.items.some((i) => i.id === 2))!;
    expect(pano.items).toHaveLength(1);
    expect(pano.items[0]!.width).toBeCloseTo(1000, 0);
    expect(pano.items[0]!.width / pano.height).toBeCloseTo(6, 2);
  });

  it("shrinks a lone wide row below the target rather than overflow", () => {
    const rows = packRows([{ id: 1, aspect: 9 }], 1000, 200, 6);
    expect(rows).toHaveLength(1);
    expect(rows[0]!.items[0]!.width).toBeLessThanOrEqual(1000);
  });

  it("leaves a short last row at the target height instead of stretching it", () => {
    const rows = packRows([{ id: 1, aspect: 1.5 }], 1000, 200, 6);
    expect(rows[0]!.height).toBe(200);
    expect(rows[0]!.items[0]!.width).toBe(300);
  });

  it("handles a zero-width container without looping or NaN", () => {
    const rows = packRows([{ id: 1, aspect: 1.5 }], 0, 200, 6);
    expect(rows.flatMap((r) => r.items).every((i) => Number.isFinite(i.width))).toBe(true);
  });
});
