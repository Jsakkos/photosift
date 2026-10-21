import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "react-router";

import { getTaste, getTasteDecisions, saveTaste } from "../api/client";
import type { TasteDecision, TasteProfile } from "../api/types";
import { useServerEvents } from "../api/ws";
import { OUTCOME_LABEL } from "../review/model";

const SAVE_DELAY_MS = 800;
const TASTE_MAX = 4000;

type SaveState = "idle" | "pending" | "saving" | "saved" | "failed";

function dateOf(stamp: string): string {
  const d = new Date(stamp);
  return Number.isNaN(d.getTime()) ? stamp : d.toLocaleDateString();
}

/** The helper's taste notes, editable, and the recent times the user disagreed with it. */
export function TastePage() {
  const [text, setText] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [author, setAuthor] = useState<TasteProfile["updated_by"]>("user");
  const [changedAt, setChangedAt] = useState("");
  const [save, setSave] = useState<SaveState>("idle");
  const [problem, setProblem] = useState<string | null>(null);
  const [decisions, setDecisions] = useState<TasteDecision[] | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const latest = useRef("");
  const dirty = useRef(false);

  const loadProfile = useCallback(() => {
    getTaste().then(
      (p) => {
        // Never overwrite what the user is typing.
        if (!dirty.current) {
          setText(p.text);
          latest.current = p.text;
        }
        setAuthor(p.updated_by);
        setChangedAt(p.updated_at);
        setLoaded(true);
      },
      (e: unknown) => setProblem(e instanceof Error ? e.message : String(e)),
    );
  }, []);

  const loadDecisions = useCallback(() => {
    getTasteDecisions(50).then(setDecisions, () => setDecisions([]));
  }, []);

  useEffect(() => {
    loadProfile();
    loadDecisions();
  }, [loadProfile, loadDecisions]);

  useServerEvents((event) => {
    if (event.type === "taste_changed") loadProfile();
  }, loadProfile);

  const flush = useCallback(() => {
    window.clearTimeout(timer.current);
    timer.current = undefined;
    if (!dirty.current) return Promise.resolve();
    const sent = latest.current;
    setSave("saving");
    return saveTaste(sent).then(
      (p) => {
        if (latest.current === sent) dirty.current = false;
        setAuthor(p.updated_by);
        setChangedAt(p.updated_at);
        setProblem(null);
        setSave(dirty.current ? "pending" : "saved");
      },
      (e: unknown) => {
        setProblem(e instanceof Error ? e.message : String(e));
        setSave("failed");
      },
    );
  }, []);

  // Leaving the page keeps what was typed in the last moments.
  useEffect(
    () => () => {
      if (dirty.current) void saveTaste(latest.current).catch(() => {});
      window.clearTimeout(timer.current);
    },
    [],
  );

  const onChange = (value: string) => {
    setText(value);
    latest.current = value;
    dirty.current = true;
    setSave("pending");
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => void flush(), SAVE_DELAY_MS);
  };

  const status =
    save === "saved"
      ? "Saved"
      : save === "saving"
        ? "Saving…"
        : save === "pending"
          ? "Unsaved changes"
          : save === "failed"
            ? "Not saved"
            : "";

  return (
    <section className="taste-page">
      <div className="page-head">
        <h1>Taste</h1>
        <Link to="/" className="page-head-link">
          Shoots
        </Link>
      </div>
      <p className="muted">
        The helper reads these notes and your last 40 disagreements before every run.
      </p>
      {author === "helper" && (
        <p className="taste-notice" role="status" data-testid="taste-helper-notice">
          The helper changed these notes on {dateOf(changedAt)} &mdash; check they still say what
          you want.
        </p>
      )}
      <label className="taste-label" htmlFor="taste-notes">
        What you keep and what you reject
      </label>
      <textarea
        id="taste-notes"
        className="taste-notes"
        rows={10}
        maxLength={TASTE_MAX}
        disabled={!loaded}
        value={text}
        placeholder="For example: I keep candid laughs over posed smiles, and I archive near-duplicates unless the eyes are better."
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => void flush()}
      />
      <p className="taste-status" role="status" data-testid="taste-save-state">
        {status}
        <span className="muted taste-count">
          {" "}
          {text.length}/{TASTE_MAX}
        </span>
      </p>
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}

      <h2>Where you disagreed</h2>
      {decisions === null && <p className="muted">Loading…</p>}
      {decisions !== null && decisions.length === 0 && (
        <p className="muted">
          Nothing yet. When you choose differently from a suggestion, it shows up here.
        </p>
      )}
      {decisions !== null && decisions.length > 0 && (
        <ul className="taste-decisions">
          {decisions.map((d) => (
            <li key={`${d.photo_id}-${d.decided_at}`} className="taste-decision">
              <Link to={`/review/${d.shoot_id}`}>
                <img
                  src={`/media/thumb/${d.photo_id}`}
                  alt={`Photo ${d.photo_id}`}
                  loading="lazy"
                />
              </Link>
              <div>
                <p>
                  Suggested <strong>{OUTCOME_LABEL[d.proposed]}</strong> because &ldquo;
                  {d.proposed_reason}&rdquo; &rarr; you chose <strong>{OUTCOME_LABEL[d.chosen]}</strong>
                </p>
                <p className="muted">
                  {d.criterion ? `Because of ${d.criterion}` : "No reason given"}
                  {d.note ? `: ${d.note}` : ""} &middot; {dateOf(d.decided_at)}
                </p>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
