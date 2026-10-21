import { describe, expect, it } from "vitest";

import { backoffDelay, parseServerEvent } from "./ws";

describe("parseServerEvent", () => {
  it("parses a known event", () => {
    const event = parseServerEvent(
      JSON.stringify({ type: "enrich_done", shoot_id: 4 }),
    );
    expect(event).toEqual({ type: "enrich_done", shoot_id: 4 });
  });

  it("drops unknown types, malformed JSON and non-objects", () => {
    expect(parseServerEvent(JSON.stringify({ type: "from_the_future" }))).toBeNull();
    expect(parseServerEvent("{nope")).toBeNull();
    expect(parseServerEvent("42")).toBeNull();
    expect(parseServerEvent("null")).toBeNull();
  });
});

describe("backoffDelay", () => {
  it("grows exponentially and is capped", () => {
    const delays = [0, 1, 2, 3, 4, 5, 6, 20].map(backoffDelay);
    expect(delays[0]).toBe(500);
    expect(delays[1]).toBe(1000);
    expect(delays[2]).toBe(2000);
    expect(delays.at(-1)).toBe(10_000);
    expect([...delays].sort((a, b) => a - b)).toEqual(delays);
  });
});
