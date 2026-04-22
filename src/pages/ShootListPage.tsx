import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useShootListStore } from "../stores/shootListStore";
import { useSettingsStore } from "../stores/settingsStore";
import { ImportDialog } from "../components/ImportDialog";
import { thumbUrl } from "../hooks/useImageLoader";
import { LogoB } from "../components/primitives";
import type { ShootSummary } from "../types";

interface ImportPhotoReady {
  shootId: number;
  photoId: number;
  filename: string;
  imported: number;
  total: number;
}

type ImportingProgress = Map<number, { imported: number; total: number }>;

function ShootCard({
  shoot,
  progress,
  onOpen,
  onDelete,
}: {
  shoot: ShootSummary;
  progress?: { imported: number; total: number };
  onOpen: () => void;
  onDelete: (e: React.MouseEvent) => void;
}) {
  const picks = shoot.picks ?? 0;
  const rejects = shoot.rejects ?? 0;
  const unreviewed = shoot.unreviewed ?? shoot.photoCount;
  const total = Math.max(1, shoot.photoCount);
  const pickPct = (picks / total) * 100;
  const rejectPct = (rejects / total) * 100;
  const done = unreviewed === 0 && shoot.photoCount > 0;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className="relative rounded-md overflow-hidden cursor-pointer group"
      style={{
        background: "var(--color-bg2)",
        border: "1px solid var(--color-border)",
      }}
    >
      <div className="relative" style={{ aspectRatio: "4/3" }}>
        {shoot.coverPhotoId != null ? (
          <img
            src={thumbUrl(shoot.coverPhotoId)}
            alt=""
            loading="lazy"
            draggable={false}
            className="w-full h-full object-cover"
          />
        ) : (
          <div
            className="w-full h-full flex items-center justify-center font-mono text-[10px] uppercase tracking-[0.8px]"
            style={{
              background: "var(--color-bg3)",
              color: "var(--color-fg-mute)",
            }}
          >
            no cover
          </div>
        )}
        {progress && (
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-[10px] bg-black/50">
            <div className="font-mono text-[11px]" style={{ color: "var(--color-fg)" }}>
              importing · {progress.imported} / {progress.total}
            </div>
            <div
              className="w-[140px] h-[3px] rounded-sm overflow-hidden"
              style={{ background: "rgba(255,255,255,0.1)" }}
            >
              <div
                className="h-full transition-all"
                style={{
                  width: `${Math.min(100, (progress.imported / Math.max(1, progress.total)) * 100)}%`,
                  background: "var(--color-accent-blue)",
                }}
              />
            </div>
          </div>
        )}
        {done && !progress && (
          <div
            className="absolute top-2 right-2 font-mono text-[9px] uppercase tracking-[1px] px-[6px] py-[2px] rounded-sm"
            style={{ color: "var(--color-success)", background: "rgba(0,0,0,0.55)" }}
          >
            ✓ routed
          </div>
        )}
        <button
          type="button"
          onClick={onDelete}
          title="Delete shoot"
          aria-label={`Delete shoot ${shoot.slug}`}
          className="absolute top-2 left-2 w-6 h-6 flex items-center justify-center rounded-xs opacity-0 group-hover:opacity-100 transition-opacity"
          style={{
            background: "rgba(0,0,0,0.6)",
            color: "var(--color-fg-dim)",
          }}
        >
          ×
        </button>
      </div>
      <div className="px-[14px] py-3">
        <div className="flex items-baseline justify-between mb-1 gap-3">
          <div
            className="text-[14px] font-semibold truncate"
            style={{ color: "var(--color-fg)" }}
          >
            {shoot.slug}
          </div>
          <div
            className="font-mono text-[10px] shrink-0"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {shoot.date}
          </div>
        </div>
        <div
          className="text-[11px] mb-[10px]"
          style={{ color: "var(--color-fg-dim)" }}
        >
          {shoot.photoCount} photos
        </div>
        <div className="flex gap-[10px] font-mono text-[10px]">
          <span style={{ color: "var(--color-success)" }}>● {picks} kept</span>
          <span style={{ color: "var(--color-danger)" }}>● {rejects} tossed</span>
          <span style={{ color: "var(--color-warning)" }}>
            ★ {unreviewed} left
          </span>
        </div>
        {!progress && (
          <div
            className="mt-[10px] h-[2px] rounded-[1px] overflow-hidden flex"
            style={{ background: "var(--color-bg3)" }}
          >
            <div
              style={{ width: `${pickPct}%`, background: "var(--color-success)" }}
            />
            <div
              style={{ width: `${rejectPct}%`, background: "var(--color-danger)" }}
            />
          </div>
        )}
      </div>
    </div>
  );
}

