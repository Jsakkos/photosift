# PhotoSift copy style

A single page that codifies how PhotoSift talks to the photographer. Keep it
honest, short, and active. The goal isn't a corporate voice — it's the same
direct tone the rest of the app uses.

When you write a new user-visible string, run it past the checklist below. When
in doubt, copy a pattern from an existing example rather than inventing a new one.

## View names

The three culling passes have proper nouns: **Triage**, **Select**, **Route**.
The shoots list is the **Library**.

- Always capitalise. They are names, not verbs.
- Never say "Triage view" or "Select page" — drop the noun.
- "Route to Edit" / "Route to Publish Direct" use the destination names as
  proper nouns too.

## Case

| Where | Case |
|---|---|
| Sentence / body / paragraph copy | Sentence case |
| Empty-state copy | Sentence case |
| Toast headlines (≤ 6 words) | Sentence case |
| Toast body sentences | Sentence case |
| Short button labels (≤ 2 words) | Title case OK ("Re-run", "Browse", "Save") |
| Multi-word button labels | Sentence case ("Back to shoots") |
| Dialog headings | Sentence case ("Settings", "Re-cluster current shoot") |
| Section headings | Sentence case |
| Keyboard hint chips | UPPERCASE for the kbd glyph; lowercase for the label |

## Punctuation

- **No trailing period** on short labels, headings, button text, or toast
  headlines. ("Couldn't load your shoots", not "Couldn't load your shoots.")
- **Period** on full-sentence body copy, helper text, dialog descriptions.
- **Em dash (—)** between a label and its reason or detail.
  ("Permission denied — check folder access.")
- **Ellipsis (…)** — single character — for in-flight states ("Loading…",
  "Testing…", "Re-clustering…"). Never three dots.
- No exclamation marks. Ever.

## Tense and voice

| Situation | Pattern | Example |
|---|---|---|
| Failure / error toast | Past passive: "Couldn't [verb] [noun]" | "Couldn't save rating" |
| Failure with detail | Headline em-dash detail | "Couldn't save rating — Permission denied" |
| Prompt / required input | Present imperative | "Select at least one photo" |
| Success confirmation | Past active | "Rated 3★", "Routed to Edit" |
| In-flight | Present continuous, ellipsis | "Re-clustering…", "Importing…" |
| Empty state | Present indicative | "Your library is empty" |

Avoid:

- "Failed to X" → say "Couldn't X" instead. Same meaning, friendlier voice.
- Bare "Error:" prefix. The toast colour already says "this is bad."
- Passive past with no actor ("X was not saved"). Make it about the user's
  action: "Couldn't save X".

## Error wrapping

Tauri commands surface raw Rust error strings (`os error 5`,
`error sending request for url …`). **Never show these to the user.** Run
every error through `src/lib/errorMessages.ts`:

```ts
import { formatError } from "../lib/errorMessages";
get().setToast(`Couldn't save rating — ${formatError(e)}`, "error");
```

`formatError` matches known patterns and falls back to the trimmed raw
message when no pattern applies. When you encounter a new shape, add it to
`PATTERNS` (with a test) rather than special-casing the call site.

For Curator-style failures where you also want to branch on the failure
class, use `classifyError(e)` — it returns one of `permission · not_found ·
network · timeout · auth · rate_limit · server · schema · unknown`.

## Pluralisation

Inline ternaries are preferred over a helper:

```ts
`${n} photo${n === 1 ? "" : "s"}`
`${groupCount} group${groupCount === 1 ? "" : "s"}`
```

For star ratings use the `★` glyph: `"Rated 3★"`, `"3★+ only"`.

## Numbers and quantities

- Stars: `3★` (literal star glyph, no space).
- File counts: `12 photos` (sentence case if mid-sentence, just the number
  + unit if standalone).
- Currency: `$0.42` (dollar prefix, two decimals when < $10).
- Progress: `Routed 12 → Edit` (right-arrow with spaces).

## Specific recurring strings

Keep these consistent across the app:

| Used for | Canonical string |
|---|---|
| Failed shoot list load | "Couldn't load your shoots" |
| Failed single-shoot load | "Couldn't load this shoot" |
| Empty library | "Your library is empty. Import a folder of RAW files to start culling." |
| No-op toast (undo with nothing left) | "Nothing to undo" / "Nothing to redo" |
| In-flight test | "Testing…" |
| Test success | "Connection OK" |
| Test failure | "Couldn't reach the provider — \<formatted error\>" |
| Recluster running | "Re-clustering…" |
| Recluster done | `` `Re-clustered into ${n} group${n === 1 ? "" : "s"}` `` |
| Reanalyze running | "Queuing…" |
| Reanalyze done | "Re-analysis queued" |

## Checklist before merging copy

1. Sentence case where it should be?
2. No trailing period on short labels / headings / toast headlines?
3. Errors run through `formatError`?
4. View names capitalised, no "view"/"page" suffix?
5. Pluralisation handled (no `${n} photos` when `n` could be 1)?
6. Reads naturally out loud?

When a string can't fit any of these patterns, prefer to redesign the surface
rather than break the rules. Microcopy that fights the system reads weirdly,
and the photographer will notice.
