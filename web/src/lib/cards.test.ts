import { describe, expect, it } from "vitest";

import type { CardScanSummary } from "../api/types";
import { mergeFetchedCards, type CardEventLog } from "./cards";

function card(scanId: string, source: string): CardScanSummary {
  return {
    scan_id: scanId,
    source,
    photo_count: 1,
    video_count: 0,
    already_imported: 0,
    cameras: [],
    events: [],
  };
}

function log(added: [string, number][] = [], removed: [string, number][] = []): CardEventLog {
  return { added: new Map(added), removed: new Map(removed) };
}

describe("mergeFetchedCards", () => {
  it("takes the fetched list when no events happened since", () => {
    const stale = card("old", "E:\\");
    const fresh = card("new", "F:\\");

    expect(mergeFetchedCards([stale], [fresh], 5, log())).toEqual([fresh]);
  });

  it("keeps a card announced by an event after the request was sent", () => {
    const late = card("late", "G:\\");
    const fetched = card("a", "E:\\");

    const merged = mergeFetchedCards([late], [fetched], 3, log([["late", 4]]));

    expect(merged.map((c) => c.scan_id).sort()).toEqual(["a", "late"]);
  });

  it("drops a card announced before the request if the server no longer lists it", () => {
    const gone = card("gone", "E:\\");

    expect(mergeFetchedCards([gone], [], 3, log([["gone", 2]]))).toEqual([]);
  });

  it("does not resurrect a card removed after the request was sent", () => {
    const removed = card("a", "E:\\");

    expect(mergeFetchedCards([], [removed], 3, log([], [["E:\\", 4]]))).toEqual([]);
  });

  it("lets the fetched copy of a card replace the one held", () => {
    const held = { ...card("a", "E:\\"), already_imported: 0 };
    const fetched = { ...card("a", "E:\\"), already_imported: 35 };

    expect(mergeFetchedCards([held], [fetched], 9, log([["a", 4]]))).toEqual([fetched]);
  });
});
