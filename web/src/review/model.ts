import type { Counts, Outcome, ReviewPhoto, ShootReview, YourCall } from "../api/types";

/** Chip order: `1`-`5` pick these. "Your call" is the unreviewed pile. */
export const FILTER_ORDER: readonly Outcome[] = [
  "album",
  "library",
  "unreviewed",
  "archive",
  "delete",
];

export const OUTCOME_LABEL: Record<Outcome, string> = {
  album: "Album",
  library: "Library",
  unreviewed: "Your call",
  archive: "Archive",
  delete: "Delete?",
};

export type PhotoMap = Record<string, ReviewPhoto>;

export interface ViewBurst {
  idx: number;
  /** The frame shown large. */
  top: number;
  /** The burst's other frames, shown in the dim strip. */
  rest: number[];
}

/** Two frames the helper could not choose between, shown together above the bursts. */
export interface ViewPair {
  id: number;
  a: number;
  b: number;
  tradeoff: string;
}

export interface ViewMoment {
  idx: number;
  pairs: ViewPair[];
  start: string | null;
  end: string | null;
  bursts: ViewBurst[];
  /** Navigation order inside the moment: the your-call pairs, every large frame, then the strips. */
  order: number[];
  /** Over all the moment's photos, whatever the filter. */
  shots: number;
  byOutcome: Record<Outcome, number>;
  bloopers: number;
}

export interface ReviewView {
  moments: ViewMoment[];
  /** Every visible photo in navigation order. */
  order: number[];
  highlights: number[];
}

export function emptyOutcomeCounts(): Record<Outcome, number> {
  return { unreviewed: 0, album: 0, library: 0, archive: 0, delete: 0 };
}

export function countOutcomes(photos: PhotoMap): Counts {
  const counts: Counts = {
    total: 0,
    unreviewed: 0,
    album: 0,
    library: 0,
    archive: 0,
    delete: 0,
  };
  for (const photo of Object.values(photos)) {
    counts.total += 1;
    counts[photo.outcome] += 1;
  }
  return counts;
}

/** Width over height as displayed; a photo with unknown size is assumed 3:2. */
export function aspectOf(photo: ReviewPhoto | undefined): number {
  if (!photo || !photo.w || !photo.h) return 1.5;
  return photo.w / photo.h;
}

export function buildView(
  review: Pick<ShootReview, "moments" | "highlights">,
  photos: PhotoMap,
  filter: Outcome | null,
  yourCalls: readonly YourCall[] = [],
): ReviewView {
  const visible = (id: number): boolean => {
    const photo = photos[String(id)];
    return photo !== undefined && (filter === null || photo.outcome === filter);
  };
  const moments: ViewMoment[] = [];
  const paired = new Set<number>();
  for (const moment of review.moments) {
    const inMoment = new Set(moment.bursts.flatMap((b) => b.photo_ids));
    const pairs: ViewPair[] = [];
    for (const call of yourCalls) {
      const usable =
        inMoment.has(call.a) &&
        inMoment.has(call.b) &&
        visible(call.a) &&
        visible(call.b) &&
        !paired.has(call.a) &&
        !paired.has(call.b);
      if (!usable) continue;
      paired.add(call.a);
      paired.add(call.b);
      pairs.push({ id: call.id, a: call.a, b: call.b, tradeoff: call.tradeoff });
    }
    const byOutcome = emptyOutcomeCounts();
    let shots = 0;
    let bloopers = 0;
    const bursts: ViewBurst[] = [];
    for (const burst of moment.bursts) {
      for (const id of burst.photo_ids) {
        const photo = photos[String(id)];
        if (!photo) continue;
        shots += 1;
        byOutcome[photo.outcome] += 1;
        if (photo.blooper) bloopers += 1;
      }
      const shown = burst.photo_ids.filter((id) => visible(id) && !paired.has(id));
      const [top, ...rest] = shown;
      if (top !== undefined) bursts.push({ idx: burst.idx, top, rest });
    }
    if (bursts.length === 0 && pairs.length === 0) continue;
    moments.push({
      idx: moment.idx,
      pairs,
      start: moment.start,
      end: moment.end,
      bursts,
      order: [
        ...pairs.flatMap((p) => [p.a, p.b]),
        ...bursts.map((b) => b.top),
        ...bursts.flatMap((b) => b.rest),
      ],
      shots,
      byOutcome,
      bloopers,
    });
  }
  return {
    moments,
    order: moments.flatMap((m) => m.order),
    highlights: review.highlights.filter(visible),
  };
}

/** The photo `delta` steps from `id` in `order`, clamped; the first when `id` is not in it. */
export function stepPhoto(
  order: readonly number[],
  id: number | null,
  delta: number,
): number | null {
  if (order.length === 0) return null;
  const at = id === null ? -1 : order.indexOf(id);
  if (at < 0) return order[0] ?? null;
  return order[Math.min(order.length - 1, Math.max(0, at + delta))] ?? null;
}

