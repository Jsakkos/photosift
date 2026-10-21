import { describe, expect, it } from "vitest";

import { prefetchWindow } from "./prefetch";

describe("prefetchWindow", () => {
  const order = [10, 11, 12, 13, 14, 15];

  it("takes the next three and the previous one", () => {
    expect(prefetchWindow(order, 2)).toEqual([13, 14, 15, 11]);
  });

  it("stops at both ends", () => {
    expect(prefetchWindow(order, 0)).toEqual([11, 12, 13]);
    expect(prefetchWindow(order, 5)).toEqual([14]);
    expect(prefetchWindow([], 0)).toEqual([]);
  });
});
