import type { Counts, Outcome, ShootSummary } from "../api/types";
import type { EnrichProgress } from "./useReview";
import { FILTER_ORDER, OUTCOME_LABEL, plural } from "./model";

interface Props {
  shoot: ShootSummary;
  counts: Counts;
  momentCount: number;
  cameras: readonly string[];
  filter: Outcome | null;
  progress: EnrichProgress | null;
  onFilter: (outcome: Outcome) => void;
  onHelp: () => void;
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
  onFilter,
  onHelp,
}: Props) {
  const meta = [
    dates(shoot),
    plural(counts.total, "photo", "photos"),
    plural(momentCount, "moment", "moments"),
    cameras.join(" + "),
  ].filter(Boolean);
  return (
    <div className="summary">
      <div className="summary-title">
        <h1>{title(shoot)}</h1>
        <span className="muted">{meta.join(" · ")}</span>
      </div>
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
  );
}
