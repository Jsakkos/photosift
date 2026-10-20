import { useCallback, useRef, useState } from "react";
import { Link, useMatch } from "react-router";

import { listCards } from "../api/client";
import type { CardScanSummary, ServerEvent } from "../api/types";
import { useServerEvents } from "../api/ws";
import { mergeFetchedCards, type CardEventLog } from "../lib/cards";
import { countDays } from "../lib/importDraft";

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Cards the server has announced and that are still inserted, kept live. */
function useCards(): CardScanSummary[] {
  const [cards, setCards] = useState<CardScanSummary[]>([]);
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
    } else if (event.type === "card_removed") {
      log.current.removed.set(event.source, ++clock.current);
      setCards((prev) => prev.filter((c) => c.source !== event.source));
    }
    // Opening the socket refreshes too, so a card announced while a tab was
    // closed, or while the socket was down, still shows.
  }, refresh);

  return cards;
}

/** Offers to import each inserted card, on every page but that card's own import page. */
export function CardBanner() {
  const cards = useCards();
  const open = useMatch("/import/:scanId")?.params.scanId;

  return (
    <>
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
