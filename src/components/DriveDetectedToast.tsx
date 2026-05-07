import { useEffect } from "react";
import { useDriveDetection } from "../hooks/useDriveDetection";
import { useImportIntentStore } from "../stores/importIntentStore";

const DWELL_MS = 7000;

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/// Bottom-floating notification fired the first time a removable drive
/// appears since the app started polling. Acts as a one-click jump into
/// the Import dialog with the new drive pre-selected.
export function DriveDetectedToast() {
  const { newlyDetected, acknowledge } = useDriveDetection();
  const requestImport = useImportIntentStore((s) => s.requestImport);

  useEffect(() => {
    if (!newlyDetected) return;
    const id = setTimeout(acknowledge, DWELL_MS);
    return () => clearTimeout(id);
  }, [newlyDetected, acknowledge]);

  if (!newlyDetected) return null;

  const title = newlyDetected.label ?? "Removable drive";
  const letter = newlyDetected.driveLetter ? `${newlyDetected.driveLetter}:` : newlyDetected.mountPoint;

  return (
    <div
      className="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100]"
      role="status"
      aria-live="polite"
    >
      <div
        className="relative flex items-center gap-3 pl-4 pr-2 py-2 rounded-md shadow-lg"
        style={{
          background: "var(--color-bg2)",
          border: "1px solid var(--color-border)",
          minWidth: "320px",
        }}
      >
        <div
          className="absolute left-0 top-0 bottom-0"
          style={{
            width: "4px",
            background: "var(--color-accent)",
            borderTopLeftRadius: "var(--radius-md)",
            borderBottomLeftRadius: "var(--radius-md)",
          }}
        />
        <div className="flex-1 pl-1">
          <div
            className="text-[13px] font-semibold"
            style={{ color: "var(--color-fg)" }}
          >
            {title} ({letter}) detected
          </div>
          <div
            className="text-[11px] font-mono"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {formatBytes(newlyDetected.totalBytes)} ·{" "}
            {formatBytes(newlyDetected.availableBytes)} free
          </div>
        </div>
        <button
          type="button"
          onClick={() => {
            requestImport(newlyDetected);
            acknowledge();
          }}
          className="px-3 py-1 rounded-md text-[12px] font-medium cursor-pointer"
          style={{
            background: "var(--color-accent-blue)",
            color: "#fff",
            border: "none",
          }}
        >
          Import
        </button>
        <button
          type="button"
          onClick={acknowledge}
          aria-label="Dismiss"
          className="text-[18px] leading-none px-1 cursor-pointer"
          style={{
            background: "transparent",
            color: "var(--color-fg-mute)",
            border: "none",
          }}
        >
          ×
        </button>
      </div>
    </div>
  );
}
