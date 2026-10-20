import type {
  CardScan,
  CardScanSummary,
  ImportRequest,
  ImportStarted,
  Outcome,
  OutcomeResponse,
  ServerStatus,
  ShootReview,
  ShootSummary,
  TrashReport,
} from "./types";

/** A non-2xx response; `message` is the server's `{"error": ...}` text. */
export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

async function errorMessage(res: Response): Promise<string> {
  const fallback = `${res.status} ${res.statusText}`.trim();
  let text: string;
  try {
    text = await res.text();
  } catch {
    return fallback;
  }
  if (!text) return fallback;
  try {
    const body: unknown = JSON.parse(text);
    if (
      typeof body === "object" &&
      body !== null &&
      "error" in body &&
      typeof body.error === "string"
    ) {
      return body.error;
    }
  } catch {
    // Not JSON (the Host/Origin guard answers in plain text): show it as is.
  }
  return text;
}

async function request<T>(
  method: "GET" | "POST",
  path: string,
  body?: unknown,
): Promise<T> {
  const init: RequestInit = { method };
  if (body !== undefined) {
    init.headers = { "Content-Type": "application/json" };
    init.body = JSON.stringify(body);
  }
  const res = await fetch(path, init);
  if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
  return (await res.json()) as T;
}

export const getStatus = () => request<ServerStatus>("GET", "/api/status");

export const listCards = () => request<CardScanSummary[]>("GET", "/api/cards");

export const scan = (source: string) =>
  request<CardScan>("POST", "/api/scan", { source });

export const startImport = (req: ImportRequest) =>
  request<ImportStarted>("POST", "/api/import", req);

export const listShoots = () => request<ShootSummary[]>("GET", "/api/shoots");

export const getReview = (shootId: number) =>
  request<ShootReview>("GET", `/api/shoots/${shootId}/review`);

/** Omit `blooper` to leave the tag as it is. */
export const setOutcome = (photoId: number, outcome: Outcome, blooper?: boolean) =>
  request<OutcomeResponse>("POST", `/api/photos/${photoId}/outcome`, {
    outcome,
    blooper,
  });

/** The explicit batch approval: sends the shoot's `delete` photos to the Recycle Bin. */
export const trashDeletes = (shootId: number) =>
  request<TrashReport>("POST", `/api/shoots/${shootId}/trash`);

export const previewUrl = (photoId: number) => `/media/preview/${photoId}`;
export const thumbUrl = (photoId: number) => `/media/thumb/${photoId}`;
