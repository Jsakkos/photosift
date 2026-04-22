import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useProjectStore } from "../../stores/projectStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import { Photo, Kbd } from "../primitives";
import type { ImageEntry } from "../../types";

type DestinationId = "capture_one" | "dxo" | "publish_direct";

const PASS_TIERS: { floor: number; label: string }[] = [
  { floor: 0, label: "all" },
  { floor: 1, label: "★≥1" },
  { floor: 2, label: "★≥2" },
  { floor: 3, label: "★≥3" },
  { floor: 4, label: "★≥4" },
  { floor: 5, label: "★≥5" },
];

function destTag(image: ImageEntry): { label: string; tone: string } | null {
  if (image.destination === "edit") return { label: "C1", tone: "var(--color-accent-2)" };
  if (image.destination === "publish_direct") return { label: "Pub", tone: "var(--color-accent)" };
  return null;
}

function PassPills() {
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const setSelectMinStar = useProjectStore((s) => s.setSelectMinStar);

  return (
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
              color: active ? "#1a1a1a" : "var(--color-fg-dim)",
              fontWeight: active ? 600 : 400,
            }}
            aria-pressed={active}
          >
            {tier.label}
          </button>
        );
      })}
    </div>
  );
}

function Stars9({ n }: { n: number }) {
  return (
    <span className="font-mono text-[9px]" style={{ color: "var(--color-warning)" }}>
      {"★".repeat(Math.max(0, Math.min(5, n)))}
      {n > 0 && n < 5 ? "" : ""}
    </span>
  );
}

function PickCell({ image }: { image: ImageEntry }) {
  const tag = destTag(image);
  const rating = Math.max(0, Math.min(5, image.starRating));
  return (
    <div className="relative">
      <Photo
        src={thumbUrl(image.id)}
        alt={image.filename}
        fit="cover"
        style={{ width: "100%", height: 110, borderRadius: 2 }}
      />
      <div
        className="absolute top-[4px] left-[4px] rounded-xs px-[5px] py-[2px]"
        style={{ background: "rgba(0,0,0,0.6)" }}
      >
        <Stars9 n={rating} />
      </div>
      {tag && (
        <div
          className="absolute bottom-[4px] right-[4px] rounded-xs px-[5px] py-[2px] font-mono text-[9px]"
          style={{ background: "rgba(0,0,0,0.7)", color: tag.tone }}
        >
          → {tag.label}
        </div>
      )}
    </div>
  );
}

function DestCard({
  name,
  sub,
  kbds,
  count,
  disabled = false,
  onClick,
}: {
  name: string;
  sub: string;
  kbds: string[];
  count: number;
  disabled?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      disabled={disabled}
      onClick={onClick}
      className="w-full text-left p-[10px] rounded-md cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 border bg-transparent"
      style={{
        background: "var(--color-hover)",
        borderColor: "var(--color-border)",
      }}
    >
      <div className="flex items-center justify-between mb-[3px]">
        <span className="text-[12px] font-medium" style={{ color: "var(--color-fg)" }}>
          {name}
        </span>
        <div className="flex gap-[3px]">
          {kbds.map((k, i) => (
            <Kbd key={i}>{k}</Kbd>
          ))}
        </div>
      </div>
      <div className="text-[10px]" style={{ color: "var(--color-fg-dim)" }}>
        {sub}
      </div>
      <div
        className="font-mono text-[9px] mt-[3px]"
        style={{ color: "var(--color-accent)" }}
      >
        {count} routed
      </div>
    </button>
  );
}

