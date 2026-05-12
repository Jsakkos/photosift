import { describe, it, expect } from "vitest";
import { humanizeCuratorReason } from "../curatorText";

describe("humanizeCuratorReason", () => {
  const names: Record<number, string> = { 42: "DSC_0042.NEF", 7: "DSC_0007.NEF" };
  const lookup = (id: number) => names[id] ?? null;

  it("replaces a single [id] token with the filename", () => {
    expect(humanizeCuratorReason("[42] is the sharpest frame", lookup)).toBe(
      "DSC_0042.NEF is the sharpest frame",
    );
  });

  it("replaces multiple tokens", () => {
    expect(
      humanizeCuratorReason("Prefer [42] over [7]; both eyes open", lookup),
    ).toBe("Prefer DSC_0042.NEF over DSC_0007.NEF; both eyes open");
  });

  it("degrades unknown ids to (removed)", () => {
    expect(humanizeCuratorReason("[999] was deleted", lookup)).toBe(
      "(removed) was deleted",
    );
  });

  it("leaves text without tokens untouched", () => {
    expect(humanizeCuratorReason("Crisp, well-composed portrait", lookup)).toBe(
      "Crisp, well-composed portrait",
    );
  });

  it("ignores non-numeric bracket content", () => {
    expect(humanizeCuratorReason("looks good [imho]", lookup)).toBe(
      "looks good [imho]",
    );
  });
});
