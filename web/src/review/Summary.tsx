import type { Counts, HelperView, Outcome, ShootSummary } from "../api/types";
import type { EnrichProgress } from "./useReview";
import { FILTER_ORDER, OUTCOME_LABEL, plural } from "./model";

interface Props {
  shoot: ShootSummary;
  counts: Counts;
  momentCount: number;
  cameras: readonly string[];
  filter: Outcome | null;
  progress: EnrichProgress | null;
  helper: HelperView;
  /** Why the helper cannot run, from `/api/status`, while its state is `unavailable`. */
  unavailableReason: string | null;
  /** Suggestions still open on the whole shoot. */
  openSuggestions: number;
  onFilter: (outcome: Outcome) => void;
  onHelp: () => void;
  /** Opens or closes the helper chat drawer (key H). */
  onChat: () => void;
  chatOpen: boolean;
  onRetry: () => void;
  /** Stops the run in progress. */
  onStop: () => void;
  onAcceptAll: () => void;
}

/** "42 album · 60 library · 250 archive · 3 delete?", the non-zero parts only. */
export function suggestedLine(counts: Counts): string {
  return (["album", "library", "archive", "delete"] as const)
    .filter((o) => counts[o] > 0)
    .map((o) => `${counts[o]} ${OUTCOME_LABEL[o].toLowerCase()}`)
    .join(" · ");
}

function HelperLine({
  helper,
  unavailableReason,
  onRetry,
  onStop,
}: Pick<Props, "helper" | "unavailableReason" | "onRetry" | "onStop">) {
  const { status } = helper;
  if (status === "queued" || status === "running") {
    return (
      <span className="helper-status" data-helper={status}>
        <span className="muted" role="status">
          Helper is reading the shoot…
        </span>
        <button
          type="button"
          onClick={(e) => {
            onStop();
            e.currentTarget.blur();
          }}
        >
          Stop
        </button>
      </span>
    );
  }
  if (status === "failed" || status === "interrupted" || status === "unavailable") {
    const reason =
      status === "unavailable"
        ? (unavailableReason ?? helper.error ?? "The helper is not available.")
        : (helper.error ??
          (status === "interrupted" ? "The helper was interrupted." : "The helper failed."));
    return (
      <span className="helper-status helper-problem" data-helper={status}>
        <span role="alert">{reason}</span>
        <button
          type="button"
          disabled={status === "unavailable"}
          title={status === "unavailable" ? reason : undefined}
          onClick={(e) => {
            onRetry();
            e.currentTarget.blur();
          }}
        >
          Retry helper
        </button>
      </span>
    );
  }
  return null;
}

function title(shoot: ShootSummary): string {
  if (shoot.name) return shoot.name;
  return shoot.kind === "everyday" ? "Everyday" : "Untitled shoot";
}

function dates(shoot: ShootSummary): string {
  const { date_start: start, date_end: end } = shoot;
  if (!start) return "";
  return end && end !== start ? `${start} to ${end}` : start;
}

/** What this shoot is, and its outcome counts as filters (keys 1-5). */
export function Summary({
  shoot,
  counts,
  momentCount,
  cameras,
  filter,
  progress,
  helper,
  unavailableReason,
  openSuggestions,
  onFilter,
  onHelp,
  onChat,
  chatOpen,
  onRetry,
  onStop,
  onAcceptAll,
}: Props) {
  const suggested = suggestedLine(helper.suggested);
  const meta = [
    dates(shoot),
    plural(counts.total, "photo", "photos"),
    plural(momentCount, "moment", "moments"),
    cameras.join(" + "),
  ].filter(Boolean);
  return (
    <>
      <div className="summary-head">
        <div className="summary-title">
          <h1>{title(shoot)}</h1>
          <span className="muted">{meta.join(" · ")}</span>
        </div>
        {helper.summary && (
          <p className="helper-summary" data-testid="helper-summary">
            {helper.summary}
          </p>
        )}
        <div className="helper-row">
          <HelperLine
            helper={helper}
            unavailableReason={unavailableReason}
            onRetry={onRetry}
            onStop={onStop}
          />
          {suggested && (
            <span className="helper-suggested" data-testid="helper-suggested">
              Suggested: {suggested}
            </span>
          )}
          <button
            type="button"
            aria-pressed={chatOpen}
            title="Talk to the helper (H)"
            onClick={(e) => {
              onChat();
              e.currentTarget.blur();
            }}
          >
            Helper
          </button>
          {openSuggestions > 0 && (
            <button
              type="button"
              onClick={(e) => {
                onAcceptAll();
                e.currentTarget.blur();
              }}
            >
              Accept all suggestions ({openSuggestions})
            </button>
          )}
        </div>
      </div>
      <div className="summary">
        {progress && (
          <span className="muted summary-progress" role="status">
            Grouping photos… {progress.done} of {progress.total}
          </span>
        )}
        <div className="chips" role="group" aria-label="Show only">
          {FILTER_ORDER.map((outcome, i) => (
            <button
              key={outcome}
              type="button"
              className={`chip chip-${outcome}`}
              aria-pressed={filter === outcome}
              title={`Show only ${OUTCOME_LABEL[outcome]} (${i + 1})`}
              onClick={(e) => {
                onFilter(outcome);
                // Enter and Space belong to the review keys, not to this button.
                e.currentTarget.blur();
              }}
            >
              <span>{OUTCOME_LABEL[outcome]}</span> <b>{counts[outcome]}</b>
            </button>
          ))}
          <button
            type="button"
            className="help-button"
            aria-label="Keyboard shortcuts"
            onClick={(e) => {
              onHelp();
              e.currentTarget.blur();
            }}
          >
            ?
          </button>
        </div>
      </div>
    </>
  );
}
