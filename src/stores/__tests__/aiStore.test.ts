import { describe, it, expect, beforeEach } from "vitest";
import { useAiStore } from "../aiStore";

describe("aiStore", () => {
  beforeEach(() => {
    useAiStore.setState({ provider: "disabled", analyzed: 0, failed: 0, total: 0 });
  });

  it("patches progress counters from ai-progress event", () => {
    useAiStore.getState().handleProgress({
      photoId: 5, ok: true, done: 3, total: 10, failed: 0,
    });
    const s = useAiStore.getState();
    expect(s.analyzed).toBe(3);
    expect(s.total).toBe(10);
    expect(s.failed).toBe(0);
  });

  it("tracks failures separately", () => {
    useAiStore.getState().handleProgress({ photoId: 5, ok: false, done: 4, total: 10, failed: 2 });
    const s = useAiStore.getState();
    expect(s.failed).toBe(2);
    expect(s.analyzed).toBe(2); // done=4 minus failed=2
    expect(s.total).toBe(10);
  });

  it("resets counters on reset()", () => {
    useAiStore.getState().handleProgress({ photoId: 5, ok: true, done: 3, total: 10, failed: 0 });
    useAiStore.getState().reset();
    const s = useAiStore.getState();
    expect(s.analyzed).toBe(0);
    expect(s.total).toBe(0);
    expect(s.failed).toBe(0);
  });

  it("setProvider changes the provider status", () => {
    useAiStore.getState().setProvider("cuda");
    expect(useAiStore.getState().provider).toBe("cuda");
    useAiStore.getState().setProvider("disabled");
    expect(useAiStore.getState().provider).toBe("disabled");
  });
});
