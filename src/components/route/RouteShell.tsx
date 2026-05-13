import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, type PhotoDestination, type StarCount } from "../primitives";
import { RouteLightbox } from "./RouteLightbox";
import type { ImageEntry } from "../../types";

type DestinationId = "edit" | "export";

const PASS_TIERS: { floor: number; label: string }[] = [
  { floor: 0, label: "all" },
  { floor: 1, label: "★≥1" },
  { floor: 2, label: "★≥2" },
  { floor: 3, label: "★≥3" },
  { floor: 4, label: "★≥4" },
  { floor: 5, label: "★≥5" },
];

function PickCell({
  image,
  selected,
  onToggle,
  onOpen,
}: {
  image: ImageEntry;
  selected: boolean;
  onToggle: () => void;
  onOpen: () => void;
}) {
  const rating = Math.max(0, Math.min(5, image.starRating)) as StarCount;
  return (
    <Photo
      src={thumbUrl(image.id)}
      alt={image.filename}
      fit="cover"
      selected={selected}
      stars={rating}
      destination={image.destination === "unrouted" ? null : (image.destination as PhotoDestination)}
      onClick={onToggle}
      style={{ width: "100%", aspectRatio: "3 / 2", borderRadius: 2 }}
    >
      <button
        type="button"
        tabIndex={-1}
        onClick={(e) => {
          e.stopPropagation();
          onOpen();
        }}
        title="Open in sequential view"
        className="absolute top-1 right-1 rounded-xs w-[18px] h-[18px] flex items-center justify-center cursor-pointer border-0"
        style={{
          background: "rgba(0,0,0,0.55)",
          color: "var(--color-fg)",
          fontSize: 11,
          lineHeight: 1,
        }}
      >
        ⤢
      </button>
    </Photo>
  );
}

function ShipStrip({
  title,
  subtitle,
  count,
  accentTone,
  onOpenFolder,
  onCopyPath,
}: {
  title: string;
  subtitle: string;
  count: number;
  accentTone: string;
  onOpenFolder: () => void;
  onCopyPath: () => void;
}) {
  return (
    <div
      className="rounded-md p-[10px] border"
      style={{
        background: "var(--color-hover)",
        borderColor: "var(--color-border)",
      }}
    >
      <div className="flex items-baseline justify-between mb-[3px]">
        <span className="text-[12px] font-medium" style={{ color: accentTone }}>
          {title}
        </span>
        <span
          className="font-mono text-[10px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          {count} ready
        </span>
      </div>
      <div
        className="text-[10px] leading-[1.4] mb-[8px]"
        style={{ color: "var(--color-fg-dim)" }}
      >
        {subtitle}
      </div>
      <div className="flex gap-[6px]">
        <button
          type="button"
          tabIndex={-1}
          onClick={onOpenFolder}
          className="flex-1 px-[10px] py-[5px] rounded-xs text-[10px] border bg-transparent cursor-pointer"
          style={{ borderColor: "var(--color-border)", color: "var(--color-fg)" }}
        >
          Open folder
        </button>
        <button
          type="button"
          tabIndex={-1}
          onClick={onCopyPath}
          className="flex-1 px-[10px] py-[5px] rounded-xs text-[10px] border bg-transparent cursor-pointer"
          style={{ borderColor: "var(--color-border)", color: "var(--color-fg)" }}
        >
          Copy path
        </button>
      </div>
    </div>
  );
}

