import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Module-level reset so each test re-evaluates the env-flag check at
// import time. Vitest hoists vi.mock; we use dynamic imports here
// because the real value of import.meta.env is read once at module
// init, and we need to flip it between test cases.

describe("debugFocus", () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.resetModules();
    consoleSpy = vi.spyOn(console, "log").mockImplementation(() => {});
  });

  afterEach(() => {
    consoleSpy.mockRestore();
  });

  it("is inert (no console output, isFocusDebugEnabled=false) when env flag is unset", async () => {
    vi.stubEnv("VITE_DEBUG_FOCUS", "");
    const { logFocus, isFocusDebugEnabled } = await import("../debugFocus");

    expect(isFocusDebugEnabled()).toBe(false);
    logFocus("test message");
    expect(consoleSpy).not.toHaveBeenCalled();
    vi.unstubAllEnvs();
  });

  it("logs to console when env flag is '1'", async () => {
    vi.stubEnv("VITE_DEBUG_FOCUS", "1");
    const { logFocus, isFocusDebugEnabled } = await import("../debugFocus");

    expect(isFocusDebugEnabled()).toBe(true);
    logFocus("alt-tab returned");
    expect(consoleSpy).toHaveBeenCalledTimes(1);
    const callArg = consoleSpy.mock.calls[0]?.[0] as string;
    expect(callArg).toContain("[focus]");
    expect(callArg).toContain("alt-tab returned");
    expect(callArg).toContain("ae=");
    expect(callArg).toContain("t=");
    vi.unstubAllEnvs();
  });

  it("describes the active element with tag, id, and classes when present", async () => {
    vi.stubEnv("VITE_DEBUG_FOCUS", "1");
    const { logFocus } = await import("../debugFocus");

    const div = document.createElement("div");
    div.id = "shell";
    div.className = "outline-none flex";
    div.tabIndex = -1;
    document.body.appendChild(div);
    div.focus();

    logFocus("focused");
    const callArg = consoleSpy.mock.calls[0]?.[0] as string;
    expect(callArg).toContain("ae=div#shell.outline-none.flex");

    document.body.removeChild(div);
    vi.unstubAllEnvs();
  });

  it("renders ae=null when nothing is focused", async () => {
    vi.stubEnv("VITE_DEBUG_FOCUS", "1");
    const { logFocus } = await import("../debugFocus");

    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }

    logFocus("blank");
    const callArg = consoleSpy.mock.calls[0]?.[0] as string;
    // happy-dom defaults activeElement to body when nothing is focused;
    // that's still a valid description string. We just want the call to
    // not throw and to include a recognizable token.
    expect(callArg).toMatch(/ae=(null|body)/);
    vi.unstubAllEnvs();
  });
});
