import { describe, it, expect } from "vitest";
import {
  formatError,
  formatErrorWithContext,
  classifyError,
} from "../errorMessages";

describe("formatError", () => {
  it("maps Windows permission errors", () => {
    expect(formatError("os error 5")).toBe(
      "Permission denied — check folder access.",
    );
    expect(formatError("Access is denied. (os error 5)")).toBe(
      "Permission denied — check folder access.",
    );
  });

  it("maps POSIX permission errors", () => {
    expect(formatError("Permission denied (os error 13)")).toBe(
      "Permission denied — check folder access.",
    );
  });

  it("maps file-not-found errors", () => {
    expect(formatError("os error 2")).toBe("File or folder not found.");
    expect(formatError("No such file or directory")).toBe(
      "File or folder not found.",
    );
  });

  it("maps disk-full errors", () => {
    expect(formatError("No space left on device (os error 28)")).toBe(
      "Disk is full.",
    );
  });

  it("maps network errors", () => {
    expect(
      formatError("error sending request for url (https://api.example.com/)"),
    ).toBe("Couldn't reach the server — check your network.");
    expect(formatError("connection refused")).toBe(
      "Couldn't reach the server — check your network.",
    );
  });

  it("maps auth failures", () => {
    expect(formatError("HTTP 401 unauthorized")).toBe(
      "Authentication failed — check your API key.",
    );
    expect(formatError("invalid api key")).toBe(
      "Authentication failed — check your API key.",
    );
    expect(formatError("403 Forbidden")).toBe(
      "Access forbidden — your key may lack the required scope.",
    );
  });

  it("maps rate-limit errors", () => {
    expect(formatError("HTTP 429: rate limit exceeded")).toBe(
      "Rate-limited — try again in a moment.",
    );
    expect(formatError("Too Many Requests")).toBe(
      "Rate-limited — try again in a moment.",
    );
  });

  it("maps server errors", () => {
    expect(formatError("HTTP 503 service unavailable")).toBe(
      "Provider is unavailable — try again shortly.",
    );
    expect(formatError("internal server error")).toBe(
      "Provider is unavailable — try again shortly.",
    );
  });

  it("maps SQLite locking", () => {
    expect(formatError("database is locked")).toBe(
      "Database is busy — try again.",
    );
  });

  it("falls back to the raw message when no pattern matches", () => {
    expect(formatError("Some bespoke failure")).toBe("Some bespoke failure");
  });

  it("trims whitespace", () => {
    expect(formatError("  bespoke  ")).toBe("bespoke");
  });

  it("handles Error instances", () => {
    expect(formatError(new Error("boom"))).toBe("boom");
  });

  it("handles objects with a message property", () => {
    expect(formatError({ message: "shaped like an error" })).toBe(
      "shaped like an error",
    );
  });

  it("returns a fallback for null/undefined", () => {
    expect(formatError(null)).toBe("Something went wrong.");
    expect(formatError(undefined)).toBe("Something went wrong.");
    expect(formatError("")).toBe("Something went wrong.");
  });

  it("stringifies arbitrary values rather than crashing", () => {
    expect(formatError(42)).toBe("42");
  });
});

describe("formatErrorWithContext", () => {
  it("prefixes the context with an em dash", () => {
    expect(formatErrorWithContext("Couldn't save rating", "os error 5")).toBe(
      "Couldn't save rating — Permission denied — check folder access.",
    );
  });

  it("uses the raw message when no pattern matches", () => {
    expect(formatErrorWithContext("Couldn't reject group", "weird thing")).toBe(
      "Couldn't reject group — weird thing",
    );
  });
});

describe("classifyError", () => {
  it("classifies permission errors", () => {
    expect(classifyError("os error 5")).toBe("permission");
    expect(classifyError("Access is denied")).toBe("permission");
  });

  it("classifies auth errors", () => {
    expect(classifyError("HTTP 401")).toBe("auth");
    expect(classifyError("invalid api key")).toBe("auth");
    expect(classifyError("403 forbidden")).toBe("auth");
  });

  it("classifies rate limits", () => {
    expect(classifyError("429 rate limit")).toBe("rate_limit");
  });

  it("classifies server errors", () => {
    expect(classifyError("HTTP 502 bad gateway")).toBe("server");
  });

  it("classifies schema / parse errors", () => {
    expect(classifyError("missing field `score`")).toBe("schema");
    expect(classifyError("invalid type: string, expected number")).toBe(
      "schema",
    );
  });

  it("classifies network errors", () => {
    expect(classifyError("error sending request")).toBe("network");
    expect(classifyError("dns error")).toBe("network");
  });

  it("classifies timeouts", () => {
    expect(classifyError("operation timed out")).toBe("timeout");
  });

  it("returns 'unknown' for unmatched strings", () => {
    expect(classifyError("a creative new failure mode")).toBe("unknown");
  });
});
