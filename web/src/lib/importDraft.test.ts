import { describe, expect, it } from "vitest";

import type { ProposedEvent, ScanItem } from "../api/types";
import {
  correctedTime,
  countDays,
  draftsFromProposed,
  eventSpan,
  folderPreview,
  mergeWithNext,
  offsetsToSeconds,
  splitBefore,
  toEventSpecs,
  formatSpan,
} from "./importDraft";

function item(idx: number, time: string, camera = "CamA"): ScanItem {
  return {
    idx,
    primary: `C:\\card\\${idx}.JPG`,
    sibling_jpeg: null,
    kind: "photo",
    camera,
    capture_time: time,
    time_source: "exif",
    bytes: 1,
  };
}

function proposed(idxs: number[], kind: "named" | "everyday" = "named"): ProposedEvent {
  return {
    item_idxs: idxs,
    start: "2026-10-18T10:00:00",
    end: "2026-10-18T10:05:00",
    kind,
    suggested_name: "",
  };
}

describe("correctedTime", () => {
  it("adds the camera's offset in seconds and can cross midnight", () => {
    const it0 = item(0, "2026-10-18T23:30:00", "CamB");
    expect(correctedTime(it0, { CamB: 3600 })).toBe("2026-10-19T00:30:00");
    expect(correctedTime(it0, { CamB: -86400 })).toBe("2026-10-17T23:30:00");
    expect(correctedTime(it0, {})).toBe("2026-10-18T23:30:00");
  });
});

describe("offsetsToSeconds", () => {
  it("converts minutes typed by the user, ignoring blanks, junk and zeros", () => {
    expect(offsetsToSeconds({ A: "180", B: "", C: "x", D: "-1.5", E: "0" })).toEqual({
      A: 10800,
      D: -90,
    });
  });
});

describe("draftsFromProposed", () => {
  it("carries names over to the event with the most items in common", () => {
    const first = draftsFromProposed([proposed([0, 1, 2]), proposed([3, 4])], []);
    const named = first.map((d, i) => ({ ...d, name: i === 0 ? "Picnic" : "" }));

    const regrouped = draftsFromProposed([proposed([0, 1, 2, 3]), proposed([4])], named);

    expect(regrouped.map((d) => d.name)).toEqual(["Picnic", ""]);
  });

  it("keeps a kind the user chose, and otherwise takes the server's", () => {
    const first = draftsFromProposed([proposed([0, 1]), proposed([2, 3])], []);
    const edited = first.map((d, i) =>
      i === 0 ? { ...d, kind: "everyday" as const, kindTouched: true } : d,
    );

    const regrouped = draftsFromProposed(
      [proposed([0, 1], "named"), proposed([2, 3], "everyday")],
      edited,
    );

    expect(regrouped.map((d) => d.kind)).toEqual(["everyday", "everyday"]);
    expect(regrouped.map((d) => d.kindTouched)).toEqual([true, false]);
  });

  it("gives every draft its own key", () => {
    const drafts = draftsFromProposed([proposed([0]), proposed([1])], []);
    expect(new Set(drafts.map((d) => d.key)).size).toBe(2);
  });
});

describe("mergeWithNext and splitBefore", () => {
  const drafts = draftsFromProposed([proposed([0, 1, 2]), proposed([3, 4], "everyday")], []);

  it("merges two neighbours into a named event keeping the first name", () => {
    const named = drafts.map((d, i) => (i === 0 ? { ...d, name: "Picnic" } : d));
    const merged = mergeWithNext(named, 0);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.itemIdxs).toEqual([0, 1, 2, 3, 4]);
    expect(merged[0]?.name).toBe("Picnic");
    expect(merged[0]?.kind).toBe("named");
  });

  it("leaves the list alone when there is no next event", () => {
    expect(mergeWithNext(drafts, 1)).toBe(drafts);
  });

  it("splits before a photo, the second half inheriting the kind with no name", () => {
    const named = drafts.map((d, i) => (i === 0 ? { ...d, name: "Picnic" } : d));
    const split = splitBefore(named, 0, 2);
    expect(split.map((d) => d.itemIdxs)).toEqual([[0, 1], [2], [3, 4]]);
    expect(split.map((d) => d.name)).toEqual(["Picnic", "", ""]);
    expect(split[1]?.kind).toBe("named");
    expect(new Set(split.map((d) => d.key)).size).toBe(3);
  });

  it("ignores a split at either end", () => {
    expect(splitBefore(drafts, 0, 0)).toBe(drafts);
    expect(splitBefore(drafts, 0, 3)).toBe(drafts);
  });
});

describe("eventSpan", () => {
  it("is the corrected time of the first and last item", () => {
    const items = [
      item(0, "2026-10-18T07:00:00", "CamB"),
      item(1, "2026-10-18T10:20:00", "CamA"),
    ];
    const [draft] = draftsFromProposed([proposed([0, 1])], []);
    expect(eventSpan(draft!, items, { CamB: 10800 })).toEqual({
      start: "2026-10-18T10:00:00",
      end: "2026-10-18T10:20:00",
    });
  });
});

describe("folderPreview", () => {
  it("builds the named and everyday paths the server will create", () => {
    expect(folderPreview("E:\\photos\\DSLR", "named", "2026-10-18T10:00:00", "Babys-First-Meal")).toBe(
      "E:\\photos\\DSLR\\2026\\2026-10-18_Babys-First-Meal",
    );
    expect(folderPreview("E:\\photos\\DSLR\\", "everyday", "2026-10-18T10:00:00", "ignored")).toBe(
      "E:\\photos\\DSLR\\2026\\2026-10_Everyday",
    );
  });
});

describe("toEventSpecs", () => {
  it("sends names only for named events", () => {
    const drafts = draftsFromProposed([proposed([0]), proposed([1], "everyday")], []).map((d) => ({
      ...d,
      name: "Typed",
    }));
    expect(toEventSpecs(drafts)).toEqual([
      { item_idxs: [0], kind: "named", name: "Typed" },
      { item_idxs: [1], kind: "everyday" },
    ]);
  });
});

describe("countDays and formatSpan", () => {
  it("counts distinct calendar days an event list covers", () => {
    expect(
      countDays([
        { start: "2026-10-18T10:00:00", end: "2026-10-18T11:00:00" },
        { start: "2026-10-18T20:00:00", end: "2026-10-19T01:00:00" },
      ]),
    ).toBe(2);
    expect(countDays([])).toBe(0);
  });

  it("shows the date once when the span stays within a day", () => {
    expect(formatSpan("2026-10-18T10:00:00", "2026-10-18T10:29:00")).toBe("2026-10-18 10:00 to 10:29");
    expect(formatSpan("2026-10-18T23:00:00", "2026-10-19T01:00:00")).toBe(
      "2026-10-18 23:00 to 2026-10-19 01:00",
    );
  });
});
