import { useEffect, useRef } from "react";

import { BINDINGS } from "./keyboard";
import { OUTCOME_LABEL, plural, type Acceptance } from "./model";
import type { Notice } from "./useReview";

export function HelpOverlay({ onClose }: { onClose: () => void }) {
  return (
    <div className="overlay" onClick={onClose}>
      <div
        className="dialog help"
        role="dialog"
        aria-label="Keyboard shortcuts"
        onClick={(e) => e.stopPropagation()}
      >
        <h2>Keyboard shortcuts</h2>
        <table>
          <tbody>
            {BINDINGS.map((b) => (
              <tr key={b.label}>
                <th scope="row">
                  <kbd>{b.label}</kbd>
                </th>
                <td>{b.description}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </div>
    </div>
  );
}

interface DeleteProps {
  count: number;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

/** The explicit batch approval: nothing leaves the library before this is confirmed. */
export function DeleteDialog({ count, busy, onCancel, onConfirm }: DeleteProps) {
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => cancel.current?.focus(), []);
  const photos = plural(count, "photo", "photos");
  return (
    <div className="overlay">
      <div className="dialog" role="dialog" aria-modal="true" aria-label="Move to the Recycle Bin?">
        <h2>Move {photos} to the Recycle Bin?</h2>
        <p>
          These {count === 1 ? "is the 1 photo" : `are the ${count} photos`} you marked Delete?.
          Their files go to the Windows Recycle Bin, so you can still restore them from there.
        </p>
        <p className="muted">
          Already imported this shoot into Lightroom? Use Lightroom&apos;s Delete Rejected Photos
          instead, so the catalog stays in sync.
        </p>
        <div className="dialog-actions">
          <button type="button" ref={cancel} onClick={onCancel} disabled={busy}>
            Keep them
          </button>
          <button type="button" className="danger" onClick={onConfirm} disabled={busy}>
            {busy ? "Moving…" : `Send ${photos} to the Recycle Bin`}
          </button>
        </div>
      </div>
    </div>
  );
}

export function Toasts({
  notices,
  onDismiss,
}: {
  notices: { id: number; notice: Notice }[];
  onDismiss: (id: number) => void;
}) {
  return (
    <div className="toasts" aria-live="polite">
      {notices.map(({ id, notice }) => (
        <div
          key={id}
          className={`toast toast-${notice.kind}`}
          role={notice.kind === "error" ? "alert" : "status"}
        >
          <span>{notice.text}</span>
          <button type="button" onClick={() => onDismiss(id)} aria-label="Dismiss">
            ×
          </button>
        </div>
      ))}
    </div>
  );
}

interface AcceptProps {
  items: readonly Acceptance[];
  /** How many are saved so far, while the batch runs. */
  done: number | null;
  onCancel: () => void;
  onConfirm: () => void;
}

/** Applies the helper's open suggestions one at a time; each is still a normal, undoable outcome. */
export function AcceptDialog({ items, done, onCancel, onConfirm }: AcceptProps) {
  const accept = useRef<HTMLButtonElement>(null);
  useEffect(() => accept.current?.focus(), []);
  const busy = done !== null;
  const parts = (["album", "library", "archive", "delete"] as const)
    .map((o) => [o, items.filter((i) => i.outcome === o).length] as const)
    .filter(([, n]) => n > 0)
    .map(([o, n]) => `${n} ${OUTCOME_LABEL[o]}`);
  return (
    <div className="overlay">
      <div className="dialog" role="dialog" aria-modal="true" aria-label="Accept all suggestions?">
        <h2>Accept {plural(items.length, "suggestion", "suggestions")}?</h2>
        <p data-testid="accept-counts">{parts.join(" · ")}</p>
        <p className="muted">
          Photos you have already decided stay as they are. Delete? only marks a photo; nothing
          leaves the disk until you approve it at the bottom of the page.
        </p>
        <div className="dialog-actions">
          <button type="button" onClick={onCancel} disabled={busy}>
            Not now
          </button>
          <button type="button" ref={accept} onClick={onConfirm} disabled={busy}>
            {busy ? `Saving ${done} of ${items.length}…` : `Accept ${items.length}`}
          </button>
        </div>
      </div>
    </div>
  );
}
