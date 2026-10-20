import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router";

import { listCards, listShoots } from "../api/client";
import type { CardScanSummary, ServerEvent, ShootSummary } from "../api/types";
import { useServerEvents } from "../api/ws";

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

function shootTitle(shoot: ShootSummary): string {
  if (shoot.name) return shoot.name;
  return shoot.kind === "everyday" ? "Everyday" : "Untitled shoot";
}

function shootDates(shoot: ShootSummary): string {
  const { date_start: start, date_end: end } = shoot;
  if (!start) return "";
  return end && end !== start ? `${start} to ${end}` : start;
}

export function ShootsPage() {
  const [shoots, setShoots] = useState<ShootSummary[] | null>(null);
  const [cards, setCards] = useState<CardScanSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  // Only the newest response may land: a slow earlier fetch must not overwrite it.
  const latest = useRef(0);

  const refresh = useCallback(() => {
    const mine = ++latest.current;
    listShoots().then(
      (list) => {
        if (mine !== latest.current) return;
        setShoots(list);
        setError(null);
      },
      (e: unknown) => {
        if (mine !== latest.current) return;
        setError(e instanceof Error ? e.message : String(e));
      },
    );
  }, []);

  useEffect(() => {
    refresh();
    // A tab opened after the card was announced still shows its banner.
    listCards().then(setCards, () => undefined);
  }, [refresh]);

  useServerEvents((event: ServerEvent) => {
    switch (event.type) {
      case "import_done":
      case "enrich_done":
        refresh();
        break;
      case "card_detected":
        setCards((prev) => [
          ...prev.filter((c) => c.scan_id !== event.scan.scan_id),
          event.scan,
        ]);
        break;
      case "card_removed":
        setCards((prev) => prev.filter((c) => c.source !== event.source));
        break;
      default:
        break;
    }
  });

  return (
    <section>
      <h1>Shoots</h1>
      {cards.map((card) => (
        <div className="card-banner" key={card.scan_id}>
          <span>
            Card {card.source}: {plural(card.photo_count, "photo", "photos")}
            {card.already_imported > 0 && `, ${card.already_imported} already imported`}
          </span>
          <Link to={`/import/${card.scan_id}`}>Import</Link>
        </div>
      ))}
      {error && (
        <p className="error" role="alert">
          Could not load shoots: {error}
        </p>
      )}
      {shoots === null && !error && <p className="muted">Loading...</p>}
      {shoots !== null && shoots.length === 0 && (
        <p className="muted">No shoots yet</p>
      )}
      {shoots !== null && shoots.length > 0 && (
        <ul className="shoot-list">
          {shoots.map((shoot) => (
            <li key={shoot.id}>
              <Link to={`/review/${shoot.id}`} className="shoot">
                <span className="shoot-title">{shootTitle(shoot)}</span>
                <span className="shoot-dates">{shootDates(shoot)}</span>
                <span className="shoot-count">
                  {plural(shoot.counts.total, "photo", "photos")}
                </span>
                {shoot.counts.unreviewed > 0 && (
                  <span className="shoot-unreviewed">
                    {shoot.counts.unreviewed} to review
                  </span>
                )}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
