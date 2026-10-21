import { describe, expect, it } from "vitest";

import { BINDINGS, isTypingTarget, resolveKey } from "./keyboard";

describe("resolveKey", () => {
  it("maps the documented keys", () => {
    expect(resolveKey({ key: "a" })).toEqual({ kind: "outcome", outcome: "album" });
    expect(resolveKey({ key: "A" })).toEqual({ kind: "outcome", outcome: "album" });
    expect(resolveKey({ key: "x" })).toEqual({ kind: "outcome", outcome: "delete" });
    expect(resolveKey({ key: "ArrowDown" })).toEqual({ kind: "moment", delta: 1 });
    expect(resolveKey({ key: "3" })).toEqual({ kind: "filter", outcome: "unreviewed" });
    expect(resolveKey({ key: "?" })).toEqual({ kind: "help" });
  });

  it("tells y from Y and nothing else by case", () => {
    expect(resolveKey({ key: "y" })).toEqual({ kind: "accept", scope: "one" });
    expect(resolveKey({ key: "Y" })).toEqual({ kind: "accept", scope: "moment" });
    expect(resolveKey({ key: "L" })).toEqual({ kind: "outcome", outcome: "library" });
  });

  it("leaves browser shortcuts and unknown keys alone", () => {
    expect(resolveKey({ key: "a", ctrlKey: true })).toBeNull();
    expect(resolveKey({ key: "r", metaKey: true })).toBeNull();
    expect(resolveKey({ key: "z" })).toBeNull();
  });

  it("has no key bound twice", () => {
    const keys = BINDINGS.flatMap((b) => b.keys);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe("isTypingTarget", () => {
  it("lets Enter reach a focused button but not arrows", () => {
    const button = { tagName: "BUTTON" } as unknown as EventTarget;
    expect(isTypingTarget(button, "Enter")).toBe(true);
    expect(isTypingTarget(button, "ArrowRight")).toBe(false);
    expect(isTypingTarget({ tagName: "INPUT" } as unknown as EventTarget, "a")).toBe(true);
    expect(isTypingTarget(null, "a")).toBe(false);
  });
});
