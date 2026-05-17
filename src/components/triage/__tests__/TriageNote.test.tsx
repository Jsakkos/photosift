/// Component tests for the Triage-tab verdict chip. The bug these guard
/// against: the Triage tab used to render the *selection-stage* Curator
/// chip (`curator_judgments`), so a frame the triage pass kept could
/// show a contradictory editorial "reject — closed eyes". `TriageNote`
/// reads the triage stage's own table (`triage_judgments`) instead.

import { render, screen } from "@testing-library/react";
import { describe, it, expect, beforeEach } from "vitest";
import { TriageNote } from "../TriageNote";
import { useProjectStore } from "../../../stores/projectStore";
import { makeImage } from "../../../test/fixtures";
import type { CuratorJudgment, TriageJudgment } from "../../../types";

function triageJudgment(overrides: Partial<TriageJudgment> = {}): TriageJudgment {
  return {
    photoId: 1,
    shootId: 1,
    suggestedFlag: "keep",
    reason: "No technical defect detected.",
    applied: false,
    judgedAt: "2026-05-16T10:00:00",
    model: "claude-test",
    promptVersion: 1,
    ...overrides,
  };
}

function curatorJudgment(overrides: Partial<CuratorJudgment> = {}): CuratorJudgment {
  return {
    photoId: 1,
    shootId: 1,
    composition: 7,
    aesthetic: 6,
    clusterRank: null,
    isKeeper: false,
    suggestedFlag: "reject",
    reason: "Closed eyes — weakest of the burst.",
    userAction: null,
    judgedAt: "2026-05-16T11:00:00",
    provider: "anthropic",
    model: "claude-test",
    promptVersion: 1,
    ...overrides,
  };
}

/// Seed `displayItems`/`currentIndex` so the chip resolves a current photo.
function seedCurrentPhoto(photoId: number) {
  const image = makeImage({ id: photoId });
  useProjectStore.setState({
    images: [image],
    displayItems: [{ imageIndex: 0, image }],
    currentIndex: 0,
  });
}

describe("TriageNote", () => {
  beforeEach(() => {
    useProjectStore.setState({
      triageJudgments: new Map<number, TriageJudgment>(),
      curatorJudgments: new Map<number, CuratorJudgment>(),
    });
  });

  it("renders the triage-stage reason for the current photo", () => {
    seedCurrentPhoto(1);
    useProjectStore.setState({
      triageJudgments: new Map([[1, triageJudgment({ photoId: 1 })]]),
    });

    render(<TriageNote />);

    expect(screen.getByText("No technical defect detected.")).toBeTruthy();
  });

  it("labels a keep verdict as clear", () => {
    seedCurrentPhoto(1);
    useProjectStore.setState({
      triageJudgments: new Map([
        [1, triageJudgment({ photoId: 1, suggestedFlag: "keep" })],
      ]),
    });

    render(<TriageNote />);

    expect(screen.getByText(/Triage AI · clear/i)).toBeTruthy();
  });

  it("labels a reject verdict as flagged", () => {
    seedCurrentPhoto(1);
    useProjectStore.setState({
      triageJudgments: new Map([
        [
          1,
          triageJudgment({
            photoId: 1,
            suggestedFlag: "reject",
            reason: "Severe motion blur across the frame.",
          }),
        ],
      ]),
    });

    render(<TriageNote />);

    expect(screen.getByText(/Triage AI · flagged/i)).toBeTruthy();
    expect(screen.getByText("Severe motion blur across the frame.")).toBeTruthy();
  });

  it("marks an applied reject as auto-rejected", () => {
    seedCurrentPhoto(1);
    useProjectStore.setState({
      triageJudgments: new Map([
        [
          1,
          triageJudgment({ photoId: 1, suggestedFlag: "reject", applied: true }),
        ],
      ]),
    });

    render(<TriageNote />);

    expect(screen.getByText(/auto-rejected/i)).toBeTruthy();
  });

  it("shows an empty state when the current photo has no triage judgment", () => {
    seedCurrentPhoto(99);

    render(<TriageNote />);

    expect(screen.getByText(/no verdict/i)).toBeTruthy();
  });

  it("surfaces the triage reason, never the selection-stage Curator reason", () => {
    seedCurrentPhoto(1);
    // Both passes judged photo 1 with *contradictory* verdicts — this is
    // exactly the real-shoot situation that produced the bug.
    useProjectStore.setState({
      triageJudgments: new Map([
        [1, triageJudgment({ photoId: 1, reason: "Frame is technically usable." })],
      ]),
      curatorJudgments: new Map([[1, curatorJudgment({ photoId: 1 })]]),
    });

    render(<TriageNote />);

    expect(screen.getByText("Frame is technically usable.")).toBeTruthy();
    expect(screen.queryByText(/Closed eyes/)).toBeNull();
    expect(screen.queryByText(/Curator/)).toBeNull();
  });
});
