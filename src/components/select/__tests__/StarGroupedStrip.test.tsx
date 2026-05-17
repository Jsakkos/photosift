/// Tests for the Select filmstrip's cluster color-coding. `buildGroupInfo`
/// is the pure core: it maps the display list to a stable per-group color
/// + member count so the strip can mark which thumbnails belong together
/// (and the user can verify regrouping at a glance).

import { describe, it, expect } from "vitest";
import { buildGroupInfo, GROUP_PALETTE } from "../StarGroupedStrip";
import { makeImage } from "../../../test/fixtures";
import type { DisplayItem } from "../../../types";

function di(id: number, groupId?: number): DisplayItem {
  return {
    imageIndex: id,
    image: makeImage({ id }),
    ...(groupId != null ? { groupId } : {}),
  };
}

describe("buildGroupInfo", () => {
  it("gives every visible member of one cluster the same entry", () => {
    const info = buildGroupInfo([di(1, 10), di(2, 10), di(3, 10)]);
    const g = info.get(10);
    expect(g).toBeDefined();
    expect(g!.count).toBe(3);
  });

  it("gives distinct clusters distinct, consecutive palette colors", () => {
    const info = buildGroupInfo([di(1, 10), di(2, 10), di(3, 20), di(4, 20)]);
    expect(info.get(10)!.color).toBe(GROUP_PALETTE[0]);
    expect(info.get(20)!.color).toBe(GROUP_PALETTE[1]);
    expect(info.get(10)!.color).not.toBe(info.get(20)!.color);
  });

  it("omits singletons and ungrouped photos", () => {
    const info = buildGroupInfo([di(1), di(2), di(3)]);
    expect(info.size).toBe(0);
  });

  it("omits a group with only one visible member", () => {
    // Photo 1's burst siblings were filtered out by the star floor; a
    // lone visible member is not a cluster worth marking.
    const info = buildGroupInfo([di(1, 10), di(2, 20), di(3, 20)]);
    expect(info.has(10)).toBe(false);
    expect(info.has(20)).toBe(true);
  });

  it("cycles the palette after 8 clusters", () => {
    const items: DisplayItem[] = [];
    for (let g = 1; g <= 9; g++) {
      items.push(di(g * 100, g), di(g * 100 + 1, g));
    }
    const info = buildGroupInfo(items);
    expect(GROUP_PALETTE).toHaveLength(8);
    expect(info.get(1)!.color).toBe(GROUP_PALETTE[0]);
    expect(info.get(9)!.color).toBe(GROUP_PALETTE[0]); // 9th wraps to the first
  });
});
