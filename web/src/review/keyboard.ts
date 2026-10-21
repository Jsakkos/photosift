import type { Outcome } from "../api/types";
import { FILTER_ORDER } from "./model";

export type Action =
  | { kind: "moment"; delta: -1 | 1 }
  | { kind: "photo"; delta: -1 | 1 }
  | { kind: "outcome"; outcome: Outcome }
  | { kind: "blooper" }
  | { kind: "loupe" }
  | { kind: "compare" }
  | { kind: "back" }
  | { kind: "filter"; outcome: Outcome }
  | { kind: "help" };

export interface Binding {
  /** `KeyboardEvent.key` values (letters are matched case-insensitively). */
  keys: readonly string[];
  /** What the `?` overlay shows. */
  label: string;
  description: string;
  action: Action;
}

const FILTER_NAME: Record<Outcome, string> = {
  album: "album",
  library: "library",
  unreviewed: "your call",
  archive: "archive",
  delete: "delete?",
};

/** The only key map: the handler resolves keys through it and `?` renders it. */
export const BINDINGS: readonly Binding[] = [
  { keys: ["ArrowUp"], label: "↑", description: "Previous moment", action: { kind: "moment", delta: -1 } },
  { keys: ["ArrowDown"], label: "↓", description: "Next moment", action: { kind: "moment", delta: 1 } },
  { keys: ["ArrowLeft"], label: "←", description: "Previous photo", action: { kind: "photo", delta: -1 } },
  { keys: ["ArrowRight"], label: "→", description: "Next photo", action: { kind: "photo", delta: 1 } },
  { keys: ["a"], label: "A", description: "Album", action: { kind: "outcome", outcome: "album" } },
  { keys: ["l"], label: "L", description: "Library", action: { kind: "outcome", outcome: "library" } },
  { keys: ["r"], label: "R", description: "Archive", action: { kind: "outcome", outcome: "archive" } },
  { keys: ["x"], label: "X", description: "Delete?", action: { kind: "outcome", outcome: "delete" } },
  { keys: ["u"], label: "U", description: "Back to your call", action: { kind: "outcome", outcome: "unreviewed" } },
  { keys: ["b"], label: "B", description: "Toggle blooper", action: { kind: "blooper" } },
  { keys: ["Enter"], label: "Enter", description: "Open or close the full-size view", action: { kind: "loupe" } },
  { keys: ["c"], label: "C", description: "Compare this burst side by side", action: { kind: "compare" } },
  { keys: ["Escape"], label: "Esc", description: "Close, then go back to the shoots", action: { kind: "back" } },
  ...FILTER_ORDER.map(
    (outcome, i): Binding => ({
      keys: [String(i + 1)],
      label: String(i + 1),
      description: `Show only ${FILTER_NAME[outcome]} (press again to clear)`,
      action: { kind: "filter", outcome },
    }),
  ),
  { keys: ["?"], label: "?", description: "This list", action: { kind: "help" } },
];

export interface KeyLike {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

export function resolveKey(e: KeyLike): Action | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  return BINDINGS.find((b) => b.keys.includes(key))?.action ?? null;
}

/** Keys typed into a field, or Enter/Space on a focused button, are not shortcuts. */
export function isTypingTarget(target: EventTarget | null, key: string): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.tagName !== "string") return false;
  const tag = el.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable) {
    return true;
  }
  return tag === "BUTTON" && (key === "Enter" || key === " ");
}
