import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useShootListStore } from "../stores/shootListStore";
import { useSettingsStore } from "../stores/settingsStore";
import { ImportDialog } from "../components/ImportDialog";

export function ShootListPage() {
  const { shoots, isLoading, refresh, deleteShoot } = useShootListStore();
  const openSettings = useSettingsStore((s) => s.openDialog);
  const navigate = useNavigate();
  const [showImport, setShowImport] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
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
    const unlisten = listen("import-complete", () => {
      refresh();
    });
    return () => { unlisten.then((fn) => fn()).catch(() => {}); };
  }, [refresh]);

  return (
    <div className="h-screen w-screen flex flex-col bg-[var(--bg-primary)]">
      <div className="flex items-center justify-between px-6 py-4 border-b border-white/10">
        <h1 className="text-2xl font-light text-[var(--text-primary)]">PhotoSift</h1>
        <div className="flex items-center gap-2">
          <button
            onClick={openSettings}
            title="Settings (,)"
            aria-label="Settings"
            className="w-9 h-9 flex items-center justify-center rounded-lg bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-tertiary)] transition-colors"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3"/>
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
            </svg>
          </button>
          <button
            onClick={() => setShowImport(true)}
            className="px-4 py-2 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors"
          >
            New Import
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-auto p-6">
        {isLoading && shoots.length === 0 && (
          <p className="text-[var(--text-secondary)] text-center mt-12">Loading shoots...</p>
        )}

        {!isLoading && shoots.length === 0 && (
          <div className="flex flex-col items-center justify-center mt-24">
            <p className="text-[var(--text-secondary)] mb-4">No shoots imported yet.</p>
            <button
              onClick={() => setShowImport(true)}
              className="px-6 py-3 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors"
            >
              Import Your First Shoot
            </button>
          </div>
        )}

        {shoots.length > 0 && (
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
            {shoots.map((shoot) => {
              const open = () => navigate(`/shoots/${shoot.id}`);
              const handleDelete = async (e: React.MouseEvent) => {
                e.stopPropagation();
                if (!window.confirm(
                  `Delete shoot "${shoot.slug}"? This removes the DB record and cached thumbnails. RAW files on disk are preserved.`
                )) return;
                try {
                  await deleteShoot(shoot.id);
                } catch (err) {
                  window.alert(`Delete failed: ${err}`);
                }
              };
              return (
                <div
                  key={shoot.id}
                  role="button"
                  tabIndex={0}
                  onClick={open}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      open();
                    }
                  }}
                  className="relative text-left p-4 rounded-lg bg-[var(--bg-secondary)] hover:bg-[var(--bg-tertiary)] border border-white/5 hover:border-white/10 transition-colors cursor-pointer"
                >
                  <div className="font-medium text-[var(--text-primary)] text-lg mb-1 pr-8">
                    {shoot.slug}
                  </div>
                  <div className="text-[var(--text-secondary)] text-sm mb-2">
                    {shoot.date}
                  </div>
                  <div className="text-[var(--text-secondary)] text-xs">
                    {shoot.photoCount} photos
                  </div>
                  <button
                    type="button"
                    onClick={handleDelete}
                    title="Delete shoot"
                    aria-label={`Delete shoot ${shoot.slug}`}
                    className="absolute top-2 right-2 w-7 h-7 flex items-center justify-center rounded text-[var(--text-secondary)]/60 hover:text-red-400 hover:bg-red-400/10 transition-colors"
                  >
                    ×
                  </button>
                </div>
              );
            })}
          </div>
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
