import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useNavigate } from "react-router";

import { getJob, listShoots, openInPlace, scan } from "../api/client";
import type { JobStateReply, ServerEvent, ShootSummary } from "../api/types";
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

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Follows the job indexing an opened folder until it names the shoot. */
function FollowIndexing({
  jobId,
  photos,
  onShoot,
  onError,
}: {
  jobId: string;
  photos: number;
  onShoot: (shootId: number) => void;
  onError: (message: string) => void;
}) {
  const [done, setDone] = useState(0);

  const apply = (reply: JobStateReply) => {
    if (reply.state === "running") {
      setDone(reply.done);
    } else if (reply.state === "failed") {
      onError(reply.error);
    } else if (reply.shoot_ids[0] === undefined) {
      onError("every photo in this folder is already in another shoot");
    } else {
      onShoot(reply.shoot_ids[0]);
    }
  };

  // Also on (re)connect: the job may have finished before the socket opened.
  const resync = () => {
    getJob(jobId).then(apply, (e: unknown) => onError(messageOf(e)));
  };

  useServerEvents((event: ServerEvent) => {
    if (event.type === "import_progress" && event.job_id === jobId) {
      apply({ state: "running", done: event.done, total: event.total });
    } else if (event.type === "import_done" && event.job_id === jobId) {
      apply({
        state: "done",
        shoot_ids: event.shoot_ids,
        copied: event.copied,
        skipped_duplicates: event.skipped_duplicates,
      });
    } else if (event.type === "import_failed" && event.job_id === jobId) {
      apply({ state: "failed", error: event.error });
    }
  }, resync);

  return (
    <p className="hint muted" role="status">
      Indexing {done} of {plural(photos, "photo", "photos")}...
    </p>
  );
}

type FolderAction = "scan" | "open";

/** Manual import for when no card was announced (or the watcher could not
 * read it), and opening a library folder in place, e.g. a shoot Lightroom
 * imported: culled where it is, nothing copied. */
function ImportFromFolder() {
  const navigate = useNavigate();
  const [source, setSource] = useState("");
  const [busy, setBusy] = useState<FolderAction | null>(null);
  const [indexing, setIndexing] = useState<{ jobId: string; photos: number } | null>(null);
  const [error, setError] = useState<{ action: FolderAction; message: string } | null>(null);

  const path = source.trim();
  const disabled = busy !== null || path === "";

  const fail = (action: FolderAction) => (message: string) => {
    setError({ action, message });
    setIndexing(null);
    setBusy(null);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (disabled) return;
    setBusy("scan");
    setError(null);
    scan(path).then(
      (result) => void navigate(`/import/${result.scan_id}`),
      (err: unknown) => fail("scan")(messageOf(err)),
    );
  };

  const openHere = () => {
    if (disabled) return;
    setBusy("open");
    setError(null);
    openInPlace(path).then(
      (reply) => {
        if (reply.status === "existing") void navigate(`/review/${reply.shoot_id}`);
        else setIndexing({ jobId: reply.job_id, photos: reply.photos });
      },
      (err: unknown) => fail("open")(messageOf(err)),
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
        disabled={busy !== null}
      />
      <button type="submit" disabled={disabled}>
        {busy === "scan" ? "Scanning..." : "Import from folder…"}
      </button>
      <button type="button" disabled={disabled} onClick={openHere}>
        {busy === "open" ? "Opening..." : "Open in place"}
      </button>
      <p className="hint muted">
        Open in place culls a folder already in your library, such as a shoot Lightroom
        imported, without copying it.
      </p>
      {indexing && (
        <FollowIndexing
          jobId={indexing.jobId}
          photos={indexing.photos}
          onShoot={(id) => void navigate(`/review/${id}`)}
          onError={fail("open")}
        />
      )}
      {error && (
        <p className="error hint" role="alert">
          {error.action === "scan" ? "Could not scan" : "Could not open"}: {error.message}
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
      <div className="page-head">
        <h1>Shoots</h1>
        <Link to="/taste" className="page-head-link">
          Taste
        </Link>
      </div>
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
