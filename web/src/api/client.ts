import type {
  CardScan,
  ChatMessage,
  CardScanSummary,
  ClientSettings,
  Criterion,
  DecisionNoteRequest,
  HelperRunStarted,
  ImportRequest,
  ImportStarted,
  JobStateReply,
  Outcome,
  OutcomeResponse,
  ProposedEvent,
  ServerStatus,
  ShootReview,
  ShootSummary,
  TasteDecision,
  TasteProfile,
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
  method: "GET" | "POST" | "PUT",
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

/** The cached scan; a 404 means it was evicted and the card must be scanned again. */
export const getScan = (scanId: string) =>
  request<CardScan>("GET", `/api/scan/${encodeURIComponent(scanId)}`);

/** Events re-proposed by the server's split rule; offsets are seconds per camera. */
export const regroupEvents = (
  scanId: string,
  cameraOffsets: Record<string, number>,
) =>
  request<ProposedEvent[]>(
    "POST",
    `/api/scan/${encodeURIComponent(scanId)}/events`,
    { camera_offsets: cameraOffsets },
  );

/** The folder-name slug the server will use for an event name. */
export const slugFor = async (name: string): Promise<string> =>
  (await request<{ slug: string }>("POST", "/api/slug", { name })).slug;

export const getSettings = () => request<ClientSettings>("GET", "/api/settings");

export const startImport = (req: ImportRequest) =>
  request<ImportStarted>("POST", "/api/import", req);

/** The server's record of an import job; a 404 means it was forgotten. */
export const getJob = (jobId: string) =>
  request<JobStateReply>("GET", `/api/import/${encodeURIComponent(jobId)}`);

/** The import job running now, if any (`job_id` is null when idle). */
export const currentJob = () =>
  request<{ job_id: string | null }>("GET", "/api/import/current");

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
/** Trashes only `photoIds`, the photos the confirm dialog listed. */
export const trashDeletes = (shootId: number, photoIds: number[]) =>
  request<TrashReport>("POST", `/api/shoots/${shootId}/trash`, { photo_ids: photoIds });

/** Runs the helper on the shoot now: 409 while one is queued or running, 503 when unavailable. */
export const runHelper = (shootId: number) =>
  request<HelperRunStarted>("POST", `/api/shoots/${shootId}/helper/run`);

/** Stops the shoot's helper run (it fails "stopped by you"): 204, or 404 when there is none. */
export const cancelHelper = async (shootId: number): Promise<void> => {
  const res = await fetch(`/api/shoots/${shootId}/helper/cancel`, { method: "POST" });
  if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
};

/** The shoot's conversation with the helper. */
export const getChat = (shootId: number) =>
  request<ChatMessage[]>("GET", `/api/shoots/${shootId}/chat`);

/** 202 once the helper is answering (the answer arrives as `chat_message` events); 409 while it is still on the last message. */
export const sendChat = async (shootId: number, text: string): Promise<void> => {
  const res = await fetch(`/api/shoots/${shootId}/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text }),
  });
  if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
};

/** Why the user overrode a proposal; `note` goes with `other`. */
export const setDecisionNote = async (
  photoId: number,
  criterion: Criterion,
  note?: string,
): Promise<void> => {
  const body: DecisionNoteRequest = note === undefined ? { criterion } : { criterion, note };
  const res = await fetch(`/api/photos/${photoId}/decision-note`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new ApiError(res.status, await errorMessage(res));
};

export const previewUrl = (photoId: number) => `/media/preview/${photoId}`;
/** A thumbnail of a picture still on the card, by scan and item index. */
export const scanThumbUrl = (scanId: string, idx: number) =>
  `/media/scan/${encodeURIComponent(scanId)}/${idx}`;
export const thumbUrl = (photoId: number) => `/media/thumb/${photoId}`;

export const getTaste = () => request<TasteProfile>("GET", "/api/taste");

/** 400 over 4000 characters. */
export const saveTaste = (text: string) => request<TasteProfile>("PUT", "/api/taste", { text });

/** The newest overrides, newest first. */
export const getTasteDecisions = (limit = 50) =>
  request<TasteDecision[]>("GET", `/api/taste/decisions?limit=${limit}`);
