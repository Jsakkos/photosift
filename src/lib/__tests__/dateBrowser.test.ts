import {
  dayKey,
  deriveSlug,
  groupByDay,
  newestDayKey,
  relativeDayLabel,
  slugify,
  UNKNOWN_DAY,
} from "../dateBrowser";
import type { ScanDateEntry } from "../../types";

function entry(path: string, capturedAt: string | null, alreadyImported = false): ScanDateEntry {
  return {
    path,
    filename: path.split(/[/\\]/).pop() ?? path,
    capturedAt,
    camera: "NIKON D750",
    fileSizeBytes: 1024,
    thumbDataUrl: null,
    alreadyImported,
  };
}

describe("dayKey", () => {
  test("normalizes EXIF colon-separated dates", () => {
    expect(dayKey("2026:03:05 14:23:56")).toBe("2026-03-05");
  });

  test("accepts dash-separated dates", () => {
    expect(dayKey("2026-03-05 14:23:56")).toBe("2026-03-05");
  });

  test("returns UNKNOWN_DAY for null", () => {
    expect(dayKey(null)).toBe(UNKNOWN_DAY);
  });

  test("returns UNKNOWN_DAY for unparsable input", () => {
    expect(dayKey("garbage")).toBe(UNKNOWN_DAY);
  });
});

describe("groupByDay", () => {
  test("buckets entries by EXIF day, newest first, unknown last", () => {
    const buckets = groupByDay([
      entry("a", "2026-03-04 09:00:00"),
      entry("b", "2026-03-05 14:23:56"),
      entry("c", null),
      entry("d", "2026-03-05 10:00:00"),
      entry("e", "2026-02-28 16:00:00"),
    ]);
    const keys = buckets.map((b) => b.key);
    expect(keys).toEqual(["2026-03-05", "2026-03-04", "2026-02-28", UNKNOWN_DAY]);
  });

  test("sorts entries within a day chronologically", () => {
    const buckets = groupByDay([
      entry("late", "2026-03-05 17:00:00"),
      entry("early", "2026-03-05 09:00:00"),
      entry("mid", "2026-03-05 13:00:00"),
    ]);
    expect(buckets[0].entries.map((e) => e.path)).toEqual(["early", "mid", "late"]);
  });
});

describe("newestDayKey", () => {
  test("skips the unknown bucket", () => {
    const buckets = [
      { key: UNKNOWN_DAY, entries: [] },
      { key: "2026-03-04", entries: [] },
    ];
    expect(newestDayKey(buckets)).toBe("2026-03-04");
  });

  test("returns null when only unknown exists", () => {
    expect(newestDayKey([{ key: UNKNOWN_DAY, entries: [] }])).toBe(null);
  });

  test("returns null on empty input", () => {
    expect(newestDayKey([])).toBe(null);
  });
});

describe("relativeDayLabel", () => {
  const today = new Date(2026, 2, 5);

  test("labels today with the Today prefix", () => {
    expect(relativeDayLabel("2026-03-05", today)).toMatch(/^Today /);
  });

  test("labels yesterday with the Yesterday prefix", () => {
    expect(relativeDayLabel("2026-03-04", today)).toMatch(/^Yesterday /);
  });

  test("renders Unknown date for the synthetic bucket", () => {
    expect(relativeDayLabel(UNKNOWN_DAY, today)).toBe("Unknown date");
  });
});

describe("slugify", () => {
  test("lowercases and collapses non-alphanumerics", () => {
    expect(slugify("NIKON D750")).toBe("nikon-d750");
  });

  test("trims trailing dashes", () => {
    expect(slugify("--Card 01--")).toBe("card-01");
  });
});

describe("deriveSlug", () => {
  test("combines newest day with drive label", () => {
    expect(deriveSlug("NIKON D750", "E", "2026-03-05")).toBe("2026-03-05_nikon-d750");
  });

  test("falls back to drive letter when no label", () => {
    expect(deriveSlug(null, "E", "2026-03-05")).toBe("2026-03-05_e");
  });

  test("uses today when no day key is known", () => {
    const today = new Date(2026, 2, 5);
    expect(deriveSlug("Card", null, null, today)).toBe("2026-03-05_card");
  });

  test("uses today for the unknown bucket", () => {
    const today = new Date(2026, 4, 1);
    expect(deriveSlug("Card", null, UNKNOWN_DAY, today)).toBe("2026-05-01_card");
  });
});
