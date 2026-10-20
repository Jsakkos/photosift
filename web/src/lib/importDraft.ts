// Client-side editing of the event list on the import screen. The server owns
// the split rule (it re-proposes events when camera offsets change); this
// module only does what the user does by hand afterwards: merge, split, name.

import type {
  EventKind,
  EventSpec,
  NaiveDateTime,
  ProposedEvent,
  ScanItem,
} from "../api/types";

export interface DraftEvent {
  /** Stable React key; survives edits to the event's contents. */
  key: string;
  /** `ScanItem::idx` values in offset-corrected capture order. */
  itemIdxs: number[];
  kind: EventKind;
  name: string;
  /** The user picked `kind` by hand, so a regroup must not overwrite it. */
  kindTouched: boolean;
}

let keyCounter = 0;
function nextKey(): string {
  keyCounter += 1;
  return `event-${keyCounter}`;
}

const NAIVE = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})/;

/** Naive camera-local times are treated as UTC so no timezone or DST creeps in. */
function parseNaive(value: NaiveDateTime): number {
  const m = NAIVE.exec(value);
  if (!m) return Number.NaN;
  const [, y, mo, d, h, mi, s] = m.map(Number);
  return Date.UTC(y ?? 0, (mo ?? 1) - 1, d ?? 1, h ?? 0, mi ?? 0, s ?? 0);
}

function formatNaive(ms: number): NaiveDateTime {
  return new Date(ms).toISOString().slice(0, 19);
}

export function shiftTime(time: NaiveDateTime, seconds: number): NaiveDateTime {
  return formatNaive(parseNaive(time) + seconds * 1000);
}

/** The item's capture time with its camera's clock offset (seconds) applied. */
export function correctedTime(
  item: ScanItem,
  offsetsSec: Record<string, number>,
): NaiveDateTime {
  return shiftTime(item.capture_time, offsetsSec[item.camera] ?? 0);
}

/** Minutes as typed (one string per camera) to the seconds the API takes; zero and unparsable entries drop out. */
export function offsetsToSeconds(
  minutes: Record<string, string>,
): Record<string, number> {
  const out: Record<string, number> = {};
  for (const [camera, text] of Object.entries(minutes)) {
    const value = Number(text.trim());
    if (text.trim() === "" || !Number.isFinite(value)) continue;
    const seconds = Math.round(value * 60);
    if (seconds !== 0) out[camera] = seconds;
  }
  return out;
}

function overlap(a: number[], b: number[]): number {
  const inB = new Set(b);
  return a.filter((i) => inB.has(i)).length;
}

/**
 * Turns the server's proposal into editable drafts. Names (and a kind the user
 * chose) carry over from the `previous` draft sharing the most items, so
 * nudging a clock offset does not wipe what was typed.
 */
export function draftsFromProposed(
  proposed: ProposedEvent[],
  previous: DraftEvent[],
): DraftEvent[] {
  return proposed.map((p) => {
    let best: DraftEvent | undefined;
    let bestOverlap = 0;
    for (const old of previous) {
      const n = overlap(p.item_idxs, old.itemIdxs);
      if (n > bestOverlap) {
        best = old;
        bestOverlap = n;
      }
    }
    const keepKind = best?.kindTouched === true;
    return {
      key: nextKey(),
      itemIdxs: p.item_idxs,
      kind: keepKind && best ? best.kind : p.kind,
      name: best?.name ?? p.suggested_name,
      kindTouched: keepKind,
    };
  });
}

/** Joins event `i` with the one after it. Named wins: a merged event is rarely "everyday". */
export function mergeWithNext(drafts: DraftEvent[], i: number): DraftEvent[] {
  const a = drafts[i];
  const b = drafts[i + 1];
  if (!a || !b) return drafts;
  const merged: DraftEvent = {
    key: a.key,
    itemIdxs: [...a.itemIdxs, ...b.itemIdxs],
    kind: a.kind === "named" || b.kind === "named" ? "named" : "everyday",
    name: a.name || b.name,
    kindTouched: a.kindTouched || b.kindTouched,
  };
  return [...drafts.slice(0, i), merged, ...drafts.slice(i + 2)];
}

/** Cuts event `i` so that its photo at `position` starts a new event. */
export function splitBefore(
  drafts: DraftEvent[],
  i: number,
  position: number,
): DraftEvent[] {
  const ev = drafts[i];
  if (!ev || position <= 0 || position >= ev.itemIdxs.length) return drafts;
  const head: DraftEvent = { ...ev, itemIdxs: ev.itemIdxs.slice(0, position) };
  const tail: DraftEvent = {
    key: nextKey(),
    itemIdxs: ev.itemIdxs.slice(position),
    kind: ev.kind,
    name: "",
    kindTouched: ev.kindTouched,
  };
  return [...drafts.slice(0, i), head, tail, ...drafts.slice(i + 1)];
}

/** Offset-corrected first and last time of an event. */
export function eventSpan(
  draft: DraftEvent,
  items: ScanItem[],
  offsetsSec: Record<string, number>,
): { start: NaiveDateTime; end: NaiveDateTime } {
  const times = draft.itemIdxs
    .map((idx) => items[idx])
    .filter((it): it is ScanItem => it !== undefined)
    .map((it) => correctedTime(it, offsetsSec))
    .sort();
  return { start: times[0] ?? "", end: times.at(-1) ?? "" };
}

/** The folder the server will create (`naming::folder_path`), shown before importing. */
export function folderPreview(
  libraryRoot: string,
  kind: EventKind,
  start: NaiveDateTime,
  slug: string,
): string {
  const root = libraryRoot.replace(/[\\/]+$/, "");
  const year = start.slice(0, 4);
  const leaf =
    kind === "named" ? `${start.slice(0, 10)}_${slug}` : `${start.slice(0, 7)}_Everyday`;
  return `${root}\\${year}\\${leaf}`;
}

export function toEventSpecs(drafts: DraftEvent[]): EventSpec[] {
  return drafts.map((d) =>
    d.kind === "named"
      ? { item_idxs: d.itemIdxs, kind: d.kind, name: d.name }
      : { item_idxs: d.itemIdxs, kind: d.kind },
  );
}

/** Distinct calendar days covered by a list of `{start, end}` spans. */
export function countDays(spans: { start: NaiveDateTime; end: NaiveDateTime }[]): number {
  const days = new Set<string>();
  const DAY = 86_400_000;
  for (const { start, end } of spans) {
    const from = parseNaive(start.slice(0, 10) + "T00:00:00");
    const to = parseNaive(end.slice(0, 10) + "T00:00:00");
    if (Number.isNaN(from) || Number.isNaN(to)) continue;
    for (let t = from; t <= to; t += DAY) days.add(formatNaive(t).slice(0, 10));
  }
  return days.size;
}

export function formatSpan(start: NaiveDateTime, end: NaiveDateTime): string {
  const s = `${start.slice(0, 10)} ${start.slice(11, 16)}`;
  if (start.slice(0, 10) === end.slice(0, 10)) return `${s} to ${end.slice(11, 16)}`;
  return `${s} to ${end.slice(0, 10)} ${end.slice(11, 16)}`;
}