/** The first photo of the moment before/after the one holding `id`. */
export function stepMoment(view: ReviewView, id: number | null, delta: -1 | 1): number | null {
  if (view.moments.length === 0) return null;
  const at = view.moments.findIndex((m) => id !== null && m.order.includes(id));
  if (at < 0) return view.moments[0]?.order[0] ?? null;
  const target = Math.min(view.moments.length - 1, Math.max(0, at + delta));
  return view.moments[target]?.order[0] ?? null;
}

/** All visible frames of the burst containing `id`, large frame first. */
export function burstOf(view: ReviewView, id: number | null): number[] {
  if (id === null) return [];
  for (const moment of view.moments) {
    for (const pair of moment.pairs) {
      if (pair.a === id || pair.b === id) return [pair.a, pair.b];
    }
    for (const burst of moment.bursts) {
      const frames = [burst.top, ...burst.rest];
      if (frames.includes(id)) return frames;
    }
  }
  return [];
}

export type AcceptScope = { photoId: number } | { momentIdx: number } | "all";

export interface Acceptance {
  photoId: number;
  outcome: Exclude<Outcome, "unreviewed">;
  blooper: boolean;
}

/**
 * The helper's suggestions that are still open: unreviewed photos with a
 * proposal, in navigation order. A decided photo is never touched.
 */
export function acceptable(
  view: ReviewView,
  photos: PhotoMap,
  scope: AcceptScope,
): Acceptance[] {
  const ids =
    scope === "all"
      ? view.order
      : "photoId" in scope
        ? [scope.photoId]
        : (view.moments.find((m) => m.idx === scope.momentIdx)?.order ?? []);
  const out: Acceptance[] = [];
  for (const id of ids) {
    const photo = photos[String(id)];
    const proposal = photo?.proposal;
    if (photo && proposal && photo.outcome === "unreviewed") {
      out.push({ photoId: id, outcome: proposal.outcome, blooper: proposal.blooper });
    }
  }
  return out;
}

/** The moment holding `id`, over the visible photos. */
export function momentOf(view: ReviewView, id: number | null): ViewMoment | undefined {
  return id === null ? undefined : view.moments.find((m) => m.order.includes(id));
}

function clock(time: string | null): string | null {
  const m = time ? /T(\d{2}):(\d{2})/.exec(time) : null;
  if (!m) return null;
  const h = Number(m[1]);
  const suffix = h >= 12 ? "pm" : "am";
  return `${h % 12 === 0 ? 12 : h % 12}:${m[2]} ${suffix}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "2026-10-18T15:48:00" -> "Oct 18", read from the text so no timezone can shift it. */
export function dayLabel(time: string | null): string | null {
  const m = time ? /^(\d{4})-(\d{2})-(\d{2})/.exec(time) : null;
  const month = m ? MONTHS[Number(m[2]) - 1] : undefined;
  return m && month ? `${month} ${Number(m[3])}` : null;
}

/** "3:48 pm", or "Oct 18 · 3:48 pm" when the shoot spans several days. */
export function momentTitle(start: string | null, withDay = false): string {
  const time = clock(start);
  if (!time) return "Undated";
  const day = withDay ? dayLabel(start) : null;
  return day ? `${day} · ${time}` : time;
}

export function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/**
 * A freshly fetched photo map, except that photos for which `keepLocal` says so
 * keep the value on screen: the answer predates a change the user made.
 */
export function mergePhotos(
  fetched: PhotoMap,
  current: PhotoMap,
  keepLocal: (photoId: number) => boolean,
): PhotoMap {
  const merged: PhotoMap = {};
  for (const [key, photo] of Object.entries(fetched)) {
    const mine = current[key];
    merged[key] =
      mine && keepLocal(photo.id)
        ? { ...photo, outcome: mine.outcome, blooper: mine.blooper }
        : photo;
  }
  return merged;
}

/**
 * Orders local changes against fetches. A GET that started before a change (or
 * before that change's POST settled) may carry the old value even if it
 * resolves later, so its answer must not overwrite the photo.
 */
export class MutationClock {
  private now = 0;
  private touched = new Map<number, number>();

  /** Call when a fetch starts; pass the result to `changedSince` when it resolves. */
  begin(): number {
    return this.now;
  }

  /** Call when a photo changes locally and again when its POST settles. */
  touch(photoId: number): void {
    this.now += 1;
    this.touched.set(photoId, this.now);
  }

  changedSince(photoId: number, began: number): boolean {
    return (this.touched.get(photoId) ?? 0) > began;
  }

  reset(): void {
    this.touched.clear();
  }
}
