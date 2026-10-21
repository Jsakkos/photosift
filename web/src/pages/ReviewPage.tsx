import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";

import { getStatus, trashDeletes } from "../api/client";
import type { Outcome } from "../api/types";
import { Compare } from "../review/Compare";
import { AcceptDialog, DeleteDialog, HelpOverlay, Toasts } from "../review/Dialogs";
import { HelperPanel } from "../review/HelperPanel";
import { Highlights } from "../review/Highlights";
import { isTypingTarget, resolveKey, type Action } from "../review/keyboard";
import { Loupe } from "../review/Loupe";
import {
  acceptable,
  buildView,
  burstOf,
  momentOf,
  type Acceptance,
  countOutcomes,
  plural,
  stepMoment,
  stepPhoto,
} from "../review/model";
import { MomentSection } from "../review/MomentSection";
import { Prefetcher } from "../review/prefetch";
import { Summary } from "../review/Summary";
import { useReview, type Notice } from "../review/useReview";
import "../review/review.css";

type Mode = "grid" | "loupe" | "compare";

const NOTICE_MS = 7000;

function focusKey(shootId: number): string {
  return `photosift.review.focus.${shootId}`;
}

function readFocus(shootId: number): number | null {
  try {
    const value = Number(localStorage.getItem(focusKey(shootId)));
    return Number.isInteger(value) && value > 0 ? value : null;
  } catch {
    return null;
  }
}

function writeFocus(shootId: number, id: number): void {
  try {
    localStorage.setItem(focusKey(shootId), String(id));
  } catch {
    // Storage can be blocked; the screen works without remembering.
  }
}

