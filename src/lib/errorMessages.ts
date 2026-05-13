/// Turn an unknown thrown value (Tauri command errors are usually `string`,
/// but JS `Error`s and arbitrary objects also flow through here) into a short,
/// sentence-case message suited for a toast or status line.
///
/// Centralised so we never surface raw Rust strings like `os error 5` or
/// `error sending request for url (…)` straight to the user. Patterns map
/// known shapes to friendlier text; everything else falls through to the
/// trimmed raw message (still better than `[object Object]`).
///
/// See `docs/copy_style.md` for the voice/tone rules these strings follow.

interface ErrorPattern {
  /// Regex matched against the raw error string (case-insensitive).
  pattern: RegExp;
  /// Replacement text. Plain string, no template — keep them short.
  message: string;
}

const PATTERNS: ErrorPattern[] = [
  // Windows / POSIX filesystem permission errors.
  { pattern: /os error 5\b|access is denied|permission denied/i,
    message: "Permission denied — check folder access." },
  // File-not-found from either OS.
  { pattern: /os error 2\b|no such file or directory|cannot find the (file|path) specified/i,
    message: "File or folder not found." },
  // Disk-full.
  { pattern: /os error 28\b|no space left on device/i,
    message: "Disk is full." },
  // Network unreachable / DNS / connection refused — typical reqwest errors.
  { pattern: /error sending request|connection refused|dns error|failed to lookup address/i,
    message: "Couldn't reach the server — check your network." },
  // Request timeout.
  { pattern: /\btimed out\b|operation timed out|request timeout/i,
    message: "Request timed out." },
  // HTTP 401 / 403 — usually a missing or wrong API key.
  { pattern: /\b401\b|unauthorized|invalid api key|authentication.*failed/i,
    message: "Authentication failed — check your API key." },
  { pattern: /\b403\b|forbidden/i,
    message: "Access forbidden — your key may lack the required scope." },
  // Rate limits.
  { pattern: /\b429\b|rate.?limit|too many requests/i,
    message: "Rate-limited — try again in a moment." },
  // Server-side LLM errors (5xx).
  { pattern: /\b5\d\d\b|internal server error|bad gateway|service unavailable/i,
    message: "Provider is unavailable — try again shortly." },
  // SQLite locking.
  { pattern: /database is locked/i,
    message: "Database is busy — try again." },
];

/// Convert an unknown thrown value into a short user-facing message.
///
/// Examples:
///   formatError("os error 5")           → "Permission denied — check folder access."
///   formatError("Error: 429 rate-limit") → "Rate-limited — try again in a moment."
///   formatError(new Error("boom"))      → "boom"
///   formatError({foo: 1})               → "[object Object]" (last resort)
export function formatError(err: unknown): string {
  const raw = extractMessage(err);
  if (!raw) return "Something went wrong.";
  for (const { pattern, message } of PATTERNS) {
    if (pattern.test(raw)) return message;
  }
  return raw;
}

/// Like `formatError` but prefixes a context phrase. Use when the surrounding
/// label alone doesn't make the failure obvious.
///
/// formatErrorWithContext("Couldn't save rating", err)
///   → "Couldn't save rating — Permission denied — check folder access."
export function formatErrorWithContext(context: string, err: unknown): string {
  const msg = formatError(err);
  return `${context} — ${msg}`;
}

/// Coarse categorisation for caller-side branching (used by Curator failure
/// copy to pick a recovery action). Categories are derived from the same
/// patterns as `formatError`.
export type ErrorCategory =
  | "permission"
  | "not_found"
  | "network"
  | "timeout"
  | "auth"
  | "rate_limit"
  | "server"
  | "schema"
  | "unknown";

export function classifyError(err: unknown): ErrorCategory {
  const raw = extractMessage(err).toLowerCase();
  if (/os error 5\b|access is denied|permission denied/.test(raw)) return "permission";
  if (/os error 2\b|no such file or directory|cannot find the/.test(raw)) return "not_found";
  if (/\b401\b|unauthorized|invalid api key|authentication.*failed|\b403\b|forbidden/.test(raw))
    return "auth";
  if (/\b429\b|rate.?limit|too many requests/.test(raw)) return "rate_limit";
  if (/\b5\d\d\b|internal server error|bad gateway|service unavailable/.test(raw))
    return "server";
  if (/timed out|operation timed out|request timeout/.test(raw)) return "timeout";
  if (/error sending request|connection refused|dns error|failed to lookup/.test(raw))
    return "network";
  if (/schema|json|deserialize|missing field|invalid type|parse error/.test(raw))
    return "schema";
  return "unknown";
}

function extractMessage(err: unknown): string {
  if (err == null) return "";
  if (typeof err === "string") return err.trim();
  if (err instanceof Error) return err.message.trim();
  if (typeof err === "object") {
    const maybe = err as { message?: unknown };
    if (typeof maybe.message === "string") return maybe.message.trim();
  }
  return String(err).trim();
}
