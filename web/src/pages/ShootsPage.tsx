import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";

import { scan, listShoots } from "../api/client";
import type { ServerEvent, ShootSummary } from "../api/types";
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

/** Manual import for when no card was announced (or the watcher could not read it). */
function ImportFromFolder() {
  const navigate = useNavigate();
  const [source, setSource] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const path = source.trim();
    if (!path || busy) return;
    setBusy(true);
    setError(null);
    scan(path).then(
      (result) => void navigate(`/import/${result.scan_id}`),
      (err: unknown) => {
        setError(err instanceof Error ? err.message : String(err));
        setBusy(false);
      },
    );
  };

  return (
    <form className="import-folder" onSubmit={submit}>
      <input
        type="text"
        aria-label="Folder or card to import"
        placeholder="Folder or card path, e.g. F:\ or D:\Pictures\Trip"
        value={source}
        onChange={(e) => setSource(e.target.value)}
        disabled={busy}
      />
      <button type="submit" disabled={busy || source.trim() === ""}>
        {busy ? "Scanning..." : "Import from folder…"}
      </button>
      {error && (
        <p className="error hint" role="alert">
          Could not scan: {error}
        </p>
      )}
    </form>
  );
}

export function ShootsPage() {
  const [shoots, setShoots] = useState<ShootSummary[] | null>(null);
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
  }, [refresh]);

  // Reopening the socket refreshes too: an import that finished while it was
  // down sent its event to nobody.
  useServerEvents((event: ServerEvent) => {
    if (event.type === "import_done" || event.type === "enrich_done") refresh();
  }, refresh);

  return (
    <section>
      <h1>Shoots</h1>
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
      <ImportFromFolder />
    </section>
  );
}
