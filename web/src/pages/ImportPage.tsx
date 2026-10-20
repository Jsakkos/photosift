import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import {
  ApiError,
  currentJob,
  getJob,
  getScan,
  getSettings,
  listCards,
  listShoots,
  regroupEvents,
  scan as scanSource,
  scanThumbUrl,
  slugFor,
  startImport,
} from "../api/client";
import type {
  CardScan,
  EventKind,
  JobStateReply,
  ScanItem,
  ServerEvent,
  ShootSummary,
} from "../api/types";
import { useServerEvents } from "../api/ws";
import {
  countDays,
  draftsFromProposed,
  eventSpan,
  folderPreview,
  formatSpan,
  mergeWithNext,
  offsetsToSeconds,
  shiftTime,
  splitBefore,
  toEventSpecs,
  type DraftEvent,
} from "../lib/importDraft";

/** Thumbnails shown for a collapsed event. */
const PEEK = 8;
const SLUG_DEBOUNCE_MS = 150;
const REGROUP_DEBOUNCE_MS = 250;

function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function countKinds(items: ScanItem[], idxs: number[]): { photos: number; videos: number } {
  let videos = 0;
  for (const idx of idxs) if (items[idx]?.kind === "video") videos += 1;
  return { photos: idxs.length - videos, videos };
}

function describeCount(photos: number, videos: number): string {
  const base = plural(photos, "photo", "photos");
  return videos > 0 ? `${base}, ${plural(videos, "video", "videos")}` : base;
}

// --- page: load the scan -----------------------------------------------------

type Loaded =
  | { state: "loading" }
  | { state: "ready"; scan: CardScan }
  | { state: "gone" }
  | { state: "error"; message: string };

export function ImportPage() {
  const { scanId = "" } = useParams();
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });
  const [libraryRoot, setLibraryRoot] = useState<string | null>(null);

  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let live = true;
    setLoaded({ state: "loading" });
    getScan(scanId).then(
      (scan) => live && setLoaded({ state: "ready", scan }),
      (e: unknown) => {
        if (!live) return;
        setLoaded(
          e instanceof ApiError && e.status === 404
            ? { state: "gone" }
            : { state: "error", message: messageOf(e) },
        );
      },
    );
    return () => {
      live = false;
    };
  }, [scanId, attempt]);

  useEffect(() => {
    getSettings().then(
      (s) => setLibraryRoot(s.library_root),
      () => setLibraryRoot(""),
    );
  }, []);

  // A scan that failed to load because the server was away loads once it is back.
  useServerEvents(
    () => undefined,
    () => {
      if (loaded.state === "error") setAttempt((n) => n + 1);
    },
  );

  return (
    <section>
      <h1>Import</h1>
      {loaded.state === "loading" && <p className="muted">Loading...</p>}
      {loaded.state === "error" && (
        <p className="error" role="alert">
          Could not load the card scan: {loaded.message}
        </p>
      )}
      {loaded.state === "gone" && <ScanGone scanId={scanId} />}
      {loaded.state === "ready" && (
        <ImportEditor key={loaded.scan.scan_id} scan={loaded.scan} libraryRoot={libraryRoot} />
      )}
    </section>
  );
}

/** The server keeps only the latest few scans; an old tab's id may be gone. */
function ScanGone({ scanId }: { scanId: string }) {
  const navigate = useNavigate();
  const [source, setSource] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // An announced card remembers where it was mounted.
    listCards().then(
      (cards) => setSource(cards.find((c) => c.scan_id === scanId)?.source ?? null),
      () => undefined,
    );
  }, [scanId]);

  const rescan = () => {
    if (!source) return;
    setBusy(true);
    setError(null);
    scanSource(source).then(
      (fresh) => navigate(`/import/${fresh.scan_id}`, { replace: true }),
      (e: unknown) => {
        setError(messageOf(e));
        setBusy(false);
      },
    );
  };

  return (
    <div>
      <p>This card scan is no longer available.</p>
      {source ? (
        <p>
          <button type="button" onClick={rescan} disabled={busy}>
            Rescan card
          </button>{" "}
          <span className="muted">{source}</span>
        </p>
      ) : (
        <p className="muted">Insert the card again to scan it.</p>
      )}
      {error && (
        <p className="error" role="alert">
          Could not scan the card: {error}
        </p>
      )}
    </div>
  );
}

