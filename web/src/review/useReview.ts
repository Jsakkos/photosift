import { useCallback, useEffect, useRef, useState } from "react";

import { getReview, setOutcome as postOutcome } from "../api/client";
import type { Outcome, ServerEvent, ShootReview } from "../api/types";
import { useServerEvents } from "../api/ws";
import { mergePhotos, type PhotoMap } from "./model";

export interface Notice {
  kind: "info" | "error";
  text: string;
}

interface Confirmed {
  outcome: Outcome;
  blooper: boolean;
}

export interface EnrichProgress {
  done: number;
  total: number;
}

export interface ReviewData {
  review: ShootReview | null;
  photos: PhotoMap;
  error: string | null;
  progress: EnrichProgress | null;
  /** Optimistic: the screen changes now, the server answer reconciles it. */
  setOutcome: (photoId: number, outcome: Outcome, blooper?: boolean) => void;
  reload: () => void;
}

const REFETCH_DEBOUNCE_MS = 1500;

/** Loads a shoot's review and keeps it live; outcome changes are sent one at a time per photo. */
export function useReview(shootId: number, notify: (notice: Notice) => void): ReviewData {
  const [review, setReview] = useState<ShootReview | null>(null);
  const [photos, setPhotos] = useState<PhotoMap>({});
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<EnrichProgress | null>(null);

  const photosRef = useRef<PhotoMap>(photos);
  const inFlight = useRef(new Map<number, number>());
  const confirmed = useRef(new Map<number, Confirmed>());
  const chains = useRef(new Map<number, Promise<void>>());
  const latest = useRef(0);
  const notifyRef = useRef(notify);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => {
    photosRef.current = photos;
    notifyRef.current = notify;
  });

  const reload = useCallback(() => {
    const mine = ++latest.current;
    getReview(shootId).then(
      (fresh) => {
        if (mine !== latest.current) return;
        for (const photo of Object.values(fresh.photos)) {
          if ((inFlight.current.get(photo.id) ?? 0) === 0) {
            confirmed.current.set(photo.id, { outcome: photo.outcome, blooper: photo.blooper });
          }
        }
        setReview(fresh);
        setPhotos((current) => mergePhotos(fresh.photos, current, inFlight.current));
        setError(null);
      },
      (e: unknown) => {
        if (mine !== latest.current) return;
        setError(e instanceof Error ? e.message : String(e));
      },
    );
  }, [shootId]);

  useEffect(() => {
    setReview(null);
    setPhotos({});
    setProgress(null);
    inFlight.current.clear();
    confirmed.current.clear();
    chains.current.clear();
    reload();
    return () => {
      latest.current += 1;
      clearTimeout(timer.current);
    };
  }, [reload]);

  // Unenriched photos show as single-frame bursts until grouping lands, so a
  // progress tick re-reads the shoot (at most once per debounce window).
  const scheduleReload = useCallback(() => {
    if (timer.current !== undefined) return;
    timer.current = setTimeout(() => {
      timer.current = undefined;
      reload();
    }, REFETCH_DEBOUNCE_MS);
  }, [reload]);

  useServerEvents((event: ServerEvent) => {
    if (event.type === "enrich_progress" && event.shoot_id === shootId) {
      setProgress({ done: event.done, total: event.total });
      scheduleReload();
    } else if (event.type === "enrich_done" && event.shoot_id === shootId) {
      setProgress(null);
      clearTimeout(timer.current);
      timer.current = undefined;
      reload();
    }
  }, reload);

  const setOutcome = useCallback(
    (photoId: number, outcome: Outcome, blooper?: boolean) => {
      const key = String(photoId);
      const before = photosRef.current[key];
      if (!before) return;
      if (!confirmed.current.has(photoId)) {
        confirmed.current.set(photoId, { outcome: before.outcome, blooper: before.blooper });
      }
      const apply = (value: Confirmed) =>
        setPhotos((p) => {
          const photo = p[key];
          return photo ? { ...p, [key]: { ...photo, ...value } } : p;
        });
      apply({ outcome, blooper: blooper ?? before.blooper });
      inFlight.current.set(photoId, (inFlight.current.get(photoId) ?? 0) + 1);

      const run = async () => {
        try {
          const res = await postOutcome(photoId, outcome, blooper);
          const truth = { outcome: res.outcome, blooper: res.blooper };
          confirmed.current.set(photoId, truth);
          // A newer press is already on screen; only the last answer reconciles.
          if ((inFlight.current.get(photoId) ?? 0) <= 1) apply(truth);
          if (res.xmp_error) {
            notifyRef.current({
              kind: "error",
              text: `Saved, but the Lightroom sidecar could not be written: ${res.xmp_error}`,
            });
          }
        } catch (e) {
          const text = e instanceof Error ? e.message : String(e);
          notifyRef.current({ kind: "error", text: `Could not save that change: ${text}` });
          const truth = confirmed.current.get(photoId);
          if (truth && (inFlight.current.get(photoId) ?? 0) <= 1) apply(truth);
        } finally {
          inFlight.current.set(photoId, Math.max(0, (inFlight.current.get(photoId) ?? 1) - 1));
        }
      };
      const previous = chains.current.get(photoId) ?? Promise.resolve();
      chains.current.set(photoId, previous.then(run));
    },
    [],
  );

  return { review, photos, error, progress, setOutcome, reload };
}
