import { useState, useEffect, useRef } from "react";

const PROTOCOL_BASE = "http://photosift.localhost";

export function imageUrl(imageId: number): string {
  return `${PROTOCOL_BASE}/image/${imageId}?tier=embedded`;
}

export function thumbUrl(imageId: number): string {
  return `${PROTOCOL_BASE}/thumb/${imageId}`;
}

/**
 * Loads the current image. Single request — no tier upgrading.
 * For NEF files, embedded JPEG is already full-res (~6000px).
 */
export function useImageLoader(imageId: number | null) {
  const [displayUrl, setDisplayUrl] = useState<string | null>(null);
  const prevIdRef = useRef<number | null>(null);

  useEffect(() => {
    if (imageId === null) {
      setDisplayUrl(null);
      return;
    }

    if (imageId === prevIdRef.current) return;
    prevIdRef.current = imageId;

    setDisplayUrl(imageUrl(imageId));
  }, [imageId]);

  return { displayUrl };
}
