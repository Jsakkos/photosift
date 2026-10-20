import type { CardScanSummary } from "../api/types";

/**
 * When live events touched the card list, as values of a counter that also
 * stamps each fetch. A fetch answers for the moment it was sent, so anything
 * the socket reported after that is newer than the response and must win.
 */
export interface CardEventLog {
  /** scan_id of a card announced by an event, with the counter value then. */
  added: Map<string, number>;
  /** source of a card reported removed by an event, with the counter value then. */
  removed: Map<string, number>;
}

/**
 * The card list after a fetch that was sent at counter value `sentAt`: the
 * fetched list (keyed by scan_id), plus cards announced by events since, minus
 * cards removed since. Unlike replacing state wholesale, a card that appeared
 * while the request was in flight is not lost.
 */
export function mergeFetchedCards(
  current: CardScanSummary[],
  fetched: CardScanSummary[],
  sentAt: number,
  log: CardEventLog,
): CardScanSummary[] {
  const byId = new Map(fetched.map((c) => [c.scan_id, c]));
  for (const card of current) {
    const announcedAt = log.added.get(card.scan_id) ?? 0;
    if (announcedAt > sentAt && !byId.has(card.scan_id)) byId.set(card.scan_id, card);
  }
  return [...byId.values()].filter((c) => (log.removed.get(c.source) ?? 0) <= sentAt);
}

/** A card the server saw but could not scan. */
export interface CardFailure {
  source: string;
  error: string;
}

/** `failures` with this card's entry added, replacing an earlier one for the same source. */
export function withFailure(failures: CardFailure[], failure: CardFailure): CardFailure[] {
  return [...failures.filter((f) => f.source !== failure.source), failure];
}

/** `failures` without the card at `source`, once it is pulled or scanned fine after all. */
export function withoutFailure(failures: CardFailure[], source: string): CardFailure[] {
  return failures.filter((f) => f.source !== source);
}
