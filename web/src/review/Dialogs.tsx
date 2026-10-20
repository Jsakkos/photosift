import { useEffect, useRef } from "react";

import { BINDINGS } from "./keyboard";
import { plural } from "./model";
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