export function RouteShell() {
  const currentShoot = useProjectStore((s) => s.currentShoot);
  const images = useProjectStore((s) => s.images);
  const displayItems = useProjectStore((s) => s.displayItems);
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const setSelectMinStar = useProjectStore((s) => s.setSelectMinStar);
  const bulkSetDestination = useProjectStore((s) => s.bulkSetDestination);
  const setToast = useProjectStore((s) => s.setToast);
  const [destChoice, setDestChoice] = useState<DestinationId>("edit");
  const [lightboxPhotoId, setLightboxPhotoId] = useState<number | null>(null);

  const picks = useMemo(() => displayItems.map((d) => d.image), [displayItems]);

  // Local selection state. Drop entries that fall out of the current
  // filter so the Route bar's count stays consistent with what's visible.
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  useEffect(() => {
    setSelectedIds((prev) => {
      if (prev.size === 0) return prev;
      const visible = new Set(picks.map((p) => p.id));
      let changed = false;
      const next = new Set<number>();
      for (const id of prev) {
        if (visible.has(id)) next.add(id);
        else changed = true;
      }
      return changed ? next : prev;
    });
  }, [picks]);

  const counts = useMemo(() => {
    let captureOne = 0;
    let exportN = 0;
    let pending = 0;
    for (const img of images) {
      if (img.flag !== "pick" || img.starRating < selectMinStar) continue;
      if (img.destination === "edit") captureOne++;
      else if (img.destination === "export") exportN++;
      else pending++;
    }
    return { captureOne, export: exportN, pending };
  }, [images, selectMinStar]);

  const hasSelection = selectedIds.size > 0;
  const actionScopeCount = hasSelection ? selectedIds.size : picks.length;

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () =>
    setSelectedIds(new Set(picks.map((p) => p.id)));

  const clearSelection = () => setSelectedIds(new Set());

  // ⤢ on a pick tile opens a Route-local lightbox (embedded preview at
  // fit-screen). Route doesn't have a sequential/loupe view of its own
  // — that's Triage/Select — so we render a contained modal here
  // instead of trying to jam Route into the sequential mode shape.
  const openInLightbox = (id: number) => {
    setLightboxPhotoId(id);
  };
  const lightboxFilename = useMemo(
    () =>
      lightboxPhotoId === null
        ? undefined
        : displayItems.find((d) => d.image.id === lightboxPhotoId)?.image
            .filename,
    [lightboxPhotoId, displayItems],
  );

  const applyDestination = async (dest: DestinationId | "unrouted") => {
    const ids = hasSelection ? [...selectedIds] : picks.map((p) => p.id);
    if (ids.length === 0) {
      setToast("Nothing to route at this pass level", "error");
      return;
    }
    await bulkSetDestination(ids, dest);
    if (dest === "unrouted") {
      setToast(`Unrouted ${ids.length}`);
    } else {
      const label = dest === "edit" ? "Capture One" : "Export";
      setToast(`Routed ${ids.length} → ${label}`);
    }
    clearSelection();
  };

  const handleOpenFolder = async (bucket: DestinationId) => {
    if (!currentShoot) return;
    try {
      await invoke<string>("open_shoot_folder", {
        shootId: currentShoot.id,
        bucket,
      });
    } catch (err) {
      setToast(`Open failed: ${err}`, "error");
    }
  };

  const handleCopyPath = async (bucket: DestinationId) => {
    if (!currentShoot) return;
    try {
      const path = await invoke<string>("get_shoot_bucket_path", {
        shootId: currentShoot.id,
        bucket,
      });
      await navigator.clipboard.writeText(path);
      setToast(`Copied path: ${path}`);
    } catch (err) {
      setToast(`Copy failed: ${err}`, "error");
    }
  };

  const selectedFloorLabel =
    PASS_TIERS.find((t) => t.floor === selectMinStar)?.label ?? "all";

  return (
    <div
      data-testid="route-shell"
      className="flex-1 grid min-h-0"
      style={{ gridTemplateColumns: "1fr 320px" }}
    >
      <div className="flex flex-col min-h-0 p-4">
        <div className="flex items-baseline justify-between mb-[14px] gap-4">
          <div>
            <div
              className="text-[9px] uppercase tracking-[1.4px]"
              style={{ color: "var(--color-fg-dim)" }}
            >
              Route
            </div>
            <div
              className="text-[18px] font-semibold mt-[2px] flex items-baseline gap-2"
              style={{ color: "var(--color-fg)" }}
            >
              <span className="font-mono" style={{ color: "var(--color-accent)" }}>
                {selectedFloorLabel}
              </span>
              <span>· {picks.length} picks ready</span>
            </div>
          </div>
          <div
            className="inline-flex items-center gap-[1px] rounded-md p-[2px]"
            style={{ background: "var(--color-bg2)" }}
          >
            {PASS_TIERS.map((tier) => {
              const active = tier.floor === selectMinStar;
              return (
                <button
                  key={tier.floor}
                  type="button"
                  tabIndex={-1}
                  onClick={() => setSelectMinStar(tier.floor)}
                  className="px-[10px] py-[4px] rounded-xs font-mono text-[10px] border-0 cursor-pointer"
                  style={{
                    background: active ? "var(--color-accent)" : "transparent",
                    color: active ? "var(--color-on-accent)" : "var(--color-fg-dim)",
                    fontWeight: active ? 600 : 400,
                  }}
                  aria-pressed={active}
                >
                  {tier.label}
                </button>
              );
            })}
          </div>
        </div>

        <div
          className="flex items-center gap-[10px] mb-[10px] text-[11px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          <span className="font-mono text-[11px]" style={{ color: "var(--color-fg)" }}>
            {hasSelection
              ? `${selectedIds.size} selected`
              : `${picks.length} in view`}
          </span>
          <button
            type="button"
            tabIndex={-1}
            onClick={selectAll}
            disabled={picks.length === 0}
            className="px-[10px] py-[4px] rounded-xs text-[10px] border bg-transparent cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
            style={{ borderColor: "var(--color-border)", color: "var(--color-fg)" }}
          >
            Select all
          </button>
          <button
            type="button"
            tabIndex={-1}
            onClick={clearSelection}
            disabled={!hasSelection}
            className="px-[10px] py-[4px] rounded-xs text-[10px] border bg-transparent cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
            style={{ borderColor: "var(--color-border)", color: "var(--color-fg)" }}
          >
            Clear
          </button>
          <span className="text-[10px] opacity-70">
            · Click to toggle · ⤢ to open in loupe
          </span>
        </div>

        <div
          className="flex-1 overflow-auto grid gap-[10px] content-start"
          style={{ gridTemplateColumns: "repeat(5, 1fr)" }}
        >
          {picks.map((image) => (
            <PickCell
              key={image.id}
              image={image}
              selected={selectedIds.has(image.id)}
              onToggle={() => toggleSelect(image.id)}
              onOpen={() => openInLightbox(image.id)}
            />
          ))}
          {picks.length === 0 && (
            <div
              className="col-span-5 py-12 text-center text-[12px]"
              style={{ color: "var(--color-fg-mute)" }}
            >
              No picks at this pass level. Narrow with the pills or return to Select.
            </div>
          )}
        </div>

        <div
          className="mt-3 pt-[10px] px-3 flex gap-[14px] items-center border-t text-[11px]"
          style={{ borderColor: "var(--color-border)", color: "var(--color-fg-dim)" }}
        >
          <span className="font-mono text-[11px]">
            {counts.captureOne} → C1 · {counts.export} → Export · {counts.pending} pending
          </span>
        </div>
      </div>

      <div
        className="flex flex-col gap-[10px] p-4 border-l overflow-auto"
        style={{
          borderColor: "var(--color-border)",
          background: "var(--color-stage)",
        }}
      >
        <div
          className="text-[9px] uppercase tracking-[1.2px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          Route {hasSelection ? `${selectedIds.size} selected` : `all ${picks.length}`} to
        </div>

        <label
          className="flex flex-col gap-[5px] text-[11px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          <span>Destination</span>
          <select
            value={destChoice}
            onChange={(e) => setDestChoice(e.target.value as DestinationId)}
            className="px-[10px] py-[6px] rounded-xs text-[12px] border cursor-pointer"
            style={{
              background: "var(--color-hover)",
              borderColor: "var(--color-border)",
              color: "var(--color-fg)",
            }}
          >
            <option value="edit">Capture One</option>
            <option value="export">Export</option>
          </select>
        </label>

        <button
          type="button"
          onClick={() => void applyDestination(destChoice)}
          disabled={actionScopeCount === 0}
          className="px-[14px] py-[8px] rounded-md text-[12px] font-medium cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed border-0"
          style={{
            background: "var(--color-accent)",
            color: "var(--color-on-accent)",
          }}
        >
          Route {actionScopeCount} {actionScopeCount === 1 ? "photo" : "photos"}
        </button>

        <button
          type="button"
          onClick={() => void applyDestination("unrouted")}
          disabled={actionScopeCount === 0}
          className="px-[10px] py-[5px] rounded-xs text-[10px] border bg-transparent cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed self-start"
          style={{ borderColor: "var(--color-border)", color: "var(--color-fg-dim)" }}
        >
          Unroute
        </button>

        {counts.captureOne > 0 && (
          <ShipStrip
            title="Ship to Capture One"
            subtitle="Drag the folder onto Capture One, or use File → Import Images → Choose Folder."
            count={counts.captureOne}
            accentTone="var(--color-accent-2)"
            onOpenFolder={() => void handleOpenFolder("edit")}
            onCopyPath={() => void handleCopyPath("edit")}
          />
        )}

        {counts.export > 0 && (
          <ShipStrip
            title="Ship to Export"
            subtitle="Ready for direct publish. JPEG copy to Immich happens via Settings."
            count={counts.export}
            accentTone="var(--color-accent)"
            onOpenFolder={() => void handleOpenFolder("export")}
            onCopyPath={() => void handleCopyPath("export")}
          />
        )}
      </div>
      <RouteLightbox
        photoId={lightboxPhotoId}
        filename={lightboxFilename}
        onClose={() => setLightboxPhotoId(null)}
      />
    </div>
  );
}
