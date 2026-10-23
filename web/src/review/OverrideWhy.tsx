import { useState } from "react";

import type { Criterion } from "../api/types";

export const CRITERIA: readonly { id: Criterion; label: string }[] = [
  { id: "expression", label: "expression" },
  { id: "who", label: "who's in it" },
  { id: "moment", label: "moment" },
  { id: "sharpness", label: "sharpness" },
  { id: "duplicate", label: "duplicate" },
  { id: "composition", label: "composition" },
  { id: "other", label: "other" },
];

interface Props {
  photoId: number;
  chosen: Criterion | null;
  onNote: (photoId: number, criterion: Criterion, note?: string) => void;
}

/**
 * Why the user went against the suggestion. Optional and never modal: the
 * chips do not take keyboard focus, so the review keys keep working.
 */
export function OverrideWhy({ photoId, chosen, onNote }: Props) {
  const [typing, setTyping] = useState(false);
  const [text, setText] = useState("");
  return (
    <div className="override-why" role="group" aria-label="Why did you choose differently?">
      {CRITERIA.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          className="why-chip"
          aria-pressed={chosen === id}
          // A mouse press must not move focus off the review.
          onMouseDown={(e) => e.preventDefault()}
          onClick={(e) => {
            e.currentTarget.blur();
            if (id === "other") setTyping(true);
            else {
              setTyping(false);
              onNote(photoId, id);
            }
          }}
        >
          {label}
        </button>
      ))}
      {typing && (
        <input
          className="why-input"
          type="text"
          aria-label="Why, in a few words"
          placeholder="Why? (Enter to save)"
          maxLength={200}
          value={text}
          autoFocus
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              const note = text.trim();
              onNote(photoId, "other", note || undefined);
              setTyping(false);
              setText("");
              e.currentTarget.blur();
            } else if (e.key === "Escape") {
              e.stopPropagation();
              setTyping(false);
              e.currentTarget.blur();
            }
          }}
          onBlur={() => setTyping(false)}
        />
      )}
    </div>
  );
}