// --- import job state ----------------------------------------------------------

type JobState =
  | { status: "running"; done: number; total: number; current: string }
  | { status: "done"; shootIds: number[]; copied: number; skipped: number }
  | { status: "failed"; error: string };

/** A server reply folded into what the page knows: a finished job never goes back to running. */
function applyReply(
  prev: Record<string, JobState>,
  jobId: string,
  reply: JobStateReply,
  current?: string,
): Record<string, JobState> {
  const known = prev[jobId];
  if (reply.state === "running") {
    if (known && known.status !== "running") return prev;
    return {
      ...prev,
      [jobId]: {
        status: "running",
        done: reply.done,
        total: reply.total,
        current: current ?? (known?.status === "running" ? known.current : ""),
      },
    };
  }
  if (reply.state === "done") {
    return {
      ...prev,
      [jobId]: {
        status: "done",
        shootIds: reply.shoot_ids,
        copied: reply.copied,
        skipped: reply.skipped_duplicates,
      },
    };
  }
  return { ...prev, [jobId]: { status: "failed", error: reply.error } };
}

/**
 * Latest state of every import job the socket has mentioned, by job id.
 * `watching` is a job this page started and must follow even before the first
 * event. Events sent while the socket was down are gone, so each time it opens
 * the server is asked about every unfinished job, and which job (if any) is
 * running, which also covers an import started from another tab.
 */
function useJobs(watching: string | null): {
  jobs: Record<string, JobState>;
  resync: () => void;
} {
  const [jobs, setJobs] = useState<Record<string, JobState>>({});
  const latest = useRef({ jobs, watching });
  useEffect(() => {
    latest.current = { jobs, watching };
  });

  const resync = useCallback(() => {
    const { jobs: known, watching: mine } = latest.current;
    const unfinished = new Set(
      Object.entries(known)
        .filter(([, j]) => j.status === "running")
        .map(([id]) => id),
    );
    if (mine && known[mine]?.status !== "done" && known[mine]?.status !== "failed") {
      unfinished.add(mine);
    }
    for (const id of unfinished) {
      getJob(id).then(
        (reply) => setJobs((prev) => applyReply(prev, id, reply)),
        (e: unknown) => {
          // Forgotten by the server (a restart): its outcome is unknowable, so
          // stop waiting instead of leaving the page locked.
          if (e instanceof ApiError && e.status === 404) {
            setJobs((prev) => ({
              ...prev,
              [id]: {
                status: "failed",
                error: "The server no longer has a record of this import. Check the shoot list.",
              },
            }));
          }
        },
      );
    }
    currentJob().then(
      ({ job_id }) => {
        if (job_id) {
          setJobs((prev) =>
            prev[job_id] ? prev : applyReply(prev, job_id, { state: "running", done: 0, total: 0 }),
          );
        }
      },
      () => undefined,
    );
  }, []);

  useServerEvents((event: ServerEvent) => {
    switch (event.type) {
      case "import_progress":
        setJobs((prev) =>
          applyReply(
            prev,
            event.job_id,
            { state: "running", done: event.done, total: event.total },
            event.current,
          ),
        );
        break;
      case "import_done":
        setJobs((prev) =>
          applyReply(prev, event.job_id, {
            state: "done",
            shoot_ids: event.shoot_ids,
            copied: event.copied,
            skipped_duplicates: event.skipped_duplicates,
          }),
        );
        break;
      case "import_failed":
        setJobs((prev) =>
          applyReply(prev, event.job_id, { state: "failed", error: event.error }),
        );
        break;
      default:
        break;
    }
  }, resync);
  return { jobs, resync };
}

// --- editor ----------------------------------------------------------------------