export function RouteShell() {
  const currentShoot = useProjectStore((s) => s.currentShoot);
  const images = useProjectStore((s) => s.images);
  const displayItems = useProjectStore((s) => s.displayItems);
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const setDestination = useProjectStore((s) => s.setDestination);
  const setToast = useProjectStore((s) => s.setToast);
  const [exporting, setExporting] = useState(false);

  const picks = useMemo(() => displayItems.map((d) => d.image), [displayItems]);

  const counts = useMemo(() => {
    const pickImages = images.filter(
      (img) => img.flag === "pick" && img.starRating >= selectMinStar,
    );
    let captureOne = 0;
    let publish = 0;
    let pending = 0;
    for (const img of pickImages) {
      if (img.destination === "edit") captureOne++;
      else if (img.destination === "publish_direct") publish++;
      else pending++;
    }
    return { captureOne, publish, pending, total: pickImages.length };
  }, [images, selectMinStar]);

  const handleExport = async () => {
    if (!currentShoot) return;
    setExporting(true);
    try {
      const count = await invoke<number>("export_xmp", {
        shootId: currentShoot.id,
        filter: "picks",
      });
      setToast(`Exported ${count} XMP sidecar${count === 1 ? "" : "s"}`);
    } catch (err) {
      setToast(`Export failed: ${err}`, "error");
    } finally {
      setExporting(false);
    }
  };

  const handleRouteAll = async (dest: DestinationId) => {
    const routeValue = dest === "publish_direct" ? "publish_direct" : "edit";
    const pending = picks.filter((img) => img.destination !== routeValue);
    for (const img of pending) {
      try {
        await invoke("set_destination", { photoId: img.id, destination: routeValue });
      } catch {
        /* ignore per-photo errors; toast at end */
      }
    }
    // Trigger display recompute by calling the action through the store
    // (setDestination also persists the last clicked + updates local state).
    if (pending[0]) {
      try {
        await setDestination(routeValue);
      } catch {
        /* ignore */
      }
    }
    setToast(`Routed ${pending.length} to ${dest === "publish_direct" ? "Publish Direct" : "Capture One"}`);
  };

  const selectedFloorLabel =
    PASS_TIERS.find((t) => t.floor === selectMinStar)?.label ?? "all";

  return (
    <div className="flex-1 grid min-h-0" style={{ gridTemplateColumns: "1fr 300px" }}>
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
          <PassPills />
        </div>

        <div
          className="flex-1 overflow-auto grid gap-[10px] content-start"
          style={{ gridTemplateColumns: "repeat(5, 1fr)" }}
        >
          {picks.map((image) => (
            <PickCell key={image.id} image={image} />
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
          <span className="inline-flex items-center gap-[6px]">
            <Kbd>E</Kbd>
            Capture One
          </span>
          <span className="inline-flex items-center gap-[6px]">
            <Kbd>D</Kbd>
            publish
          </span>
          <span className="inline-flex items-center gap-[6px]">
            <Kbd>⌘</Kbd>
            <Kbd>E</Kbd>
            export XMP
          </span>
          <div className="flex-1" />
          <span className="font-mono text-[11px]">
            {counts.captureOne + counts.publish} routed · {counts.pending} pending
          </span>
        </div>
      </div>

      <div
        className="flex flex-col gap-[10px] p-4 border-l"
        style={{
          borderColor: "var(--color-border)",
          background: "#111",
        }}
      >
        <div
          className="text-[9px] uppercase tracking-[1.2px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          Destinations
        </div>
        <DestCard
          name="Capture One Pro"
          sub="opens selected RAWs · reads XMP"
          kbds={["E"]}
          count={counts.captureOne}
          onClick={() => void handleRouteAll("capture_one")}
        />
        <DestCard
          name="DxO PhotoLab"
          sub="post-MVP · open source in DxO"
          kbds={["⌘", "E"]}
          count={0}
          disabled
        />
        <DestCard
          name="Publish direct"
          sub="cached JPEG → Immich ingest folder"
          kbds={["D"]}
          count={counts.publish}
          onClick={() => void handleRouteAll("publish_direct")}
        />

        <div
          className="text-[9px] uppercase tracking-[1.2px] mt-[6px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          XMP sidecars
        </div>
        <div
          className="rounded-sm p-[10px] font-mono text-[10px] leading-[1.5]"
          style={{ background: "var(--color-hover)", color: "var(--color-fg-dim)" }}
        >
          <div style={{ color: "var(--color-fg)" }}>
            {counts.total} picks · ratings + labels
          </div>
          <div>writes beside each RAW</div>
          <div>
            filter · picks {selectedFloorLabel}
          </div>
        </div>

        <button
          type="button"
          onClick={() => void handleExport()}
          disabled={exporting || !currentShoot}
          className="mt-auto px-[14px] py-[8px] rounded-md text-[12px] font-medium cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          style={{
            background: "var(--color-accent-blue)",
            color: "#fff",
            border: "none",
          }}
        >
          {exporting ? "Exporting…" : "Export XMP sidecars"}
        </button>
      </div>
    </div>
  );
}
