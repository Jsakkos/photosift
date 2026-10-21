import { useCallback, useEffect, useRef, useState } from "react";

import { ApiError, getChat, sendChat } from "../api/client";
import type { ChatMessage, ServerEvent } from "../api/types";
import { useServerEvents } from "../api/ws";

/** A turn that is still ending when the next message is sent answers 409; the send waits it out. */
const BUSY_RETRIES = 6;
const BUSY_WAIT_MS = 500;

interface Props {
  shootId: number;
  open: boolean;
  onClose: () => void;
}

function pause(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * The right-side chat drawer over the shoot's helper session. It is always
 * mounted so the conversation keeps arriving while it is closed. Review keys
 * are inactive while the textarea has focus (see `isTypingTarget`); Esc in the
 * textarea only blurs it, and the page closes the drawer on the next Esc.
 */
export function HelperPanel({ shootId, open, onClose }: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [waiting, setWaiting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  const log = useRef<HTMLDivElement>(null);
  const localId = useRef(0);

  const load = useCallback(() => {
    getChat(shootId).then(setMessages, () => {});
  }, [shootId]);

  useEffect(() => {
    setMessages([]);
    setWaiting(false);
    setProblem(null);
    load();
  }, [load]);

  // The socket's open handler also covers a reconnect: answers sent while it was down are fetched.
  useServerEvents((event: ServerEvent) => {
    if (event.type !== "chat_message" || event.shoot_id !== shootId) return;
    localId.current += 1;
    const id = -localId.current;
    setMessages((list) => [
      ...list,
      { id, role: event.role, text: event.text, at: new Date().toISOString() },
    ]);
    setWaiting(false);
  }, load);

  useEffect(() => {
    if (open) box.current?.focus();
    else box.current?.blur();
  }, [open]);

  useEffect(() => {
    const el = log.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages, waiting, open]);

  const send = async () => {
    const text = draft.trim();
    if (!text || sending) return;
    setProblem(null);
    setSending(true);
    setDraft("");
    localId.current += 1;
    const pending: ChatMessage = {
      id: -localId.current,
      role: "user",
      text,
      at: new Date().toISOString(),
    };
    setMessages((list) => [...list, pending]);
    try {
      for (let attempt = 0; ; attempt += 1) {
        try {
          await sendChat(shootId, text);
          break;
        } catch (e) {
          if (!(e instanceof ApiError && e.status === 409 && attempt < BUSY_RETRIES)) throw e;
          await pause(BUSY_WAIT_MS);
        }
      }
      setWaiting(true);
    } catch (e) {
      setMessages((list) => list.filter((m) => m.id !== pending.id));
      setDraft(text);
      setProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setSending(false);
    }
  };

  return (
    <aside className="helper-panel" aria-label="Helper chat" hidden={!open}>
      <header className="helper-panel-head">
        <h2>Helper</h2>
        <button type="button" onClick={onClose} aria-label="Close helper chat">
          Close
        </button>
      </header>
      <div className="helper-log" ref={log} role="log" aria-live="polite">
        {messages.length === 0 && (
          <p className="muted">
            Ask about this shoot, for example &ldquo;which photo has the best smile in moment
            2?&rdquo; The helper can change its suggestions; you still decide every photo.
          </p>
        )}
        {messages.map((m) => (
          <p key={m.id} className={`chat-message chat-${m.role}`} data-role={m.role}>
            {m.text}
          </p>
        ))}
        {waiting && (
          <p className="muted" role="status">
            Claude is thinking…
          </p>
        )}
      </div>
      {problem && (
        <p className="error helper-problem-line" role="alert">
          {problem}
        </p>
      )}
      <textarea
        ref={box}
        className="helper-input"
        rows={3}
        value={draft}
        placeholder="Message the helper (Enter sends, Shift+Enter adds a line)"
        aria-label="Message the helper"
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            void send();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.currentTarget.blur();
          }
        }}
      />
    </aside>
  );
}
