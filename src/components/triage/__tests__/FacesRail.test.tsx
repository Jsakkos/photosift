/// Placement guard for the Triage tab's right rail. The bug: FacesRail
/// rendered the selection-stage `CuratorChip`, leaking an editorial
/// "is this a keeper?" verdict into the triage pass. FacesRail must show
/// the triage-stage `TriageNote` and never the Curator chip.

import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { FacesRail } from "../FacesRail";
import { useProjectStore } from "../../../stores/projectStore";
import { makeImage } from "../../../test/fixtures";
import { setupMockIpc } from "../../../test/mockIpc";
import type { CuratorJudgment, TriageJudgment } from "../../../types";

function seedCurrentPhoto(photoId: number) {
  const image = makeImage({ id: photoId });
  useProjectStore.setState({
    images: [image],
    displayItems: [{ imageIndex: 0, image }],
    currentIndex: 0,
    triageJudgments: new Map<number, TriageJudgment>(),
    curatorJudgments: new Map<number, CuratorJudgment>(),
  });
}

describe("FacesRail — Triage verdict placement", () => {
  it("renders the triage-stage TriageNote", () => {
    setupMockIpc();
    seedCurrentPhoto(1);

    render(<FacesRail />);

    expect(screen.getByLabelText(/^Triage AI/)).toBeTruthy();
  });

  it("never renders the selection-stage Curator chip", () => {
    setupMockIpc();
    seedCurrentPhoto(1);
    // A Curator verdict exists for this photo — it must still not surface
    // here; the Curator chip belongs to the Select tab.
    useProjectStore.setState({
      curatorJudgments: new Map([
        [
          1,
          {
            photoId: 1,
            shootId: 1,
            composition: 5,
            aesthetic: 5,
            clusterRank: null,
            isKeeper: false,
            suggestedFlag: "reject",
            reason: "Closed eyes.",
            userAction: null,
            judgedAt: "2026-05-16T11:00:00",
            provider: "anthropic",
            model: "claude-test",
            promptVersion: 1,
          },
        ],
      ]),
    });

    render(<FacesRail />);

    expect(screen.queryByLabelText(/^Curator/)).toBeNull();
  });
});