export function ReviewPage() {
  const { shootId: param } = useParams();
  const shootId = Number(param);
  const navigate = useNavigate();

  const [notices, setNotices] = useState<{ id: number; notice: Notice }[]>([]);
  const noticeSeq = useRef(0);
  const notify = useCallback((notice: Notice) => {
    const id = ++noticeSeq.current;
    setNotices((list) => [...list, { id, notice }]);
    setTimeout(() => setNotices((list) => list.filter((n) => n.id !== id)), NOTICE_MS);
  }, []);

  const { review, photos, error, progress, setOutcome, reload, retryHelper, setNote } = useReview(
    shootId,
    notify,
  );
  const [filter, setFilter] = useState<Outcome | null>(null);
  const [mode, setMode] = useState<Mode>("grid");
  const [help, setHelp] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [confirm, setConfirm] = useState<"closed" | "open" | "busy">("closed");
  // Exactly the photos the dialog counts; what is sent is what was shown.
  const [approved, setApproved] = useState<number[]>([]);
  // The batch the accept-all dialog names; `done` is null until it is confirmed.
  const [accepting, setAccepting] = useState<{ items: Acceptance[]; done: number | null } | null>(
    null,
  );
  const [unavailableReason, setUnavailableReason] = useState<string | null>(null);
  const [focusId, setFocusId] = useState<number | null>(null);
  const prefetcher = useMemo(() => new Prefetcher(), []);
  const restored = useRef(false);
  // The default first focus must not scroll the page away from the highlights.
  const skipScroll = useRef(false);
  const lastIndex = useRef(0);
  const loupeShown = useRef<number | null>(null);

  const yourCalls = review?.helper.your_calls;
  const view = useMemo(
    () => (review ? buildView(review, photos, filter, yourCalls) : null),
    [review, photos, filter, yourCalls],
  );
  const fullView = useMemo(
    () => (review && filter !== null ? buildView(review, photos, null, yourCalls) : view),
    [review, photos, filter, yourCalls, view],
  );
  const openSuggestions = useMemo(
    () => (fullView ? acceptable(fullView, photos, "all").length : 0),
    [fullView, photos],
  );
  // Counted here, not taken from the server: it must follow the user's own decisions.
  const suggested = useMemo(() => {
    const counts = countOutcomes({});
    for (const photo of Object.values(photos)) {
      if (photo.proposal && photo.outcome === "unreviewed") counts[photo.proposal.outcome] += 1;
    }
    return counts;
  }, [photos]);
  const helperStatus = review?.helper.status;
  useEffect(() => {
    if (helperStatus !== "unavailable") return;
    let live = true;
    getStatus().then(
      (s) => live && setUnavailableReason(s.helper.reason),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [helperStatus]);
  const multiDay = useMemo(
    () => new Set(view?.moments.map((m) => m.start?.slice(0, 10))).size > 1,
    [view],
  );
  const counts = useMemo(() => countOutcomes(photos), [photos]);
  const cameras = useMemo(
    () => [...new Set(Object.values(photos).flatMap((p) => (p.camera ? [p.camera] : [])))].sort(),
    [photos],
  );

  useEffect(() => {
    restored.current = false;
    setFocusId(null);
    setMode("grid");
    setFilter(null);
    setAccepting(null);
    setChatOpen(false);
  }, [shootId]);

  // Pick the focus once the photos are known: the remembered one, else the
  // first. Afterwards, if the focused photo leaves the view (filtered out,
  // trashed), land on whatever now sits at its old position.
  useEffect(() => {
    if (!view || view.order.length === 0) return;
    if (!restored.current) {
      restored.current = true;
      const saved = readFocus(shootId);
      const known = saved !== null && view.order.includes(saved);
      skipScroll.current = !known;
      setFocusId(known ? saved : (view.order[0] ?? null));
      return;
    }
    if (focusId === null || !view.order.includes(focusId)) {
      setFocusId(view.order[Math.min(lastIndex.current, view.order.length - 1)] ?? null);
    }
  }, [view, focusId, shootId]);

  useEffect(() => {
    if (focusId === null || !view) return;
    lastIndex.current = Math.max(0, view.order.indexOf(focusId));
    writeFocus(shootId, focusId);
    if (mode === "loupe") prefetcher.around(view.order, lastIndex.current);
  }, [focusId, view, shootId, mode, prefetcher]);

  useEffect(() => () => prefetcher.clear(), [prefetcher]);

  useEffect(() => {
    if (mode !== "grid" || focusId === null) return;
    if (skipScroll.current) {
      skipScroll.current = false;
      return;
    }
    document
      .querySelector(`.tile[data-photo-id="${focusId}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [focusId, mode]);

  const openConfirm = () => {
    setApproved(
      Object.values(photos)
        .filter((p) => p.outcome === "delete")
        .map((p) => p.id),
    );
    setConfirm("open");
  };

  const confirmTrash = useCallback(async () => {
    setConfirm("busy");
    try {
      const report = await trashDeletes(shootId, approved);
      if (report.failed.length > 0) {
        notify({
          kind: "error",
          text: `${plural(report.failed.length, "photo", "photos")} could not be moved: ${report.failed
            .map((f) => f.error)
            .slice(0, 2)
            .join("; ")}`,
        });
      }
      if (report.trashed > 0) {
        notify({ kind: "info", text: `${plural(report.trashed, "photo", "photos")} moved to the Recycle Bin.` });
      }
    } catch (e) {
      notify({ kind: "error", text: `Could not move to the Recycle Bin: ${e instanceof Error ? e.message : String(e)}` });
    }
    setConfirm("closed");
    reload();
  }, [shootId, approved, notify, reload]);

  const openAccept = () => {
    if (!fullView) return;
    const items = acceptable(fullView, photos, "all");
    if (items.length > 0) setAccepting({ items, done: null });
  };

  const confirmAccept = async () => {
    if (!accepting) return;
    const { items } = accepting;
    for (const [i, item] of items.entries()) {
      setAccepting({ items, done: i });
      // A failed save has already said so in a toast; do not pile more on top.
      if (!(await setOutcome(item.photoId, item.outcome, item.blooper))) break;
    }
    setAccepting(null);
    reload();
  };

  // In the loupe the focus can be ahead of the picture (it waits for its
  // decode), and a rating must never land on a photo the user has not seen.
  const onScreen = (id: number) => mode !== "loupe" || loupeShown.current === id;

  const act = (action: Action, repeat: boolean) => {
    if (!view) return;
    if (help) {
      if (action.kind === "back" || action.kind === "help") setHelp(false);
      return;
    }
    if (accepting) {
      if (action.kind === "back" && accepting.done === null) setAccepting(null);
      return;
    }
    if (confirm !== "closed") {
      if (action.kind === "back" && confirm === "open") setConfirm("closed");
      return;
    }
    // The drawer is the top layer: Esc closes it before anything under it.
    if (action.kind === "back" && chatOpen) {
      setChatOpen(false);
      return;
    }
    const frames = mode === "compare" ? burstOf(view, focusId) : view.order;
    switch (action.kind) {
      case "photo":
        setFocusId(stepPhoto(frames, focusId, action.delta));
        break;
      case "moment":
        if (mode !== "compare") setFocusId(stepMoment(view, focusId, action.delta));
        break;
      case "outcome":
        if (focusId !== null && !repeat && onScreen(focusId)) {
          void setOutcome(focusId, action.outcome);
          // The cheap swap: picking one of a your-call pair as album sends the
          // other to archive, unless the user has already decided it.
          if (action.outcome === "album") {
            const pair = view.moments
              .flatMap((m) => m.pairs)
              .find((p) => p.a === focusId || p.b === focusId);
            const other = pair && (pair.a === focusId ? pair.b : pair.a);
            if (other !== undefined && photos[String(other)]?.outcome === "unreviewed") {
              void setOutcome(other, "archive");
            }
          }
        }
        break;
      case "accept": {
        if (repeat || focusId === null || !onScreen(focusId)) break;
        const moment = momentOf(view, focusId);
        const items =
          action.scope === "one"
            ? acceptable(view, photos, { photoId: focusId })
            : moment
              ? acceptable(view, photos, { momentIdx: moment.idx })
              : [];
        for (const item of items) void setOutcome(item.photoId, item.outcome, item.blooper);
        break;
      }
      case "blooper": {
        const photo = focusId === null ? undefined : photos[String(focusId)];
        if (photo && !repeat && onScreen(photo.id)) {
          void setOutcome(photo.id, photo.outcome, !photo.blooper);
        }
        break;
      }
      case "loupe":
        if (!repeat) setMode(mode === "loupe" ? "grid" : "loupe");
        break;
      case "compare":
        if (!repeat) setMode(mode === "compare" ? "grid" : "compare");
        break;
      case "filter":
        if (!repeat) setFilter((f) => (f === action.outcome ? null : action.outcome));
        break;
      case "helper":
        if (!repeat) setChatOpen((o) => !o);
        break;
      case "help":
        if (!repeat) setHelp(true);
        break;
      case "back":
        if (mode !== "grid") setMode("grid");
        else navigate("/");
        break;
    }
  };

  const onKeyDown = useRef((_e: KeyboardEvent) => {});
  useEffect(() => {
    onKeyDown.current = (e: KeyboardEvent) => {
      if (isTypingTarget(e.target, e.key)) return;
      const action = resolveKey(e);
      if (!action) return;
      e.preventDefault();
      act(action, e.repeat);
    };
  });
  useEffect(() => {
    const handler = (e: KeyboardEvent) => onKeyDown.current(e);
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const select = useCallback((id: number) => setFocusId(id), []);
  const open = useCallback((id: number) => {
    setFocusId(id);
    setMode("loupe");
  }, []);

  if (!Number.isInteger(shootId)) return <p className="muted">Page not found.</p>;
  if (error && !review) {
    return (
      <p className="error" role="alert">
        Could not load this shoot: {error}. <Link to="/">Back to shoots</Link>
      </p>
    );
  }
  if (!review || !view) return <p className="muted">Loading…</p>;

  const focused = focusId === null ? undefined : photos[String(focusId)];
  return (
    <div className={chatOpen ? "review with-helper" : "review"}>
      <Summary
        shoot={review.shoot}
        counts={counts}
        momentCount={review.moments.length}
        cameras={cameras}
        filter={filter}
        progress={progress}
        helper={{ ...review.helper, suggested }}
        unavailableReason={unavailableReason}
        openSuggestions={openSuggestions}
        onRetry={retryHelper}
        onAcceptAll={openAccept}
        onFilter={(o) => setFilter((f) => (f === o ? null : o))}
        onHelp={() => setHelp(true)}
        onChat={() => setChatOpen((o) => !o)}
        chatOpen={chatOpen}
      />
      {counts.total === 0 && <p className="muted">This shoot has no photos left.</p>}
      {counts.total > 0 && view.order.length === 0 && (
        <p className="muted empty-filter">Nothing here. Press the same number again to see everything.</p>
      )}
      <Highlights
        ids={view.highlights}
        photos={photos}
        focusId={focusId}
        onSelect={select}
        onOpen={open}
        onNote={setNote}
      />
      <div className="moments">
        {view.moments.map((moment) => (
          <MomentSection
            key={moment.idx}
            moment={moment}
            photos={photos}
            // Only the focused moment re-renders when focus moves.
            focusId={focusId !== null && moment.order.includes(focusId) ? focusId : null}
            withDay={multiDay}
            onSelect={select}
            onOpen={open}
            onNote={setNote}
          />
        ))}
      </div>
      <footer className="delete-bar">
        <button
          type="button"
          className="danger"
          disabled={counts.delete === 0}
          onClick={openConfirm}
        >
          Delete? ({counts.delete}) — Move to Recycle Bin
        </button>
        <span className="muted">
          Nothing is deleted until you confirm. Files go to the Recycle Bin, never straight off the disk.
        </span>
      </footer>
      {mode === "loupe" && focusId !== null && (
        <Loupe
          id={focusId}
          photo={focused}
          position={view.order.indexOf(focusId) + 1}
          total={view.order.length}
          prefetcher={prefetcher}
          shownRef={loupeShown}
        />
      )}
      {mode === "compare" && (
        <Compare ids={burstOf(view, focusId)} photos={photos} focusId={focusId} onSelect={select} />
      )}
      <HelperPanel shootId={shootId} open={chatOpen} onClose={() => setChatOpen(false)} />
      {help && <HelpOverlay onClose={() => setHelp(false)} />}
      {confirm !== "closed" && (
        <DeleteDialog
          count={approved.length}
          busy={confirm === "busy"}
          onCancel={() => setConfirm("closed")}
          onConfirm={() => void confirmTrash()}
        />
      )}
      {accepting && (
        <AcceptDialog
          items={accepting.items}
          done={accepting.done}
          onCancel={() => setAccepting(null)}
          onConfirm={() => void confirmAccept()}
        />
      )}
      <Toasts
        notices={notices}
        onDismiss={(id) => setNotices((list) => list.filter((n) => n.id !== id))}
      />
    </div>
  );
}
