/// Placement guard for the Select tab's detail rail. The editorial
/// selection-stage Curator verdict ("is this a keeper?") belongs here —
/// it was moved out of the Triage tab, where it contradicted the
/// triage pass. DetailRail must render the `CuratorChip`.

import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { DetailRail } from "../DetailRail";
import { useProjectStore } from "../../../stores/projectStore";
import { makeImage } from "../../../test/fixtures";
import { setupMockIpc } from "../../../test/mockIpc";
import type { CuratorJudgment } from "../../../types";

function seedCurrentPhoto(photoId: number) {
  const image = makeImage({ id: photoId, flag: "pick" });
  useProjectStore.setState({
    images: [image],
    displayItems: [{ imageIndex: 0, image }],
    currentIndex: 0,
    curatorJudgments: new Map<number, CuratorJudgment>(),
  });
}

describe("DetailRail — Select Curator placement", () => {
  it("renders the selection-stage Curator chip", () => {
    setupMockIpc();
    seedCurrentPhoto(1);

    render(<DetailRail />);

    expect(screen.getByLabelText(/^Curator/)).toBeTruthy();
  });

  it("surfaces the Curator verdict reason when one exists", () => {
    setupMockIpc();
    seedCurrentPhoto(1);
    useProjectStore.setState({
      curatorJudgments: new Map([
        [
          1,
          {
            photoId: 1,
            shootId: 1,
            composition: 8,
            aesthetic: 7,
            clusterRank: null,
            isKeeper: true,
            suggestedFlag: "pick",
            reason: "Strongest expression of the set.",
            userAction: null,
            judgedAt: "2026-05-16T11:00:00",
            provider: "anthropic",
            model: "claude-test",
            promptVersion: 1,
          },
        ],
      ]),
    });

    render(<DetailRail />);

    expect(screen.getByText("Strongest expression of the set.")).toBeTruthy();
  });
});
