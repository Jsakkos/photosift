import { describe, expect, it } from "vitest";

import type { ReviewPhoto, ShootReview } from "../api/types";
import {
  MutationClock,
  acceptable,
  buildView,
  burstOf,
  mergePhotos,
  momentTitle,
  stepMoment,
  stepPhoto,
} from "./model";

function photo(
  id: number,
  outcome: ReviewPhoto["outcome"] = "unreviewed",
  proposal: ReviewPhoto["proposal"] = null,
): ReviewPhoto {
  return {
    id,
    w: 640,
    h: 480,
    capture_time: null,
    camera: null,
    outcome,
    blooper: false,
    face_count: 0,
    signal_score: null,
    has_video: false,
    proposal,
  };
}

function proposing(outcome: "album" | "archive" | "delete", blooper = false) {
  return { outcome, blooper, reason: "because", criterion: null };
}

const review: Pick<ShootReview, "moments" | "highlights"> = {
  highlights: [2, 5],
  moments: [
    {
      idx: 0,
      start: "2024-05-01T15:48:00",
      end: null,
      bursts: [
        { idx: 0, photo_ids: [2, 1] },
        { idx: 1, photo_ids: [3] },
      ],
    },
    { idx: 1, start: null, end: null, bursts: [{ idx: 2, photo_ids: [5, 4, 6] }] },
  ],
};
const photos = Object.fromEntries(
  [photo(1), photo(2, "album"), photo(3), photo(4, "album"), photo(5), photo(6)].map((p) => [
    String(p.id),
    p,
  ]),
);

describe("buildView", () => {
  it("lists large frames before strip frames inside a moment", () => {
    const view = buildView(review, photos, null);
    expect(view.moments[0]!.order).toEqual([2, 3, 1]);
    expect(view.moments[1]!.order).toEqual([5, 4, 6]);
    expect(view.order).toEqual([2, 3, 1, 5, 4, 6]);
  });

  it("counts every shot of a moment whatever the filter", () => {
    const view = buildView(review, photos, "album");
    expect(view.moments[0]!.shots).toBe(3);
    expect(view.moments[0]!.byOutcome.album).toBe(1);
  });

  it("filters photos, promotes the next frame to large and drops empty moments", () => {
    const view = buildView(review, photos, "album");
    expect(view.order).toEqual([2, 4]);
    expect(view.moments.map((m) => m.bursts.map((b) => b.top))).toEqual([[2], [4]]);
    expect(view.highlights).toEqual([2]);
    expect(buildView(review, photos, "delete").moments).toEqual([]);
  });

  it("skips ids the photo map does not know", () => {
    const view = buildView(review, { "2": photo(2) }, null);
    expect(view.order).toEqual([2]);
  });
});

describe("navigation", () => {
  const view = buildView(review, photos, null);

  it("steps and clamps", () => {
    expect(stepPhoto(view.order, 2, 1)).toBe(3);
    expect(stepPhoto(view.order, 6, 1)).toBe(6);
    expect(stepPhoto(view.order, 2, -1)).toBe(2);
    expect(stepPhoto(view.order, null, 1)).toBe(2);
    expect(stepPhoto(view.order, 999, 1)).toBe(2);
    expect(stepPhoto([], 1, 1)).toBeNull();
  });

  it("jumps to the first photo of the neighbouring moment", () => {
    expect(stepMoment(view, 1, 1)).toBe(5);
    expect(stepMoment(view, 4, -1)).toBe(2);
    expect(stepMoment(view, 2, -1)).toBe(2);
  });

  it("finds the burst of a strip frame, large frame first", () => {
    expect(burstOf(view, 1)).toEqual([2, 1]);
    expect(burstOf(view, 999)).toEqual([]);
  });
});

describe("momentTitle", () => {
  it("formats the camera-local clock without a timezone shift", () => {
    expect(momentTitle("2024-05-01T15:48:00")).toBe("3:48 pm");
    expect(momentTitle("2024-05-01T00:05:00")).toBe("12:05 am");
    expect(momentTitle(null)).toBe("Undated");
  });

  it("adds the day when the shoot spans several", () => {
    expect(momentTitle("2026-10-18T15:48:00", true)).toBe("Oct 18 · 3:48 pm");
    expect(momentTitle("2026-10-18T15:48:00", false)).toBe("3:48 pm");
  });
});

describe("mergePhotos", () => {
  it("keeps the on-screen value of photos the caller marks as newer than the fetch", () => {
    const fetched = { "1": photo(1, "unreviewed"), "2": photo(2, "unreviewed") };
    const current = { "1": photo(1, "album"), "2": photo(2, "library") };
    const merged = mergePhotos(fetched, current, (id) => id === 1);
    expect(merged["1"]!.outcome).toBe("album");
    expect(merged["2"]!.outcome).toBe("unreviewed");
  });

  it("drops photos the server no longer lists", () => {
    const merged = mergePhotos({ "1": photo(1) }, { "1": photo(1), "9": photo(9) }, () => false);
    expect(Object.keys(merged)).toEqual(["1"]);
  });
});

