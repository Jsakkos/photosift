import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { FixedSizeGrid as Grid, GridChildComponentProps } from "react-window";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";
import { AiPickBadge } from "./AiPickBadge";

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
          {
            const focused = displayItems[focusIndex];
            if (
              focused?.isGroupCover &&
              focused.groupId !== undefined &&
              (currentView === "triage" || currentView === "select")
            ) {
              setActiveInnerGroup(focused.groupId);
            } else {
              setCurrentIndex(focusIndex);
              setViewMode("sequential");
            }
          }
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
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [colWidth, displayItems, focusIndex, columnCount, setCurrentIndex, setViewMode, currentView, setActiveInnerGroup]);

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
        // An expanded-group member: has a groupId but isn't the cover.
        // These get a shared background tint so adjacent members read as
        // belonging to one group; alternating tints keep neighboring
        // groups distinguishable.
        const isGroupMember =
          item.groupId !== undefined && !item.isGroupCover;
        const tintClass = isGroupMember
          ? item.groupId! % 2 === 0
            ? "bg-[var(--accent)]/[0.06]"
            : "bg-[var(--accent)]/[0.12]"
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
              isMulti={selection.size > 1}
              showGroupBar={isGroupMember}
              onClick={handleClick}
              onDoubleClick={() => {
                // Symmetric toggle: clicking a collapsed group cover
                // expands it; clicking any expanded-group member
                // collapses it back. Non-group photos open loupe.
                const inExpandableView =
                  currentView === "triage" || currentView === "select";
                const isExpandedMember =
                  inExpandableView &&
                  item.groupId !== undefined &&
                  !item.isGroupCover;
                if (
                  item.isGroupCover &&
                  item.groupId !== undefined &&
                  inExpandableView
                ) {
                  setActiveInnerGroup(item.groupId);
                } else if (isExpandedMember && item.groupId !== undefined) {
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
      {/* Size controls */}
      <div className="flex items-center justify-end gap-2 px-3 py-1.5 bg-[var(--bg-secondary)] border-b border-white/5 text-xs text-[var(--text-secondary)]">
        <button
          onClick={() => {
            const idx = SIZES.indexOf(colWidth);
            if (idx > 0) setColWidth(SIZES[idx - 1]);
          }}
          title="Shrink thumbnails (-)"
          aria-label="Shrink thumbnails"
          className="w-6 h-6 flex items-center justify-center rounded bg-[var(--bg-tertiary)] border border-white/10 hover:border-white/20"
        >
          −
        </button>
        <span>{colWidth === 100 ? "Small" : colWidth === 160 ? "Medium" : "Large"}</span>
        <button
          onClick={() => {
            const idx = SIZES.indexOf(colWidth);
            if (idx < SIZES.length - 1) setColWidth(SIZES[idx + 1]);
          }}
          title="Grow thumbnails (+)"
          aria-label="Grow thumbnails"
          className="w-6 h-6 flex items-center justify-center rounded bg-[var(--bg-tertiary)] border border-white/10 hover:border-white/20"
        >
          +
        </button>
        <span className="ml-4">{displayItems.length} photos</span>
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
        <div className="flex items-center justify-between px-4 py-2 bg-[#1a1a2e] border-t border-[var(--accent)]/30 text-sm">
          <span className="text-[var(--accent)] font-medium">
            {selection.size} selected
          </span>
          <div className="flex gap-3">
            <button
              onClick={() => handleBulkAction("pick")}
              title="Pick selected (P)"
              className="px-3 py-1 rounded border border-green-500/40 text-green-500 text-xs hover:bg-green-500/10"
            >
              P Pick
            </button>
            <button
              onClick={() => handleBulkAction("reject")}
              title="Reject selected (X)"
              className="px-3 py-1 rounded border border-red-500/40 text-red-500 text-xs hover:bg-red-500/10"
            >
              X Reject
            </button>
            <button
              onClick={() => handleBulkAction("unreviewed")}
              title="Reset to unreviewed (U)"
              className="px-3 py-1 rounded border border-white/20 text-[var(--text-secondary)] text-xs hover:bg-white/5"
            >
              U Reset
            </button>
            {selection.size >= 2 && (
              <button
                onClick={handleGroup}
                title="Group selected (Ctrl+G)"
                className="px-3 py-1 rounded border border-[var(--accent)]/40 text-[var(--accent)] text-xs hover:bg-[var(--accent)]/10"
              >
                Group
              </button>
            )}
            {anySelectedInGroup && (
              <button
                onClick={handleUngroup}
                title="Ungroup selected (Ctrl+Shift+G)"
                className="px-3 py-1 rounded border border-orange-400/40 text-orange-400 text-xs hover:bg-orange-400/10"
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
              className="px-3 py-1 rounded border border-white/20 text-[var(--text-secondary)] text-xs hover:bg-white/5"
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
  isMulti,
  showGroupBar,
  onClick,
  onDoubleClick,
  currentView: _currentView,
}: {
  item: ReturnType<typeof useProjectStore.getState>["displayItems"][0];
  index: number;
  isFocused: boolean;
  isSelected: boolean;
  isMulti: boolean;
  showGroupBar: boolean;
  onClick: (index: number, e: React.MouseEvent) => void;
  onDoubleClick: () => void;
  currentView: string;
}) {
  const [loaded, setLoaded] = useState(false);
  const image = item.image;
  const isRejected = image.flag === "reject";

  const ariaLabel = [
    image.filename,
    image.flag !== "unreviewed" ? image.flag : null,
    image.destination !== "unrouted" ? image.destination.replace("_", " ") : null,
    image.starRating > 0 ? `${image.starRating} star${image.starRating === 1 ? "" : "s"}` : null,
    item.isGroupCover && item.groupMemberCount
      ? `group of ${item.groupMemberCount}`
      : null,
  ]
    .filter(Boolean)
    .join(", ");

  return (
    <div
      role="button"
      tabIndex={isFocused ? 0 : -1}
      aria-label={ariaLabel}
      aria-pressed={isSelected}
      className={`relative w-full h-full rounded overflow-hidden cursor-pointer border-2 transition-all ${
        isSelected
          ? isMulti
            ? "border-purple-500 shadow-[0_0_0_1px_rgb(168,85,247)]"
            : "border-[var(--accent)] shadow-[0_0_0_1px_var(--accent)]"
          : isFocused
            ? "border-[var(--accent)]/50"
            : "border-transparent hover:border-white/20"
      } ${isRejected ? "opacity-35" : ""}`}
      onClick={(e) => onClick(index, e)}
      onDoubleClick={onDoubleClick}
    >
      <img
        src={thumbUrl(image.id)}
        alt={image.filename}
        className={`w-full h-full object-cover ${loaded ? "opacity-100" : "opacity-30"} ${isRejected ? "grayscale-[0.6]" : ""}`}
        loading="lazy"
        draggable={false}
        onLoad={(e) => {
          if (e.currentTarget.naturalWidth > 1) setLoaded(true);
        }}
      />
      {/* Expanded-group affiliation bar — left-edge accent visible inside
          the rounded clip. Matches the Filmstrip treatment so switching
          views preserves the visual cue. */}
      {showGroupBar && (
        <div
          className="absolute left-0 top-0 bottom-0 w-[3px] bg-[var(--accent)] pointer-events-none"
          aria-hidden="true"
        />
      )}
      {/* Flag dot */}
      {image.flag === "pick" && (
        <div className="absolute top-1.5 left-1.5 w-2.5 h-2.5 rounded-full bg-green-500 shadow-[0_0_4px_rgba(34,197,94,0.5)]" />
      )}
      {image.flag === "reject" && (
        <div className="absolute top-1.5 left-1.5 w-2.5 h-2.5 rounded-full bg-red-500 shadow-[0_0_4px_rgba(239,68,68,0.5)]" />
      )}
      {/* AI pick badge */}
      {item.isAiPick && <AiPickBadge />}
      {/* Destination badge */}
      {image.destination === "edit" && (
        <div className={`absolute ${item.isAiPick ? "top-7" : "top-1.5"} right-1.5 text-[9px] font-semibold px-1.5 py-0.5 rounded bg-purple-500/25 text-purple-300 border border-purple-500/30`}>
          EDIT
        </div>
      )}
      {image.destination === "publish_direct" && (
        <div className={`absolute ${item.isAiPick ? "top-7" : "top-1.5"} right-1.5 text-[9px] font-semibold px-1.5 py-0.5 rounded bg-[var(--accent)]/25 text-blue-300 border border-[var(--accent)]/30`}>
          PUBLISH
        </div>
      )}
      {/* Group stack indicator */}
      {item.isGroupCover && item.groupMemberCount && (
        <div className="absolute bottom-1.5 right-1.5 bg-black/70 text-blue-300 text-[10px] font-semibold px-1.5 py-0.5 rounded backdrop-blur-sm">
          +{item.groupMemberCount - 1}
        </div>
      )}
      {/* Filename on hover */}
      <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent pt-4 pb-1 px-1.5 opacity-0 hover:opacity-100 transition-opacity">
        <span className="text-[10px] text-white/80 truncate block">
          {image.filename}
        </span>
      </div>
    </div>
  );
}
