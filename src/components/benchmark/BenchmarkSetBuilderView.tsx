import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useShootListStore } from "../../stores/shootListStore";
import { useBenchmarkStore, emptyPhotoRecord } from "../../stores/benchmarkStore";
import { thumbUrl } from "../../hooks/useImageLoader";
import type { ImageEntry, ShootSummary } from "../../types";

interface Props {
  onCancel: () => void;
  onCreated: () => void;
}

interface SelectedPhoto {
  photoId: number;
  shootId: number;
  cameraModel: string | null;
  filename: string;
}

/// Multi-shoot set builder: pick a shoot, multi-select its photos, add
/// to the set, repeat across shoots. The set is materialized when the
/// user names it and clicks "Create" — until then it's purely local
/// state.
export function BenchmarkSetBuilderView({ onCancel, onCreated }: Props) {
  const shoots = useShootListStore((s) => s.shoots);
  const refreshShoots = useShootListStore((s) => s.refresh);
  const createSet = useBenchmarkStore((s) => s.createSet);
  const saveError = useBenchmarkStore((s) => s.saveError);

  const [shootId, setShootId] = useState<number | null>(null);
  const [shootPhotos, setShootPhotos] = useState<ImageEntry[]>([]);
  const [shootLoading, setShootLoading] = useState(false);
  const [shootError, setShootError] = useState<string | null>(null);
  const [selected, setSelected] = useState<SelectedPhoto[]>([]);
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    if (shoots.length === 0) void refreshShoots();
  }, [shoots.length, refreshShoots]);

  useEffect(() => {
    if (shootId === null) {
      setShootPhotos([]);
      return;
    }
    let cancelled = false;
    setShootLoading(true);
    setShootError(null);
    // get_shoot loads the shoot into AppState, then get_image_list reads
    // the loaded shoot's photos. CullPage uses the same sequence.
    invoke<ShootSummary>("get_shoot", { shootId })
      .then(() => invoke<ImageEntry[]>("get_image_list"))
      .then((photos) => {
        if (cancelled) return;
        setShootPhotos(photos);
        setShootLoading(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setShootError(String(e));
        setShootLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [shootId]);

  const selectedIds = useMemo(
    () => new Set(selected.map((p) => p.photoId)),
    [selected],
  );

  const toggleSelect = (image: ImageEntry) => {
    if (shootId === null) return;
    setSelected((prev) => {
      const exists = prev.some((p) => p.photoId === image.id);
      if (exists) return prev.filter((p) => p.photoId !== image.id);
      return [
        ...prev,
        {
          photoId: image.id,
          shootId,
          cameraModel: image.cameraModel,
          filename: image.filename,
        },
      ];
    });
  };

  const handleCreate = async () => {
    if (selected.length === 0) return;
    setCreating(true);
    const photoRecords = selected.map((p) =>
      emptyPhotoRecord(p.photoId, p.shootId, p.cameraModel),
    );
    const created = await createSet(name, photoRecords);
    setCreating(false);
    if (created) onCreated();
  };

  return (
    <div className="flex flex-col h-full">
      <div
        className="flex items-center justify-between px-4 py-2 border-b shrink-0"
        style={{ borderColor: "var(--color-border)", background: "var(--color-bg)" }}
      >
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={onCancel}
            className="text-[11px] font-mono"
            style={{ color: "var(--color-fg-mute)" }}
          >
            ← Back
          </button>
          <span className="text-[12px] font-medium" style={{ color: "var(--color-fg)" }}>
            Build a benchmark set
          </span>
          <span className="text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
            {selected.length} selected
          </span>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Set name (e.g. d750-smoke-test-01)"
            className="px-2 py-1 text-[12px] rounded-sm"
            style={{
              background: "var(--color-bg2)",
              color: "var(--color-fg)",
              border: "1px solid var(--color-border)",
              minWidth: 260,
            }}
          />
          <button
            type="button"
            onClick={handleCreate}
            disabled={selected.length === 0 || creating}
            className="px-3 py-1 rounded-md text-xs font-medium disabled:opacity-40"
            style={{
              background: "var(--color-accent)",
              color: "var(--color-bg)",
            }}
          >
            {creating ? "Creating…" : "Create set"}
          </button>
        </div>
      </div>

      {saveError && (
        <div
          className="px-4 py-2 text-[11px] border-b shrink-0"
          style={{
            background: "var(--color-bg2)",
            color: "var(--color-danger)",
            borderColor: "var(--color-border)",
          }}
        >
          {saveError}
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        <aside
          className="shrink-0 overflow-y-auto"
          style={{
            width: 260,
            background: "var(--color-bg2)",
            borderRight: "1px solid var(--color-border)",
          }}
        >
          <div
            className="px-3 py-2 border-b text-[11px] uppercase tracking-[0.5px]"
            style={{ color: "var(--color-fg-mute)", borderColor: "var(--color-border)" }}
          >
            Shoots
          </div>
          <ul>
            {shoots.map((s) => (
              <li key={s.id}>
                <button
                  type="button"
                  onClick={() => setShootId(s.id)}
                  className="w-full text-left px-3 py-2 flex flex-col gap-0.5"
                  style={{
                    background:
                      shootId === s.id ? "var(--color-bg3)" : "transparent",
                    color: "var(--color-fg)",
                    borderBottom: "1px solid var(--color-border)",
                  }}
                >
                  <span className="text-[12px] font-medium truncate">{s.slug}</span>
                  <span
                    className="font-mono text-2xs"
                    style={{ color: "var(--color-fg-mute)" }}
                  >
                    {s.date} · {s.photoCount} photos
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </aside>

        <main className="flex-1 overflow-y-auto p-3">
          {shootId === null && (
            <p
              className="text-[12px] text-center py-10"
              style={{ color: "var(--color-fg-mute)" }}
            >
              Pick a shoot from the left to add photos.
            </p>
          )}
          {shootLoading && (
            <p className="text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
              Loading photos…
            </p>
          )}
          {shootError && (
            <p className="text-[11px]" style={{ color: "var(--color-danger)" }}>
              Couldn't load photos: {shootError}
            </p>
          )}
          {!shootLoading && !shootError && shootPhotos.length > 0 && (
            <div className="grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-2">
              {shootPhotos.map((p) => {
                const isSelected = selectedIds.has(p.id);
                return (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => toggleSelect(p)}
                    className="relative rounded-md overflow-hidden text-left"
                    style={{
                      border: isSelected
                        ? "2px solid var(--color-accent)"
                        : "1px solid var(--color-border)",
                      background: "var(--color-bg2)",
                    }}
                  >
                    <div style={{ aspectRatio: "4/3" }}>
                      <img
                        src={thumbUrl(p.id)}
                        alt={p.filename}
                        loading="lazy"
                        draggable={false}
                        className="w-full h-full object-cover"
                      />
                    </div>
                    <div
                      className="px-2 py-1 font-mono text-2xs truncate"
                      style={{
                        background: "var(--color-bg)",
                        color: "var(--color-fg-mute)",
                      }}
                    >
                      {p.filename}
                    </div>
                    {isSelected && (
                      <div
                        className="absolute top-1.5 right-1.5 w-5 h-5 rounded-full flex items-center justify-center text-2xs font-bold"
                        style={{
                          background: "var(--color-accent)",
                          color: "var(--color-bg)",
                        }}
                      >
                        ✓
                      </div>
                    )}
                  </button>
                );
              })}
            </div>
          )}
        </main>

        <aside
          className="shrink-0 overflow-y-auto"
          style={{
            width: 280,
            background: "var(--color-bg2)",
            borderLeft: "1px solid var(--color-border)",
          }}
        >
          <div
            className="px-3 py-2 border-b text-[11px] uppercase tracking-[0.5px]"
            style={{ color: "var(--color-fg-mute)", borderColor: "var(--color-border)" }}
          >
            Selected · {selected.length}
          </div>
          {selected.length === 0 ? (
            <p
              className="text-[11px] px-3 py-3"
              style={{ color: "var(--color-fg-mute)" }}
            >
              Click a thumbnail to add it.
            </p>
          ) : (
            <ul>
              {selected.map((s) => (
                <li
                  key={s.photoId}
                  className="px-3 py-1.5 flex items-center justify-between"
                  style={{
                    borderBottom: "1px solid var(--color-border)",
                  }}
                >
                  <span
                    className="font-mono text-2xs truncate"
                    style={{ color: "var(--color-fg)" }}
                  >
                    {s.filename}
                  </span>
                  <button
                    type="button"
                    onClick={() =>
                      setSelected((prev) => prev.filter((p) => p.photoId !== s.photoId))
                    }
                    className="text-2xs px-1.5"
                    style={{ color: "var(--color-fg-mute)" }}
                  >
                    ✕
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>
      </div>
    </div>
  );
}
