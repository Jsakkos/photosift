import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { FixedSizeGrid as Grid, GridChildComponentProps } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { AiPickBadge } from "./AiPickBadge";
import { Badge, VerdictBadge, type Verdict } from "./primitives";

const SIZES = [100, 160, 240] as const;
const CELL_GAP = 8;

export function GridView() {
  const {
    displayItems,
    setCurrentIndex,
    setFlag,
    setViewMode,
    currentView,
    createGroupFromPhotos,
    ungroupPhotos,
    setActiveInnerGroup,
  } = useProjectStore();
  const [colWidth, setColWidth] = useState<(typeof SIZES)[number]>(160);
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [focusIndex, setFocusIndex] = useState(0);
  const [dims, setDims] = useState({ width: 0, height: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<Grid>(null);
  const lastClickIdx = useRef(0);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => setDims({ width: el.clientWidth, height: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const columnWidth = colWidth + CELL_GAP;
  const rowHeight = Math.round(colWidth * 2 / 3) + CELL_GAP;
  const columnCount = Math.max(1, Math.floor(dims.width / columnWidth));
  const rowCount = Math.ceil(displayItems.length / columnCount);

  useEffect(() => {
    if (!gridRef.current || columnCount === 0) return;
    const rowIndex = Math.floor(focusIndex / columnCount);
    const columnIndex = focusIndex % columnCount;
    gridRef.current.scrollToItem({ rowIndex, columnIndex, align: "smart" });
  }, [focusIndex, columnCount]);

  // Ref so the keyboard effect can invoke the latest bulk handler
  // without pulling it into its dependency array (which would forward-
  // reference the useCallback decls below and cause a TDZ error).
  const bulkActionRef = useRef<((flag: string) => void) | null>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement
      )
        return;

      switch (e.key) {
        case "ArrowRight":
          e.preventDefault();
          setFocusIndex((i) => Math.min(i + 1, displayItems.length - 1));
          break;
        case "ArrowLeft":
          e.preventDefault();
          setFocusIndex((i) => Math.max(i - 1, 0));
          break;
        case "ArrowDown":
          e.preventDefault();
          setFocusIndex((i) => Math.min(i + columnCount, displayItems.length - 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setFocusIndex((i) => Math.max(i - columnCount, 0));
          break;
        case "Enter":
          e.preventDefault();
          setCurrentIndex(focusIndex);
          setViewMode("sequential");
          break;
        case "=":
        case "+": {
          const idx = SIZES.indexOf(colWidth);
          if (idx < SIZES.length - 1) setColWidth(SIZES[idx + 1]);
          break;
        }
        case "-": {
          const idx = SIZES.indexOf(colWidth);
          if (idx > 0) setColWidth(SIZES[idx - 1]);
          break;
        }
        // Bulk flag / destination shortcuts. Only fire when a real
        // selection exists; the single-item case is already handled by
        // the main useKeyboardNav hook in sequential view. Ctrl/Cmd
        // combos and modifier-only presses don't match these keys so we
        // don't stomp on save/export/etc.
        case "p":
        case "P":
          if (e.ctrlKey || e.metaKey || e.altKey) break;
          if (selection.size === 0) break;
          e.preventDefault();
          bulkActionRef.current?.("pick");
          break;
        case "x":
        case "X":
          if (e.ctrlKey || e.metaKey || e.altKey) break;
          if (selection.size === 0) break;
          e.preventDefault();
          bulkActionRef.current?.("reject");
          break;
        case "u":
        case "U":
          if (e.ctrlKey || e.metaKey || e.altKey) break;
          if (selection.size === 0) break;
          e.preventDefault();
          bulkActionRef.current?.("unreviewed");
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [colWidth, displayItems, focusIndex, columnCount, setCurrentIndex, setViewMode, currentView, setActiveInnerGroup, selection]);

  const handleClick = useCallback(
    (index: number, e: React.MouseEvent) => {
      if (e.shiftKey) {
        const start = Math.min(lastClickIdx.current, index);
        const end = Math.max(lastClickIdx.current, index);
        const newSel = new Set(selection);
        for (let i = start; i <= end; i++) newSel.add(i);
        setSelection(newSel);
      } else {
        setSelection(new Set([index]));
        lastClickIdx.current = index;
      }
      setFocusIndex(index);
    },
    [selection],
  );

  const handleBulkAction = useCallback(
    async (flag: string) => {
      const indices = selection.size > 0 ? [...selection] : [focusIndex];
      for (const idx of indices) {
        const item = displayItems[idx];
        if (item) {
          setCurrentIndex(idx);
          await setFlag(flag);
        }
      }
      setSelection(new Set());
    },
    [selection, focusIndex, displayItems, setCurrentIndex, setFlag],
  );

  // Keep the key-handler ref pointing at the latest useCallback so its
  // invocation inside the keydown effect always sees current state.
  bulkActionRef.current = handleBulkAction;

  const selectedPhotoIds = useMemo(
    () =>
      [...selection]
        .map((idx) => displayItems[idx]?.image.id)
        .filter((id): id is number => typeof id === "number"),
    [selection, displayItems],
  );

  const anySelectedInGroup = useMemo(
    () => [...selection].some((idx) => displayItems[idx]?.groupId !== undefined),
    [selection, displayItems],
  );

  const handleGroup = useCallback(async () => {
    if (selectedPhotoIds.length < 2) return;
    await createGroupFromPhotos(selectedPhotoIds);
    setSelection(new Set());
  }, [selectedPhotoIds, createGroupFromPhotos]);

  const handleUngroup = useCallback(async () => {
    if (selectedPhotoIds.length === 0) return;
    await ungroupPhotos(selectedPhotoIds);
    setSelection(new Set());
  }, [selectedPhotoIds, ungroupPhotos]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement
      )
        return;
      if (!e.ctrlKey && !e.metaKey) return;
      if (e.key === "g" || e.key === "G") {
        if (selectedPhotoIds.length === 0) return;
        e.preventDefault();
        if (e.shiftKey) {
          handleUngroup();
        } else if (selectedPhotoIds.length >= 2) {
          handleGroup();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectedPhotoIds, handleGroup, handleUngroup]);

  const Cell = useMemo(
    () =>
      ({ columnIndex, rowIndex, style }: GridChildComponentProps) => {
        const index = rowIndex * columnCount + columnIndex;
        if (index >= displayItems.length) return null;
        const item = displayItems[index];
        // An expanded-group member gets a shared background tint so
        // adjacent members read as belonging to one group; alternating
        // tints keep neighboring groups distinguishable. Triage is a
        // flat per-photo pass — group affiliation is not surfaced there.
        const isGroupMember =
          currentView === "select" && item.groupId !== undefined;
        const tintClass = isGroupMember
          ? item.groupId! % 2 === 0
            ? "bg-accent/[0.06]"
            : "bg-accent/[0.12]"
          : "";
        return (
          <div
            style={{ ...style, padding: CELL_GAP / 2 }}
            className={tintClass}
          >
            <GridThumb
              item={item}
              index={index}
              isFocused={index === focusIndex}
              isSelected={selection.has(index)}
              showGroupBar={isGroupMember}
              onClick={handleClick}
              onDoubleClick={() => {
                // In Select, double-clicking a grouped photo drills into
                // that group's inner strip; everything else opens loupe.
                if (currentView === "select" && item.groupId !== undefined) {
                  setActiveInnerGroup(item.groupId);
                } else {
                  setCurrentIndex(index);
                  setViewMode("sequential");
                }
              }}
              currentView={currentView}
            />
          </div>
        );
      },
    [
      columnCount,
      displayItems,
      focusIndex,
      selection,
      handleClick,
      setCurrentIndex,
      setViewMode,
      currentView,
      setActiveInnerGroup,
    ],
  );

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Grid chrome — stage-aware title + thumb size + count */}
      <div
        className="flex items-center justify-between gap-3 px-4 py-2.5 border-b text-[11px]"
        style={{
          borderColor: "var(--color-border)",
          background: "var(--color-bg)",
          color: "var(--color-fg-dim)",
        }}
      >
        <div className="flex items-center gap-3">
          <span className="font-mono text-2xs uppercase tracking-[1.2px]">
            {currentView === "triage"
              ? "Triage"
              : currentView === "select"
                ? "Select"
                : "Route"}
          </span>
          <span
            className="font-mono text-2xs tabular-nums"
            style={{ color: "var(--color-fg-mute)" }}
          >
            · grid · {displayItems.length} photo{displayItems.length === 1 ? "" : "s"}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => {
              const idx = SIZES.indexOf(colWidth);
              if (idx > 0) setColWidth(SIZES[idx - 1]);
            }}
            title="Shrink thumbnails (-)"
            aria-label="Shrink thumbnails"
            tabIndex={-1}
            className="w-6 h-6 flex items-center justify-center rounded-xs cursor-pointer"
            style={{
              background: "transparent",
              border: "1px solid var(--color-border)",
              color: "var(--color-fg-dim)",
            }}
          >
            −
          </button>
          <span className="font-mono text-2xs min-w-[50px] text-center">
            {colWidth === 100 ? "Small" : colWidth === 160 ? "Medium" : "Large"}
          </span>
          <button
            onClick={() => {
              const idx = SIZES.indexOf(colWidth);
              if (idx < SIZES.length - 1) setColWidth(SIZES[idx + 1]);
            }}
            title="Grow thumbnails (+)"
            aria-label="Grow thumbnails"
            tabIndex={-1}
            className="w-6 h-6 flex items-center justify-center rounded-xs cursor-pointer"
            style={{
              background: "transparent",
              border: "1px solid var(--color-border)",
              color: "var(--color-fg-dim)",
            }}
          >
            +
          </button>
        </div>
      </div>

      {/* Virtualized grid */}
      <div ref={containerRef} className="flex-1 overflow-hidden">
        {dims.width > 0 && dims.height > 0 && columnCount > 0 && (
          <Grid
            ref={gridRef}
            columnCount={columnCount}
            columnWidth={columnWidth}
            rowCount={rowCount}
            rowHeight={rowHeight}
            width={dims.width}
            height={dims.height}
            overscanRowCount={2}
          >
            {Cell}
          </Grid>
        )}
      </div>

      {/* Bulk action bar */}
      {selection.size > 0 && (
        <div
          className="flex items-center justify-between px-4 py-2 border-t text-sm"
          style={{
            background: "var(--color-bg2)",
            borderColor: "var(--color-accent-blue)",
          }}
        >
          <span
            className="font-mono text-[11px] font-medium"
            style={{ color: "var(--color-accent-blue)" }}
          >
            {selection.size} selected
          </span>
          <div className="flex gap-3">
            <button
              onClick={() => handleBulkAction("pick")}
              title="Pick selected (P)"
              className="px-3 py-1 rounded border border-success/40 text-success text-xs hover:bg-success/10"
            >
              P Pick
            </button>
            <button
              onClick={() => handleBulkAction("reject")}
              title="Reject selected (X)"
              className="px-3 py-1 rounded border border-danger/40 text-danger text-xs hover:bg-danger/10"
            >
              X Reject
            </button>
            <button
              onClick={() => handleBulkAction("unreviewed")}
              title="Reset to unreviewed (U)"
              className="px-3 py-1 rounded border border-white/20 text-fg-dim text-xs hover:bg-white/5"
            >
              U Reset
            </button>
            {selection.size >= 2 && (
              <button
                onClick={handleGroup}
                title="Group selected (Ctrl+G)"
                className="px-3 py-1 rounded border border-accent/40 text-accent text-xs hover:bg-accent/10"
              >
                Group
              </button>
            )}
            {anySelectedInGroup && (
              <button
                onClick={handleUngroup}
                title="Ungroup selected (Ctrl+Shift+G)"
                className="px-3 py-1 rounded border border-warning/40 text-warning text-xs hover:bg-warning/10"
              >
                Ungroup
              </button>
            )}
            <button
              onClick={() => {
                const idx = [...selection][0] ?? focusIndex;
                setCurrentIndex(idx);
                setViewMode("sequential");
              }}
              title="Open first selection in sequential view (Enter)"
              className="px-3 py-1 rounded border border-white/20 text-fg-dim text-xs hover:bg-white/5"
            >
              Enter → Loupe
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function GridThumb({
  item,
  index,
  isFocused,
  isSelected,
  showGroupBar,
  onClick,
  onDoubleClick,
  currentView,
}: {
  item: ReturnType<typeof useProjectStore.getState>["displayItems"][0];
  index: number;
  isFocused: boolean;
  isSelected: boolean;
  showGroupBar: boolean;
  onClick: (index: number, e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  currentView: string;
}) {
  const image = item.image;
  const isRejected = image.flag === "reject";
  const verdict: Verdict =
    image.flag === "pick" ? "keep" : image.flag === "reject" ? "toss" : null;

  const ariaLabel = [
    image.filename,
    image.flag !== "unreviewed" ? image.flag : null,
    image.destination !== "unrouted" ? image.destination.replace("_", " ") : null,
    image.starRating > 0 ? `${image.starRating} star${image.starRating === 1 ? "" : "s"}` : null,
  ]
    .filter(Boolean)
    .join(", ");

  // Focus/selection styling lives on `outline` (no layout shift) instead
  // of `border` (which used to push the image around when focus moved).
  // Focused: 2px outline + 3px offset; range-selected: 1px outline + a
  // subtle accent-blue wash so multi-select reads at a glance.
  const outline = isFocused
    ? "2px solid var(--color-accent-blue)"
    : isSelected
      ? "1px solid var(--color-accent-blue)"
      : undefined;
  const outlineOffset = isFocused ? 3 : isSelected ? 1 : undefined;

  return (
    <div
      role="button"
      tabIndex={isFocused ? 0 : -1}
      aria-label={ariaLabel}
      aria-pressed={isSelected}
      className={`relative w-full h-full rounded-xs overflow-hidden cursor-pointer transition-[background,opacity] ${
        isSelected ? "bg-accent-blue/[0.12]" : ""
      } ${isRejected ? "opacity-35" : ""}`}
      style={{ outline, outlineOffset }}
      onClick={(e) => onClick(index, e)}
      onDoubleClick={onDoubleClick}
    >
      <img
        key={image.id}
        src={thumbUrl(image.id)}
        alt={image.filename}
        className={`w-full h-full object-cover ${isRejected ? "grayscale-[0.6]" : ""}`}
        loading="lazy"
        draggable={false}
      />
      <VerdictBadge verdict={verdict} />
      {/* Expanded-group affiliation bar — left-edge accent visible inside
          the rounded clip. Matches the Filmstrip treatment so switching
          views preserves the visual cue. */}
      {showGroupBar && (
        <div
          className="absolute left-0 top-0 bottom-0 w-[3px] bg-accent pointer-events-none"
          aria-hidden="true"
        />
      )}
      {/* Top-right stack: verdict → AI pick → destination. Verdict owns
          top-1 (its 12px square clears the 4px corner). AI pick (suppressed
          in Select since every photo there is already a pick) drops below
          it when verdict is shown. Destination chip drops below whichever
          predecessors are present. */}
      {item.isAiPick && currentView !== "select" && (
        <AiPickBadge
          pos={undefined}
          className={`absolute right-1 ${verdict !== null ? "top-6" : "top-1"}`}
        />
      )}
      {(image.destination === "edit" || image.destination === "export") && (
        <Badge
          tone={image.destination === "edit" ? "accent-2" : "accent"}
          variant="glass"
          className={`absolute right-1 ${
            verdict !== null && item.isAiPick && currentView !== "select"
              ? "top-12"
              : verdict !== null || (item.isAiPick && currentView !== "select")
                ? "top-7"
                : "top-1"
          } font-semibold pointer-events-none`}
          title={
            image.destination === "edit"
              ? "Route: Capture One\nReady to drag into Capture One (or DxO)."
              : "Route: Export\nCached JPEG copied to Immich ingest folder by the Publish button."
          }
          aria-label={
            image.destination === "edit" ? "Route: Capture One" : "Route: Export"
          }
        >
          {image.destination === "edit" ? "→ C1" : "→ Exp"}
        </Badge>
      )}
      {/* Filename on hover */}
      <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent pt-4 pb-1 px-1.5 opacity-0 hover:opacity-100 transition-opacity duration-fast">
        <span className="text-2xs text-white/80 truncate block">
          {image.filename}
        </span>
      </div>
    </div>
  );
}
