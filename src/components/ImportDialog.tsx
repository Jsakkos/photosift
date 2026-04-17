import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ImportProgress {
  shootId: number;
  phase: string;
  current: number;
  total: number;
  currentFilename: string;
}

interface ImportComplete {
  shootId: number;
  photoCount: number;
  dedupSkipped: number;
}

interface ImportDialogProps {
  onClose: () => void;
  onComplete: (shootId: number) => void;
}

type ImportMode = "copy" | "in_place";

export function ImportDialog({ onClose, onComplete }: ImportDialogProps) {
  const [sourcePath, setSourcePath] = useState("");
  const [slug, setSlug] = useState("");
  const [importMode, setImportMode] = useState<ImportMode>("copy");
  const [importing, setImporting] = useState(false);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handlePickFolder = useCallback(async () => {
    const selected = await open({ directory: true });
    if (selected) {
      setSourcePath(selected);
      if (!slug) {
        const name = selected.split(/[/\\]/).pop() || "";
        setSlug(name.replace(/[^a-zA-Z0-9_-]/g, "-"));
      }
    }
  }, [slug]);

  useEffect(() => {
    if (!importing) return;

    const unlistenProgress = listen<ImportProgress>("import-progress", (event) => {
      setProgress(event.payload);
    });

    const unlistenComplete = listen<ImportComplete>("import-complete", (event) => {
      setImporting(false);
      onComplete(event.payload.shootId);
    });

    const unlistenError = listen<string>("import-error", (event) => {
      setImporting(false);
      setError(event.payload);
    });

    return () => {
      unlistenProgress.then((fn) => fn()).catch(() => {});
      unlistenComplete.then((fn) => fn()).catch(() => {});
      unlistenError.then((fn) => fn()).catch(() => {});
    };
  }, [importing, onComplete]);

  const handleStart = useCallback(async () => {
    if (!sourcePath || !slug.trim()) return;

    setError(null);
    setImporting(true);

    try {
      await invoke("start_import", {
        sourcePath,
        slug: slug.trim(),
        importMode,
      });
    } catch (e) {
      setError(String(e));
      setImporting(false);
    }
  }, [sourcePath, slug, importMode]);

  const phaseLabel = progress?.phase === "processing"
    ? "Processing files..."
    : progress?.phase === "clustering"
    ? "Clustering groups..."
    : progress?.phase === "finalizing"
    ? "Saving to database..."
    : "Scanning...";

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-[var(--bg-secondary)] rounded-xl border border-white/10 p-6 w-[480px] max-w-[90vw]">
        <h2 className="text-xl font-medium text-[var(--text-primary)] mb-4">
          Import Photos
        </h2>

        {!importing ? (
          <>
            <div className="mb-4">
              <label className="block text-sm text-[var(--text-secondary)] mb-2">
                Import Mode
              </label>
              <div className="flex flex-col gap-2">
                <label className="flex items-start gap-2 cursor-pointer text-sm">
                  <input
                    type="radio"
                    name="import-mode"
                    value="copy"
                    checked={importMode === "copy"}
                    onChange={() => setImportMode("copy")}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="text-[var(--text-primary)]">Copy to library</span>
                    <span className="block text-xs text-[var(--text-secondary)]">
                      Files are copied into a canonical folder under the library root.
                    </span>
                  </span>
                </label>
                <label className="flex items-start gap-2 cursor-pointer text-sm">
                  <input
                    type="radio"
                    name="import-mode"
                    value="in_place"
                    checked={importMode === "in_place"}
                    onChange={() => setImportMode("in_place")}
                    className="mt-0.5"
                  />
                  <span>
                    <span className="text-[var(--text-primary)]">Import in-place</span>
                    <span className="block text-xs text-[var(--text-secondary)]">
                      Register files where they are. XMP sidecars land next to the originals on export.
                    </span>
                  </span>
                </label>
              </div>
            </div>

            <div className="mb-4">
              <label className="block text-sm text-[var(--text-secondary)] mb-1">
                Source Folder
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={sourcePath}
                  readOnly
                  placeholder="Select folder..."
                  className="flex-1 px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
                />
                <button
                  onClick={handlePickFolder}
                  className="px-3 py-2 rounded-lg bg-[var(--bg-tertiary)] text-[var(--text-primary)] hover:bg-white/10 transition-colors text-sm"
                >
                  Browse
                </button>
              </div>
            </div>

            <div className="mb-6">
              <label className="block text-sm text-[var(--text-secondary)] mb-1">
                Description
              </label>
              <input
                type="text"
                value={slug}
                onChange={(e) => setSlug(e.target.value)}
                placeholder="e.g. Greece-Trip"
                className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
              />
            </div>

            {error && (
              <p className="text-red-400 text-sm mb-4">{error}</p>
            )}

            <div className="flex justify-end gap-2">
              <button
                onClick={onClose}
                className="px-4 py-2 rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleStart}
                disabled={!sourcePath || !slug.trim()}
                className="px-4 py-2 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors disabled:opacity-50"
              >
                Start Import
              </button>
            </div>
          </>
        ) : (
          <div>
            <p className="text-[var(--text-primary)] mb-2">{phaseLabel}</p>
            {progress && progress.total > 0 && (
              <>
                <div className="w-full h-2 bg-[var(--bg-primary)] rounded-full overflow-hidden mb-2">
                  <div
                    className="h-full bg-[var(--accent)] transition-all"
                    style={{
                      width: `${Math.round((progress.current / progress.total) * 100)}%`,
                    }}
                  />
                </div>
                <p className="text-sm text-[var(--text-secondary)]">
                  {progress.current} / {progress.total}
                  {progress.currentFilename && (
                    <span className="ml-2 opacity-70">{progress.currentFilename}</span>
                  )}
                </p>
              </>
            )}
            <div className="flex justify-end mt-4">
              <button
                onClick={async () => {
                  try { await invoke("cancel_import"); } catch {}
                }}
                className="px-4 py-2 rounded-lg text-red-400 hover:text-red-300 hover:bg-red-400/10 transition-colors text-sm"
              >
                Cancel Import
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