export function ShootListPage() {
  const { shoots, isLoading, refresh, deleteShoot } = useShootListStore();
  const openSettings = useSettingsStore((s) => s.openDialog);
  const navigate = useNavigate();
  const [showImport, setShowImport] = useState(false);
  const [importingProgress, setImportingProgress] = useState<ImportingProgress>(new Map());
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return shoots;
    return shoots.filter(
      (s) => s.slug.toLowerCase().includes(q) || s.date.includes(q),
    );
  }, [shoots, query]);

  const totalPhotos = useMemo(
    () => shoots.reduce((sum, s) => sum + s.photoCount, 0),
    [shoots],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        e.target instanceof HTMLSelectElement
      )
        return;
      if (e.key === ",") {
        e.preventDefault();
        openSettings();
      } else if (
        (e.ctrlKey || e.metaKey) &&
        (e.key === "i" || e.key === "I" || e.key === "o" || e.key === "O")
      ) {
        e.preventDefault();
        setShowImport(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openSettings]);

  useEffect(() => {
    refresh();
    const unlistenComplete = listen<{ shootId: number }>("import-complete", (ev) => {
      setImportingProgress((prev) => {
        if (!prev.has(ev.payload.shootId)) return prev;
        const next = new Map(prev);
        next.delete(ev.payload.shootId);
        return next;
      });
      refresh();
    });
    const unlistenReady = listen<ImportPhotoReady>("import-photo-ready", (ev) => {
      const { shootId, imported, total } = ev.payload;
      setImportingProgress((prev) => {
        const next = new Map(prev);
        next.set(shootId, { imported, total });
        return next;
      });
      if (imported === 1) refresh();
    });
    return () => {
      unlistenComplete.then((fn) => fn()).catch(() => {});
      unlistenReady.then((fn) => fn()).catch(() => {});
    };
  }, [refresh]);

  const handleDelete = async (shoot: ShootSummary, e: React.MouseEvent) => {
    e.stopPropagation();
    if (
      !window.confirm(
        `Delete shoot "${shoot.slug}"? This removes the DB record and cached thumbnails. RAW files on disk are preserved.`,
      )
    )
      return;
    try {
      await deleteShoot(shoot.id);
    } catch (err) {
      window.alert(`Delete failed: ${err}`);
    }
  };

  return (
    <div
      className="h-screen w-screen flex flex-col overflow-hidden"
      style={{ background: "var(--color-bg)", color: "var(--color-fg)" }}
    >
      <div className="flex items-center justify-between px-8 pt-6 pb-5">
        <div className="flex items-center gap-[14px]">
          <div style={{ color: "var(--color-accent)" }}>
            <LogoB size={28} />
          </div>
          <div>
            <div
              className="text-[10px] uppercase tracking-[1.4px]"
              style={{ color: "var(--color-fg-dim)" }}
            >
              Library
            </div>
            <div
              className="text-[22px] font-semibold leading-tight"
              style={{ color: "var(--color-fg)", letterSpacing: -0.4 }}
            >
              {shoots.length} shoot{shoots.length === 1 ? "" : "s"} · {totalPhotos} photos
            </div>
          </div>
        </div>
        <div className="flex items-center gap-[10px]">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search shoots…"
            className="px-[10px] py-[6px] rounded-md text-[12px] font-mono w-[220px]"
            style={{
              background: "var(--color-bg2)",
              border: "1px solid var(--color-border)",
              color: "var(--color-fg)",
            }}
          />
          <button
            type="button"
            onClick={openSettings}
            title="Settings (,)"
            aria-label="Settings"
            className="px-[12px] py-[6px] rounded-md text-[12px] cursor-pointer"
            style={{
              background: "transparent",
              border: "1px solid var(--color-border)",
              color: "var(--color-fg-dim)",
            }}
          >
            ⚙ Settings
          </button>
          <button
            type="button"
            onClick={() => setShowImport(true)}
            className="px-[14px] py-[6px] rounded-md text-[12px] font-medium cursor-pointer"
            style={{
              background: "var(--color-accent-blue)",
              color: "#fff",
              border: "none",
            }}
          >
            ＋ Import
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto px-8 pb-8">
        {isLoading && shoots.length === 0 && (
          <p
            className="text-center mt-16 text-[12px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            Loading shoots…
          </p>
        )}

        {!isLoading && shoots.length === 0 && (
          <div className="flex flex-col items-center justify-center mt-24 gap-4">
            <div style={{ color: "var(--color-accent)", opacity: 0.7 }}>
              <LogoB size={96} />
            </div>
            <p
              className="text-[13px] max-w-[320px] text-center"
              style={{ color: "var(--color-fg-dim)" }}
            >
              Your library is empty. Import a folder of RAW files to start culling.
            </p>
            <button
              type="button"
              onClick={() => setShowImport(true)}
              className="px-5 py-[8px] rounded-md text-[13px] font-medium cursor-pointer"
              style={{
                background: "var(--color-accent-blue)",
                color: "#fff",
                border: "none",
              }}
            >
              Import your first shoot
            </button>
          </div>
        )}

        {filtered.length > 0 && (
          <div
            className="grid gap-5"
            style={{ gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))" }}
          >
            {filtered.map((shoot) => (
              <ShootCard
                key={shoot.id}
                shoot={shoot}
                progress={importingProgress.get(shoot.id)}
                onOpen={() => navigate(`/shoots/${shoot.id}`)}
                onDelete={(e) => void handleDelete(shoot, e)}
              />
            ))}
          </div>
        )}

        {!isLoading && shoots.length > 0 && filtered.length === 0 && (
          <p
            className="text-center mt-12 text-[12px]"
            style={{ color: "var(--color-fg-mute)" }}
          >
            No shoots match “{query}”.
          </p>
        )}
      </div>

      {showImport && (
        <ImportDialog
          onClose={() => setShowImport(false)}
          onComplete={(shootId) => {
            setShowImport(false);
            navigate(`/shoots/${shootId}`);
          }}
        />
      )}
    </div>
  );
}
