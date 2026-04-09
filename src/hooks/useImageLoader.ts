import { useState, useEffect, useRef } from "react";

const PROTOCOL_BASE = "http://photosift.localhost";

export function imageUrl(imageId: number, tier: "embedded" | "preview" | "full"): string {
  return `${PROTOCOL_BASE}/image/${imageId}?tier=${tier}`;
}

export function thumbUrl(imageId: number): string {
  return `${PROTOCOL_BASE}/thumb/${imageId}`;
}

export function useImageLoader(imageId: number | null) {
  const [displayUrl, setDisplayUrl] = useState<string | null>(null);
  const [isUpgrading, setIsUpgrading] = useState(false);
  const prevIdRef = useRef<number | null>(null);

  useEffect(() => {
    if (imageId === null) {
      setDisplayUrl(null);
      return;
    }

    if (imageId === prevIdRef.current) return;
    prevIdRef.current = imageId;

    // Show embedded JPEG immediately
    const embeddedSrc = imageUrl(imageId, "embedded");
    setDisplayUrl(embeddedSrc);
    setIsUpgrading(true);

    // Preload preview, swap when ready
    const previewImg = new Image();
    previewImg.onload = () => {
      if (prevIdRef.current === imageId) {
        setDisplayUrl(previewImg.src);
        setIsUpgrading(false);
      }
    };
    previewImg.onerror = () => {
      setIsUpgrading(false);
    };
    previewImg.src = imageUrl(imageId, "preview");

    return () => {
      previewImg.onload = null;
      previewImg.onerror = null;
    };
  }, [imageId]);

  return { displayUrl, isUpgrading };
}
