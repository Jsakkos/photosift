import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { useShootListStore } from "../stores/shootListStore";
import { ImportDialog } from "../components/ImportDialog";

export function ShootListPage() {
  const { shoots, isLoading, refresh } = useShootListStore();
  const navigate = useNavigate();
  const [showImport, setShowImport] = useState(false);

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
        <button
          onClick={() => setShowImport(true)}
          className="px-4 py-2 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors"
        >
          New Import
        </button>
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
            {shoots.map((shoot) => (
              <button
                key={shoot.id}
                onClick={() => navigate(`/shoots/${shoot.id}`)}
                className="text-left p-4 rounded-lg bg-[var(--bg-secondary)] hover:bg-[var(--bg-tertiary)] border border-white/5 hover:border-white/10 transition-colors"
              >
                <div className="font-medium text-[var(--text-primary)] text-lg mb-1">
                  {shoot.slug}
                </div>
                <div className="text-[var(--text-secondary)] text-sm mb-2">
                  {shoot.date}
                </div>
                <div className="text-[var(--text-secondary)] text-xs">
                  {shoot.photoCount} photos
                </div>
              </button>
            ))}
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