describe("MutationClock", () => {
  it("treats a GET that began before a change as stale for that photo only", () => {
    const clock = new MutationClock();
    const began = clock.begin();
    clock.touch(1);
    expect(clock.changedSince(1, began)).toBe(true);
    expect(clock.changedSince(2, began)).toBe(false);
  });

  it("stays stale when the POST settles after the GET began, then trusts a later GET", () => {
    const clock = new MutationClock();
    clock.touch(1); // pressed
    const during = clock.begin(); // GET issued while the POST is in flight
    clock.touch(1); // POST settled
    expect(clock.changedSince(1, during)).toBe(true);
    const after = clock.begin();
    expect(clock.changedSince(1, after)).toBe(false);
  });

  it("a stale GET that resolves after the POST does not flip the photo back", () => {
    const clock = new MutationClock();
    const began = clock.begin(); // GET starts, the server still says unreviewed
    clock.touch(1); // A pressed
    clock.touch(1); // POST settled, nothing in flight any more
    const fetched = { "1": photo(1, "unreviewed") };
    const onScreen = { "1": photo(1, "album") };
    const merged = mergePhotos(fetched, onScreen, (id) => clock.changedSince(id, began));
    expect(merged["1"]!.outcome).toBe("album");
  });
});

describe("acceptable", () => {
  const rev: Pick<ShootReview, "moments" | "highlights"> = {
    highlights: [],
    moments: [
      {
        idx: 0,
        start: null,
        end: null,
        bursts: [
          { idx: 0, photo_ids: [1] },
          { idx: 1, photo_ids: [2] },
          { idx: 2, photo_ids: [3] },
        ],
      },
      { idx: 1, start: null, end: null, bursts: [{ idx: 3, photo_ids: [4, 5] }] },
    ],
  };
  const map = Object.fromEntries(
    [
      photo(1, "unreviewed", proposing("album")),
      photo(2, "library", proposing("archive")),
      photo(3, "unreviewed"),
      photo(4, "unreviewed", proposing("delete", true)),
      photo(5, "unreviewed", proposing("archive")),
    ].map((p) => [String(p.id), p]),
  );
  const view = buildView(rev, map, null);

  it("skips decided photos and photos without a proposal, in navigation order", () => {
    expect(acceptable(view, map, "all")).toEqual([
      { photoId: 1, outcome: "album", blooper: false },
      { photoId: 4, outcome: "delete", blooper: true },
      { photoId: 5, outcome: "archive", blooper: false },
    ]);
  });

  it("keeps a blooper the user set on a photo the proposal does not call one", () => {
    const marked = { ...map, "5": { ...photo(5, "unreviewed", proposing("archive")), blooper: true } };
    const found = acceptable(buildView(rev, marked, null), marked, { photoId: 5 });
    expect(found).toEqual([{ photoId: 5, outcome: "archive", blooper: true }]);
  });

  it("limits itself to a moment or a photo", () => {
    expect(acceptable(view, map, { momentIdx: 1 }).map((a) => a.photoId)).toEqual([4, 5]);
    expect(acceptable(view, map, { photoId: 1 }).map((a) => a.photoId)).toEqual([1]);
    expect(acceptable(view, map, { photoId: 2 })).toEqual([]);
    expect(acceptable(view, map, { photoId: 3 })).toEqual([]);
  });
});

describe("your-call pairs", () => {
  const rev: Pick<ShootReview, "moments" | "highlights"> = {
    highlights: [],
    moments: [
      {
        idx: 0,
        start: null,
        end: null,
        bursts: [
          { idx: 0, photo_ids: [1] },
          { idx: 1, photo_ids: [2, 3] },
          { idx: 2, photo_ids: [4] },
        ],
      },
    ],
  };
  const map = Object.fromEntries(
    [1, 2, 3, 4].map((id) => [String(id), photo(id)]),
  );

  it("lifts the pair out of the bursts and puts it first in navigation order", () => {
    const view = buildView(rev, map, null, [{ id: 7, a: 4, b: 1, tradeoff: "t" }]);
    const moment = view.moments[0];
    expect(moment?.pairs).toEqual([{ id: 7, a: 4, b: 1, tradeoff: "t" }]);
    expect(moment?.bursts.map((b) => b.top)).toEqual([2]);
    expect(moment?.order).toEqual([4, 1, 2, 3]);
    expect(burstOf(view, 1)).toEqual([4, 1]);
  });

  it("ignores a pair with a photo the filter hides", () => {
    const view = buildView(rev, map, null, [{ id: 7, a: 4, b: 9, tradeoff: "t" }]);
    expect(view.moments[0]?.pairs).toEqual([]);
    expect(view.order).toEqual([1, 2, 4, 3]);
  });
});
