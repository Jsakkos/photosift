import { open } from "@tauri-apps/plugin-dialog";
import type { DriveInfo } from "../../types";

interface DriveSourceBarProps {
  drives: DriveInfo[];
  selectedMountPoint: string | null;
  selectedFolderPath: string | null;
  onSelectDrive: (drive: DriveInfo) => void;
  onSelectFolder: (path: string) => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(0)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function DriveSourceBar({
  drives,
  selectedMountPoint,
  selectedFolderPath,
  onSelectDrive,
  onSelectFolder,
}: DriveSourceBarProps) {
  const handleBrowse = async () => {
    const picked = await open({ directory: true });
    if (typeof picked === "string") onSelectFolder(picked);
  };

  const folderLabel = selectedFolderPath
    ? selectedFolderPath.split(/[/\\]/).filter(Boolean).pop() || selectedFolderPath
    : null;

  return (
    <div className="flex items-stretch gap-2 flex-wrap">
      {drives.length === 0 && !selectedFolderPath && (
        <div
          className="flex items-center px-2.5 text-[11px] italic"
          style={{ color: "var(--color-fg-mute)", minHeight: "60px" }}
        >
          Plug in an SD card to import from a card, or browse a folder.
        </div>
      )}

      {drives.map((drive) => {
        const isSelected = selectedMountPoint === drive.mountPoint;
        const title = drive.label ?? "Removable";
        const letter = drive.driveLetter ? `${drive.driveLetter}:` : drive.mountPoint;
        return (
          <button
            key={drive.mountPoint}
            type="button"
            onClick={() => onSelectDrive(drive)}
            className="relative flex flex-col justify-center text-left rounded-md px-3 py-2 cursor-pointer transition-colors duration-base"
            style={{
              minWidth: "200px",
              minHeight: "60px",
              background: isSelected ? "var(--color-bg2)" : "var(--color-bg3)",
              border: isSelected
                ? "1px solid var(--color-accent-blue)"
                : "1px solid var(--color-border)",
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
            <div
              className="text-[13px] font-semibold pl-2 truncate"
              style={{ color: "var(--color-fg)" }}
            >
              {title}
            </div>
            <div
              className="text-[11px] font-mono pl-2 truncate"
              style={{ color: "var(--color-fg-dim)" }}
            >
              {letter}{"  "}
              {formatBytes(drive.totalBytes)}{" · "}
              {formatBytes(drive.availableBytes)} free
            </div>
          </button>
        );
      })}

      {selectedFolderPath ? (
        <button
          type="button"
          onClick={handleBrowse}
          className="relative flex flex-col justify-center text-left rounded-md px-3 py-2 cursor-pointer transition-colors duration-base"
          style={{
            minWidth: "200px",
            minHeight: "60px",
            background: "var(--color-bg2)",
            border: "1px solid var(--color-accent-blue)",
          }}
          title={selectedFolderPath}
        >
          <div
            className="text-[13px] font-semibold truncate"
            style={{ color: "var(--color-fg)" }}
          >
            {folderLabel}
          </div>
          <div
            className="text-[11px] font-mono truncate"
            style={{ color: "var(--color-fg-dim)" }}
          >
            Folder · click to change
          </div>
        </button>
      ) : (
        <button
          type="button"
          onClick={handleBrowse}
          className="self-center px-3 py-1.5 rounded-md text-xs cursor-pointer"
          style={{
            background: "transparent",
            border: "1px solid var(--color-border)",
            color: "var(--color-fg-dim)",
          }}
        >
          Browse folder…
        </button>
      )}
    </div>
  );
}