function ImportEditor({
  scan,
  libraryRoot,
}: {
  scan: CardScan;
  libraryRoot: string | null;
}) {
  const [drafts, setDrafts] = useState<DraftEvent[]>(() =>
    draftsFromProposed(scan.events, []),
  );
  const [minutes, setMinutes] = useState<Record<string, string>>({});
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const { jobs, resync: resyncJobs } = useJobs(jobId);

  const offsetsSec = useMemo(() => offsetsToSeconds(minutes), [minutes]);
  const offsetsKey = JSON.stringify(offsetsSec);

  // Regroup with the server's rule whenever the clock offsets change. The key
  // of the offsets the current drafts were built from starts as "no offsets".
  const [appliedKey, setAppliedKey] = useState("{}");
  const [regroupError, setRegroupError] = useState<string | null>(null);
  // Until the regroup lands the events on screen belong to other offsets, so
  // importing now would pair the old grouping with the new clock.
  const regroupPending = offsetsKey !== appliedKey;
  useEffect(() => {
    if (offsetsKey === appliedKey) {
      setRegroupError(null);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      regroupEvents(scan.scan_id, offsetsSec).then(
        (proposed) => {
          if (!live) return;
          setAppliedKey(offsetsKey);
          setRegroupError(null);
          setDrafts((prev) => draftsFromProposed(proposed, prev));
        },
        (e: unknown) => live && setRegroupError(messageOf(e)),
      );
    }, REGROUP_DEBOUNCE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [offsetsKey, offsetsSec, appliedKey, scan.scan_id]);

  const job = jobId ? jobs[jobId] : undefined;
  const anyRunning = Object.values(jobs).some((j) => j.status === "running");
  const started = jobId !== null && job?.status !== "failed";
  const busy = starting || anyRunning || started;
  const cannotImport = busy || drafts.length === 0 || regroupPending || regroupError !== null;
  // Offsets and events are fixed once a job has been accepted.
  const locked = starting || started;

  const patch = (key: string, change: Partial<DraftEvent>) =>
    setDrafts((prev) => prev.map((d) => (d.key === key ? { ...d, ...change } : d)));

  const submit = () => {
    setStartError(null);
    setStarting(true);
    startImport({
      scan_id: scan.scan_id,
      events: toEventSpecs(drafts),
      camera_offsets: offsetsSec,
    }).then(
      (res) => {
        setJobId(res.job_id);
        setStarting(false);
      },
      (e: unknown) => {
        setStartError(messageOf(e));
        setStarting(false);
        // Another import holds the server: learn which, so the page shows it as busy.
        if (e instanceof ApiError && e.status === 409) resyncJobs();
      },
    );
  };

  const counts = countKinds(
    scan.items,
    scan.items.map((i) => i.idx),
  );
  const days = countDays(scan.events);

  return (
    <div className="import">
      <p className="import-summary">
        <span title={scan.source}>{scan.source}</span>
        <span className="muted">
          {" "}
          {describeCount(counts.photos, counts.videos)} on {plural(days, "day", "days")}
        </span>
        {scan.already_imported > 0 && (
          <span className="import-already"> {scan.already_imported} already imported</span>
        )}
      </p>
      {scan.warnings.length > 0 && (
        <ul className="warnings">
          {scan.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}

      <fieldset className="import-fields" disabled={locked}>
        <h2>Cameras</h2>
        <p className="muted">
          If a camera&apos;s clock was wrong, add minutes to it; events regroup as you type.
        </p>
        {scan.cameras.map((camera) => (
          <CameraRow
            key={camera.camera}
            camera={camera.camera}
            items={scan.items}
            first={camera.first}
            last={camera.last}
            value={minutes[camera.camera] ?? ""}
            offsetSec={offsetsSec[camera.camera] ?? 0}
            onChange={(v) => setMinutes((prev) => ({ ...prev, [camera.camera]: v }))}
          />
        ))}
        {regroupError && (
          <p className="error" role="alert">
            Could not regroup the events: {regroupError}
          </p>
        )}

        <h2>Events</h2>
        {drafts.map((draft, i) => (
          <EventCard
            key={draft.key}
            draft={draft}
            isLast={i === drafts.length - 1}
            scanId={scan.scan_id}
            items={scan.items}
            offsetsSec={offsetsSec}
            libraryRoot={libraryRoot}
            onPatch={(change) => patch(draft.key, change)}
            onMerge={() => setDrafts((prev) => mergeWithNext(prev, i))}
            onSplit={(position) => setDrafts((prev) => splitBefore(prev, i, position))}
          />
        ))}
      </fieldset>

      {startError && (
        <p className="error" role="alert">
          {startError}
        </p>
      )}
      <p>
        <button
          type="button"
          className="primary"
          onClick={submit}
          disabled={cannotImport}
        >
          Import {describeCount(counts.photos, counts.videos).replace(", ", " and ")}
        </button>
        {anyRunning && !started && (
          <span className="muted"> Another import is running.</span>
        )}
        {regroupPending && !anyRunning && (
          <span className="muted"> Regrouping events...</span>
        )}
      </p>
      {jobId && <JobStatus job={job} />}
    </div>
  );
}

// --- cameras --------------------------------------------------------------------

function CameraRow({
  camera,
  items,
  first,
  last,
  value,
  offsetSec,
  onChange,
}: {
  camera: string;
  items: ScanItem[];
  first: string;
  last: string;
  value: string;
  offsetSec: number;
  onChange: (value: string) => void;
}) {
  const own = items.filter((i) => i.camera === camera);
  const videos = own.filter((i) => i.kind === "video").length;
  const shift = (t: string) => shiftTime(t, offsetSec);
  return (
    <div className="camera" data-testid="camera">
      <span className="camera-name">{camera}</span>
      <span className="muted">{describeCount(own.length - videos, videos)}</span>
      <span className="muted">
        {formatSpan(first, last)}
        {offsetSec !== 0 && <> (now {formatSpan(shift(first), shift(last))})</>}
      </span>
      <label className="camera-offset">
        <span className="muted">Clock offset</span>
        <input
          type="number"
          step="any"
          inputMode="decimal"
          placeholder="0"
          aria-label={`Clock offset for ${camera} (minutes)`}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
        <span className="muted">min</span>
      </label>
    </div>
  );
}

// --- events ---------------------------------------------------------------------

/** What the server's slug rule makes of `name`, once it has answered. */
function useSlug(name: string, enabled: boolean): string {
  const [slug, setSlug] = useState<string | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    const timer = setTimeout(() => {
      slugFor(name).then(
        (s) => live && setSlug(s),
        () => undefined,
      );
    }, SLUG_DEBOUNCE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [name, enabled]);
  return slug ?? "...";
}

function EventCard({
  draft,
  isLast,
  scanId,
  items,
  offsetsSec,
  libraryRoot,
  onPatch,
  onMerge,
  onSplit,
}: {
  draft: DraftEvent;
  isLast: boolean;
  scanId: string;
  items: ScanItem[];
  offsetsSec: Record<string, number>;
  libraryRoot: string | null;
  onPatch: (change: Partial<DraftEvent>) => void;
  onMerge: () => void;
  onSplit: (position: number) => void;
}) {
  const [shown, setShown] = useState(PEEK);
  const slug = useSlug(draft.name, draft.kind === "named");
  const span = eventSpan(draft, items, offsetsSec);
  const { photos, videos } = countKinds(items, draft.itemIdxs);
  const total = draft.itemIdxs.length;
  const visible = draft.itemIdxs.slice(0, shown);
  const setKind = (kind: EventKind) => onPatch({ kind, kindTouched: true });

  return (
    <article className="event" data-testid="event">
      <header className="event-head">
        <strong>{formatSpan(span.start, span.end)}</strong>
        <span className="muted">{describeCount(photos, videos)}</span>
        <span className="kind-toggle" role="radiogroup" aria-label="Event kind">
          {(["named", "everyday"] as const).map((kind) => (
            <label key={kind}>
              <input
                type="radio"
                name={`kind-${draft.key}`}
                checked={draft.kind === kind}
                onChange={() => setKind(kind)}
              />
              {kind === "named" ? "Named" : "Everyday"}
            </label>
          ))}
        </span>
      </header>

      {draft.kind === "named" && (
        <input
          type="text"
          className="event-name"
          aria-label="Event name"
          placeholder="Name this event"
          autoComplete="off"
          value={draft.name}
          onChange={(e) => onPatch({ name: e.target.value })}
        />
      )}
      <p className="folder" data-testid="folder-path">
        {!libraryRoot ? (
          <span className="muted">...</span>
        ) : (
          <code>{folderPreview(libraryRoot, draft.kind, span.start, slug)}</code>
        )}
      </p>

      <div className="thumbs">
        {visible.map((idx, position) => {
          const item = items[idx];
          if (!item) return null;
          return (
            <div className="thumb-cell" key={idx}>
              <Thumb scanId={scanId} item={item} />
              {position > 0 && shown >= total && (
                <button
                  type="button"
                  className="split"
                  aria-label={`Split before photo ${position + 1}`}
                  onClick={() => onSplit(position)}
                >
                  Split
                </button>
              )}
            </div>
          );
        })}
      </div>

      <footer className="event-actions">
        {total > shown && (
          <button type="button" onClick={() => setShown(total)}>
            Show all {total}
          </button>
        )}
        {!isLast && (
          <button type="button" onClick={onMerge}>
            Merge with next
          </button>
        )}
      </footer>
    </article>
  );
}

function Thumb({ scanId, item }: { scanId: string; item: ScanItem }) {
  const [failed, setFailed] = useState(false);
  const name = basename(item.primary);
  if (item.kind === "video" || failed) {
    return (
      <span className="thumb thumb-missing" title={name}>
        {item.kind === "video" ? "Video" : "No preview"}
      </span>
    );
  }
  return (
    <span className="thumb" title={name}>
      <img
        src={scanThumbUrl(scanId, item.idx)}
        alt={name}
        loading="lazy"
        decoding="async"
        onError={() => setFailed(true)}
      />
    </span>
  );
}

// --- progress and result -----------------------------------------------------------

function JobStatus({ job }: { job: JobState | undefined }) {
  const running = job?.status === "running" ? job : undefined;
  const pct =
    job?.status === "done"
      ? 100
      : running && running.total > 0
        ? Math.floor((running.done / running.total) * 100)
        : 0;
  return (
    <div className="job">
      {job?.status !== "failed" && (
        <>
          <div
            className="progress"
            role="progressbar"
            aria-label="Import progress"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct}
          >
            <div className="progress-bar" style={{ width: `${pct}%` }} />
          </div>
          <p className="muted">
            {job?.status === "done"
              ? "Done."
              : running
                ? `${running.done} of ${running.total}: ${running.current}`
                : "Starting..."}
          </p>
        </>
      )}
      {job?.status === "failed" && (
        <p className="error" role="alert">
          Import failed: {job.error}
        </p>
      )}
      {job?.status === "done" && <ImportResult job={job} />}
    </div>
  );
}

function shootTitle(shoot: ShootSummary | undefined, id: number): string {
  if (!shoot) return `shoot ${id}`;
  if (shoot.name) return shoot.name;
  return shoot.kind === "everyday" ? `Everyday ${shoot.date_start?.slice(0, 7) ?? ""}`.trim() : `shoot ${id}`;
}

function ImportResult({ job }: { job: Extract<JobState, { status: "done" }> }) {
  const [shoots, setShoots] = useState<ShootSummary[]>([]);
  useEffect(() => {
    listShoots().then(setShoots, () => undefined);
  }, []);
  return (
    <div data-testid="import-result">
      <p>
        Copied {plural(job.copied, "photo", "photos")}; {job.skipped} already in the
        library.
      </p>
      <ul className="result-links">
        {job.shootIds.map((id) => (
          <li key={id}>
            <Link to={`/review/${id}`}>
              Review {shootTitle(shoots.find((s) => s.id === id), id)}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
