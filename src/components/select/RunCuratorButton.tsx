import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import {
  getCuratorApiKeyStatus,
  getCuratorStatus,
  startCuratorForShoot,
} from "../../lib/curatorApi";

/// "Run Curator" control for the Select view. The Curator's selection
/// stage judges each locked phash group and ranks its frames — so it
/// belongs here, after grouping is settled, not on import (that role is
/// the triage stage's). Disabled until a provider key is configured.
export function RunCuratorButton() {
  const currentShoot = useProjectStore((s) => s.currentShoot);
  const setToast = useProjectStore((s) => s.setToast);
  const provider = useSettingsStore((s) => s.settings.curatorProvider);

  const [keyReady, setKeyReady] = useState(false);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(
    null,
  );

  useEffect(() => {
    let cancelled = false;
    // Local provider needs no key; cloud providers do.
    if (provider === "local") {
      setKeyReady(true);
    } else {
      getCuratorApiKeyStatus(provider)
        .then((s) => { if (!cancelled) setKeyReady(s.configured); })
        .catch(() => { if (!cancelled) setKeyReady(false); });
    }
    getCuratorStatus()
      .then((s) => {
        if (cancelled) return;
        setRunning(s.status === "running");
        if (s.status === "running") setProgress({ done: s.processed, total: s.total });
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [provider]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    listen<{ processed: number; total: number }>("curator:progress", (e) => {
      setRunning(true);
      setProgress({ done: e.payload.processed, total: e.payload.total });
    }).then((fn) => unlisteners.push(fn));
    listen("curator:completed", () => {
      setRunning(false);
      setProgress(null);
    }).then((fn) => unlisteners.push(fn));
    return () => unlisteners.forEach((fn) => fn());
  }, []);

  if (!currentShoot) return null;

  const onClick = () => {
    if (running || !keyReady) return;
    setRunning(true);
    setProgress(null);
    startCuratorForShoot(currentShoot.id).catch((e) => {
      setRunning(false);
      setToast(`Couldn't start the Curator — ${String(e)}`, "error");
    });
  };

  const label = running
    ? progress && progress.total > 0
      ? `Curating ${progress.done}/${progress.total}`
      : "Curating…"
    : "Run Curator";

  return (
    <button
      type="button"
      tabIndex={-1}
      onClick={onClick}
      disabled={running || !keyReady}
      aria-label="Run the Curator on this shoot"
      title={
        keyReady
          ? "Judge and rank each group with the LLM Curator. Best run after grouping is locked in."
          : "Configure a Curator provider key in Settings to enable this."
      }
      className="inline-flex items-center gap-1.5 font-mono text-2xs uppercase tracking-[0.6px] px-2 py-[3px] rounded-xs cursor-pointer disabled:cursor-not-allowed disabled:opacity-50 border-0"
      style={{
        background: running ? "var(--color-bg3)" : "var(--color-accent)",
        color: running ? "var(--color-fg-dim)" : "var(--color-on-accent)",
      }}
    >
      {label}
    </button>
  );
}
