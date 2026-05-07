import { useEffect, useRef, useState } from "react";
import { listRemovableDrives } from "../lib/importApi";
import type { DriveInfo } from "../types";

const POLL_INTERVAL_MS = 3000;

interface UseDriveDetectionResult {
  drives: DriveInfo[];
  newlyDetected: DriveInfo | null;
  acknowledge: () => void;
}

/// Polls the backend every 3s for removable drives. Tracks the set of
/// previously-seen mount points so a freshly-inserted card can surface a
/// one-shot toast. The hook is mounted once at the App root so the toast
/// fires regardless of which page the user is on.
///
/// `acknowledge` clears the `newlyDetected` slot so the next insertion
/// can fire its own toast. Re-inserting the same card after acknowledging
/// counts as a new event because the seen-set is only kept in memory for
/// the current poll cycle (a removed drive is forgotten the moment it
/// stops appearing in the list).
export function useDriveDetection(): UseDriveDetectionResult {
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [newlyDetected, setNewlyDetected] = useState<DriveInfo | null>(null);
  const seenRef = useRef<Set<string>>(new Set());
  const initializedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;

    const tick = async () => {
      try {
        const list = await listRemovableDrives();
        if (cancelled) return;
        setDrives(list);

        const currentMounts = new Set(list.map((d) => d.mountPoint));

        if (!initializedRef.current) {
          seenRef.current = currentMounts;
          initializedRef.current = true;
          return;
        }

        for (const drive of list) {
          if (!seenRef.current.has(drive.mountPoint)) {
            setNewlyDetected(drive);
            break;
          }
        }

        seenRef.current = currentMounts;
      } catch {
        // Backend hiccup — try again on the next tick.
      }
    };

    tick();
    const id = setInterval(tick, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  const acknowledge = () => setNewlyDetected(null);

  return { drives, newlyDetected, acknowledge };
}
