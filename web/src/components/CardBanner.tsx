import { useCallback, useRef, useState } from "react";
import { Link, useMatch } from "react-router";

import { listCards } from "../api/client";
import type { CardScanSummary, ServerEvent } from "../api/types";
import { useServerEvents } from "../api/ws";
import {
  mergeFetchedCards,
  withFailure,
  withoutFailure,
  type CardEventLog,
  type CardFailure,
} from "../lib/cards";
import { countDays } from "../lib/importDraft";

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Cards the server has announced and that are still inserted, kept live, plus cards it could not read. */
function useCards(): { cards: CardScanSummary[]; failures: CardFailure[] } {
  const [cards, setCards] = useState<CardScanSummary[]>([]);
  const [failures, setFailures] = useState<CardFailure[]>([]);
  const clock = useRef(0);
  const log = useRef<CardEventLog>({ added: new Map(), removed: new Map() });

  const refresh = useCallback(() => {
    const sentAt = clock.current;
    listCards().then(
      (fetched) =>
        setCards((current) => mergeFetchedCards(current, fetched, sentAt, log.current)),
      () => undefined,
    );
  }, []);

  useServerEvents((event: ServerEvent) => {
    if (event.type === "card_detected") {
      const card = event.scan;
      log.current.added.set(card.scan_id, ++clock.current);
      setCards((prev) => [...prev.filter((c) => c.scan_id !== card.scan_id), card]);
      setFailures((prev) => withoutFailure(prev, card.source));
    } else if (event.type === "card_removed") {
      log.current.removed.set(event.source, ++clock.current);
      setCards((prev) => prev.filter((c) => c.source !== event.source));
      setFailures((prev) => withoutFailure(prev, event.source));
    } else if (event.type === "card_scan_failed") {
      setFailures((prev) => withFailure(prev, { source: event.source, error: event.error }));
    }
    // Opening the socket refreshes too, so a card announced while a tab was
    // closed, or while the socket was down, still shows.
  }, refresh);

  return { cards, failures };
}

/** Offers to import each inserted card, on every page but that card's own import page. */
export function CardBanner() {
  const { cards, failures } = useCards();
  const open = useMatch("/import/:scanId")?.params.scanId;

  return (
    <>
      {failures.map((failure) => (
        <div
          className="card-banner card-banner-error"
          data-testid="card-banner-error"
          role="alert"
          key={failure.source}
        >
          <span>
            Could not read the card in {failure.source}: {failure.error}.{" "}
            <Link to="/">Import it from a folder instead</Link>
          </span>
        </div>
      ))}
      {cards
        .filter((card) => card.scan_id !== open)
        .map((card) => (
          <div className="card-banner" data-testid="card-banner" key={card.scan_id}>
            <span title={card.source}>
              SD card: {plural(card.photo_count, "photo", "photos")},{" "}
              {plural(countDays(card.events), "day", "days")}.{" "}
              <Link to={`/import/${card.scan_id}`}>Import?</Link>
              {card.already_imported > 0 && (
                <span className="muted"> {card.already_imported} already imported</span>
              )}
            </span>
          </div>
        ))}
    </>
  );
}
