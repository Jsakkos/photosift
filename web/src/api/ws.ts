import { useEffect, useRef } from "react";

import { SERVER_EVENT_TYPES, type ServerEvent } from "./types";

const BASE_DELAY_MS = 500;
const MAX_DELAY_MS = 10_000;

/** Wait before reconnect attempt number `attempt` (0 = first retry). */
export function backoffDelay(attempt: number): number {
  return Math.min(BASE_DELAY_MS * 2 ** Math.max(attempt, 0), MAX_DELAY_MS);
}

/** Null for anything that is not a known event, so a newer server cannot crash an older page. */
export function parseServerEvent(text: string): ServerEvent | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null || !("type" in value)) {
    return null;
  }
  const known = SERVER_EVENT_TYPES.some((t) => t === value.type);
  return known ? (value as ServerEvent) : null;
}

function socketUrl(): string {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  return `${scheme}://${location.host}/ws`;
}

/**
 * Delivers every server event to `handler` for as long as the component is
 * mounted, reconnecting with exponential backoff when the socket drops. The
 * latest `handler` and `onOpen` are always used, so callers need not memoise
 * them. `onOpen` runs each time the socket opens, the first connect and every
 * reconnect: events sent while it was down are gone, so it is the place to
 * fetch state again.
 */
export function useServerEvents(
  handler: (event: ServerEvent) => void,
  onOpen?: () => void,
): void {
  const handlerRef = useRef(handler);
  const onOpenRef = useRef(onOpen);
  useEffect(() => {
    handlerRef.current = handler;
    onOpenRef.current = onOpen;
  });

  useEffect(() => {
    let socket: WebSocket | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let attempt = 0;
    let stopped = false;

    const connect = () => {
      const ws = new WebSocket(socketUrl());
      socket = ws;
      ws.onopen = () => {
        attempt = 0;
        onOpenRef.current?.();
      };
      ws.onmessage = (msg: MessageEvent<unknown>) => {
        if (typeof msg.data !== "string") return;
        const event = parseServerEvent(msg.data);
        if (event) handlerRef.current(event);
      };
      // `close` follows `error`, so reconnecting only from `close` schedules
      // exactly one retry per dropped connection.
      ws.onclose = () => {
        if (stopped) return;
        timer = setTimeout(connect, backoffDelay(attempt));
        attempt += 1;
      };
    };

    connect();
    return () => {
      stopped = true;
      clearTimeout(timer);
      if (socket) {
        socket.onclose = null;
        socket.close();
      }
    };
  }, []);
}
