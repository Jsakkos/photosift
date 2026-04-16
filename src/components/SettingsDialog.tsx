import { useState, useEffect } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import { useProjectStore } from "../stores/projectStore";

export function SettingsDialog() {
  const { isOpen, settings, closeDialog, updateSettings, reclusterShoot } =
    useSettingsStore();
  const { currentShoot, loadShoot } = useProjectStore();

  const [nearDup, setNearDup] = useState(settings.nearDupThreshold);
  const [related, setRelated] = useState(settings.relatedThreshold);
  const [expand, setExpand] = useState(settings.triageExpandGroups);
  const [reclustering, setReclustering] = useState(false);
  const [reclusterMsg, setReclusterMsg] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setNearDup(settings.nearDupThreshold);
      setRelated(settings.relatedThreshold);
      setExpand(settings.triageExpandGroups);
      setReclusterMsg(null);
    }
  }, [isOpen, settings]);

  if (!isOpen) return null;

  const valid = nearDup >= 0 && nearDup <= 64 && related >= nearDup && related <= 64;

  const handleSave = async () => {
    await updateSettings({
      nearDupThreshold: nearDup,
      relatedThreshold: related,
      triageExpandGroups: expand,
    });
    closeDialog();
  };

  const handleRecluster = async () => {
    if (!currentShoot || !valid) return;
    setReclustering(true);
    setReclusterMsg(null);
    try {
      await updateSettings({
        nearDupThreshold: nearDup,
        relatedThreshold: related,
      });
      const groupCount = await reclusterShoot(currentShoot.id);
      await loadShoot(currentShoot.id);
      setReclusterMsg(`Re-clustered: ${groupCount} group${groupCount === 1 ? "" : "s"}`);
    } catch (e) {
      setReclusterMsg(`Error: ${e}`);
    } finally {
      setReclustering(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-[var(--bg-secondary)] rounded-xl border border-white/10 p-6 w-[480px] max-w-[90vw]">
        <h2 className="text-xl font-medium text-[var(--text-primary)] mb-4">Settings</h2>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Near-duplicate threshold (hamming distance, 0–8 typical)
          </label>
          <input
            type="number"
            min={0}
            max={64}
            value={nearDup}
            onChange={(e) => setNearDup(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
          />
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Lower = stricter. Default 4.
          </p>
        </div>

        <div className="mb-4">
          <label className="block text-sm text-[var(--text-secondary)] mb-1">
            Related threshold (hamming distance)
          </label>
          <input
            type="number"
            min={0}
            max={64}
            value={related}
            onChange={(e) => setRelated(parseInt(e.target.value) || 0)}
            className="w-full px-3 py-2 rounded-lg bg-[var(--bg-primary)] text-[var(--text-primary)] border border-white/10 text-sm"
          />
          <p className="text-xs text-[var(--text-secondary)] mt-1">
            Must be ≥ near-duplicate threshold. Default 12.
          </p>
        </div>

        <div className="mb-4">
          <label className="flex items-center gap-2 text-sm text-[var(--text-primary)] cursor-pointer">
            <input
              type="checkbox"
              checked={expand}
              onChange={(e) => setExpand(e.target.checked)}
              className="w-4 h-4"
            />
            Expand groups in triage by default
          </label>
          <p className="text-xs text-[var(--text-secondary)] mt-1 ml-6">
            When on, triage shows every image individually instead of just the group cover.
          </p>
        </div>

        {!valid && (
          <p className="text-red-400 text-sm mb-3">
            Invalid thresholds: related must be ≥ near-duplicate and both within 0–64.
          </p>
        )}

        {currentShoot && (
          <div className="mb-4 p-3 rounded-lg bg-[var(--bg-primary)] border border-white/5">
            <div className="flex items-center justify-between">
              <span className="text-sm text-[var(--text-secondary)]">
                Re-cluster current shoot with these thresholds
              </span>
              <button
                onClick={handleRecluster}
                disabled={reclustering || !valid}
                className="px-3 py-1.5 rounded bg-[var(--bg-tertiary)] hover:bg-white/10 text-[var(--text-primary)] text-xs transition-colors disabled:opacity-50"
              >
                {reclustering ? "Re-clustering..." : "Re-cluster"}
              </button>
            </div>
            {reclusterMsg && (
              <p className="text-xs text-[var(--accent)] mt-2">{reclusterMsg}</p>
            )}
          </div>
        )}

        <div className="flex justify-end gap-2">
          <button
            onClick={closeDialog}
            className="px-4 py-2 rounded-lg text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!valid}
            className="px-4 py-2 rounded-lg bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white font-medium transition-colors disabled:opacity-50"
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
