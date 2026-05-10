import { describe, it, expect } from "vitest";
import { formatCuratorReason } from "../curatorReason";

describe("formatCuratorReason", () => {
  const idMap = new Map<number, string>([
    [42, "DSC_0042.NEF"],
    [43, "DSC_0043.NEF"],
    [100, "DSCF0100.RAF"],
  ]);

  it("returns the reason unchanged when no markers are present", () => {
    const reason = "Crisp focus on the eyes; expression is genuine.";
    expect(formatCuratorReason(reason, idMap)).toBe(reason);
  });

  it("substitutes a single known marker with the filename", () => {
    expect(formatCuratorReason("Photo [42] is the clear keeper.", idMap)).toBe(
      "Photo DSC_0042.NEF is the clear keeper.",
    );
  });

  it("substitutes multiple markers in one reason", () => {
    expect(
      formatCuratorReason(
        "Sharp eyes in [42] vs softer focus in [43].",
        idMap,
      ),
    ).toBe("Sharp eyes in DSC_0042.NEF vs softer focus in DSC_0043.NEF.");
  });

  it("leaves unknown markers alone (graceful fallback for stale judgments)", () => {
    expect(formatCuratorReason("Photo [99] missed the moment.", idMap)).toBe(
      "Photo [99] missed the moment.",
    );
  });

  it("does not match bare numbers (e.g. focal length, exposure)", () => {
    expect(
      formatCuratorReason("Sharp at 100mm f/2.8 ISO 400.", idMap),
    ).toBe("Sharp at 100mm f/2.8 ISO 400.");
  });

  it("does not match non-numeric brackets", () => {
    expect(
      formatCuratorReason("Composition follows the [rule of thirds].", idMap),
    ).toBe("Composition follows the [rule of thirds].");
  });

  it("returns reason unchanged when the id map is empty", () => {
    const reason = "Photo [42] is best.";
    expect(formatCuratorReason(reason, new Map())).toBe(reason);
  });

  it("substitutes inline forms with no surrounding spaces", () => {
    expect(formatCuratorReason("Compare [42]'s smile to [43]'s.", idMap)).toBe(
      "Compare DSC_0042.NEF's smile to DSC_0043.NEF's.",
    );
  });

  it("handles RAF and other extensions identically", () => {
    expect(formatCuratorReason("[100] has the strongest light.", idMap)).toBe(
      "DSCF0100.RAF has the strongest light.",
    );
  });

  it("mixed known + unknown leaves the unknowns intact", () => {
    expect(
      formatCuratorReason("Both [42] and [99] are usable, prefer [42].", idMap),
    ).toBe("Both DSC_0042.NEF and [99] are usable, prefer DSC_0042.NEF.");
  });
});
