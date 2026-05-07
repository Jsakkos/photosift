import type { ScanDateEntry } from "../types";

export const UNKNOWN_DAY = "unknown";

/// EXIF capture strings come in two formats; normalize the date portion
/// to "YYYY-MM-DD" so day buckets group correctly across both.
export function dayKey(captured: string | null): string {
  if (!captured) return UNKNOWN_DAY;
  const cleaned = captured.replace(/^(\d{4}):(\d{2}):(\d{2})/, "$1-$2-$3");
  const m = cleaned.match(/^(\d{4})-(\d{2})-(\d{2})/);
  if (!m) return UNKNOWN_DAY;
  return `${m[1]}-${m[2]}-${m[3]}`;
}

export function relativeDayLabel(key: string, today: Date): string {
  if (key === UNKNOWN_DAY) return "Unknown date";
  const [y, m, d] = key.split("-").map((s) => parseInt(s, 10));
  if (!y || !m || !d) return key;
  const dayDate = new Date(y, m - 1, d);
  const todayMid = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const dayMs = 24 * 60 * 60 * 1000;
  const diff = Math.round((todayMid.getTime() - dayDate.getTime()) / dayMs);
  if (diff === 0) {
    return `Today · ${dayDate.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
  }
  if (diff === 1) {
    return `Yesterday · ${dayDate.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
  }
  if (diff > 0 && diff < 7) {
    return dayDate.toLocaleDateString(undefined, {
      weekday: "long",
      month: "short",
      day: "numeric",
    });
  }
  if (dayDate.getFullYear() === todayMid.getFullYear()) {
    return dayDate.toLocaleDateString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
    });
  }
  return dayDate.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function slugify(s: string): string {
  return s
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function deriveSlug(
  driveLabel: string | null,
  driveLetter: string | null,
  newestDayKey: string | null,
  today: Date = new Date(),
): string {
  const labelPart = slugify(driveLabel || driveLetter || "card");
  if (newestDayKey && newestDayKey !== UNKNOWN_DAY) {
    return `${newestDayKey}_${labelPart}`;
  }
  const isoDay = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-${String(today.getDate()).padStart(2, "0")}`;
  return `${isoDay}_${labelPart}`;
}

export interface DayBucket {
  key: string;
  entries: ScanDateEntry[];
}

/// Group entries by EXIF day, sort photos chronologically inside each
/// day, then sort days newest-first with `unknown` always last.
export function groupByDay(entries: ScanDateEntry[]): DayBucket[] {
  const map = new Map<string, ScanDateEntry[]>();
  for (const e of entries) {
    const k = dayKey(e.capturedAt);
    const arr = map.get(k);
    if (arr) arr.push(e);
    else map.set(k, [e]);
  }
  const days: DayBucket[] = Array.from(map.entries()).map(([key, list]) => ({
    key,
    entries: [...list].sort((a, b) => {
      if (!a.capturedAt) return 1;
      if (!b.capturedAt) return -1;
      return a.capturedAt < b.capturedAt ? -1 : 1;
    }),
  }));
  days.sort((a, b) => {
    if (a.key === UNKNOWN_DAY) return 1;
    if (b.key === UNKNOWN_DAY) return -1;
    return b.key.localeCompare(a.key);
  });
  return days;
}

export function newestDayKey(buckets: DayBucket[]): string | null {
  for (const b of buckets) if (b.key !== UNKNOWN_DAY) return b.key;
  return null;
}
