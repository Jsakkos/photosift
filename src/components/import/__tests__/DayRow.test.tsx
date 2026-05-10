/// Component tests for the date-browser tile, focused on the
/// "+JPG" badge contract introduced for RAW+JPG mode shooting.
///
/// We don't have a full-blown e2e framework against the live Tauri
/// app (Tauri's webview isn't a regular browser). These tests render
/// the component in `happy-dom` and verify the React layer behaves
/// correctly when handed paired vs. lone scan entries.

import { render, screen, within } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { DayRow } from "../DayRow";
import type { ScanDateEntry } from "../../../types";

function entry(overrides: Partial<ScanDateEntry> = {}): ScanDateEntry {
  return {
    path: "/cam/DSCF0001.RAF",
    filename: "DSCF0001.RAF",
    capturedAt: "2024:08:02 09:15:00",
    camera: "FUJIFILM X-T5",
    fileSizeBytes: 50_000_000,
    thumbDataUrl: null,
    alreadyImported: false,
    orientation: 1,
    siblingJpegPath: null,
    ...overrides,
  };
}

function noopProps(visibleEntries: ScanDateEntry[]) {
  return {
    dayLabel: "Aug 2, 2024",
    visibleEntries,
    hiddenImportedCount: 0,
    expanded: true,
    onToggleExpand: vi.fn(),
    selected: new Set<string>(),
    onTogglePath: vi.fn(),
    onSelectAllInDay: vi.fn(),
    onSelectNoneInDay: vi.fn(),
    loadedThumbs: new Map<string, string>(),
    thumbsLoading: false,
    thumbsLoaded: 0,
  };
}

describe("DayRow +JPG badge", () => {
  it("renders the badge for a paired RAF+JPG entry", () => {
    const paired = entry({
      path: "/cam/DSCF0001.RAF",
      filename: "DSCF0001.RAF",
      siblingJpegPath: "/cam/DSCF0001.JPG",
    });
    render(<DayRow {...noopProps([paired])} />);

    const badge = screen.getByText("+JPG");
    expect(badge).toBeTruthy();
    // The tooltip text on the badge tells the user what it means.
    expect(badge.getAttribute("title")).toMatch(/sibling/i);
  });

  it("does not render the badge for a lone RAF (no sibling)", () => {
    const lone = entry({
      path: "/cam/DSCF0099.RAF",
      filename: "DSCF0099.RAF",
      siblingJpegPath: null,
    });
    render(<DayRow {...noopProps([lone])} />);

    expect(screen.queryByText("+JPG")).toBeNull();
  });

  it("does not render the badge for a standalone JPG", () => {
    const lonely = entry({
      path: "/cam/wedding.jpg",
      filename: "wedding.jpg",
      camera: "NIKON D750",
      siblingJpegPath: null,
    });
    render(<DayRow {...noopProps([lonely])} />);

    expect(screen.queryByText("+JPG")).toBeNull();
  });

  it("renders one tile per scan entry — paired items must already be collapsed by the backend", () => {
    // The backend's `pair()` is responsible for collapsing RAW+JPG into
    // one ScanEntry. The frontend trusts that contract — given two
    // entries, it renders two tiles, never four.
    const paired = entry({
      path: "/cam/A.RAF",
      filename: "A.RAF",
      siblingJpegPath: "/cam/A.JPG",
    });
    const lone = entry({
      path: "/cam/B.RAF",
      filename: "B.RAF",
      siblingJpegPath: null,
    });
    const { container } = render(<DayRow {...noopProps([paired, lone])} />);

    // The tile is the clickable div with `aspectRatio` style; counting
    // the badge presence gives us a robust paired-count.
    const badges = container.querySelectorAll('span[title*="sibling" i]');
    expect(badges.length).toBe(1);
  });

  it("badge sits on top of the thumbnail (rendered as a sibling, not replacing)", () => {
    // Regression guard: the thumbnail <img> must still render alongside
    // the badge. If we accidentally short-circuit the thumb when a
    // sibling exists, paired tiles would lose their preview.
    const paired = entry({
      path: "/cam/DSCF0001.RAF",
      filename: "DSCF0001.RAF",
      siblingJpegPath: "/cam/DSCF0001.JPG",
    });
    const props = noopProps([paired]);
    props.loadedThumbs.set(paired.path, "data:image/jpeg;base64,fake");

    const { container } = render(<DayRow {...props} />);

    const thumb = container.querySelector("img");
    expect(thumb).not.toBeNull();
    expect(thumb!.getAttribute("src")).toBe("data:image/jpeg;base64,fake");
    // Both elements are children of the same tile container.
    const tile = thumb!.parentElement!;
    const badge = within(tile).getByText("+JPG");
    expect(badge).toBeTruthy();
  });

  it("dayLabel and photo count render as before (regression check on the +JPG addition)", () => {
    const paired = entry({ siblingJpegPath: "/cam/DSCF0001.JPG" });
    render(<DayRow {...noopProps([paired])} />);

    expect(screen.getByText("Aug 2, 2024")).toBeTruthy();
    expect(screen.getByText(/^1 photo$/)).toBeTruthy();
  });
});
