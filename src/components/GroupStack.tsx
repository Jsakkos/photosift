import { useState } from "react";
import { thumbUrl } from "../hooks/useImageLoader";

interface GroupStackProps {
  imageId: number;
  filename: string;
  count: number;
  isCurrent: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
}

export function GroupStack({ imageId, filename, count, isCurrent, onClick, onDoubleClick }: GroupStackProps) {
  const [loaded, setLoaded] = useState(false);

  return (
    <div
      className="relative cursor-pointer"
      style={{ width: 92, height: 72 }}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
    >
      {/* Shadow layers */}
      <div
        className="absolute rounded border border-white/10"
        style={{ top: 0, left: 4, width: 84, height: 60, background: "#252525" }}
      />
      <div
        className="absolute rounded border border-white/10"
        style={{ top: 2, left: 2, width: 84, height: 60, background: "#222" }}
      />
      {/* Cover image */}
      <div
        className={`absolute rounded overflow-hidden ${
          isCurrent
            ? "ring-2 ring-[var(--accent)] brightness-100"
            : "brightness-75 hover:brightness-90"
        }`}
        style={{ top: 4, left: 0, width: 84, height: 60 }}
      >
        <img
          src={thumbUrl(imageId)}
          alt={filename}
          className={`w-full h-full object-cover ${loaded ? "opacity-100" : "opacity-30"}`}
          loading="lazy"
          draggable={false}
          onLoad={(e) => {
            if (e.currentTarget.naturalWidth > 1) setLoaded(true);
          }}
        />
      </div>
      {/* Count badge */}
      <div className="absolute -top-1 -right-1 bg-[var(--accent)] text-white text-[10px] font-semibold px-1.5 py-0.5 rounded-full z-10 min-w-[20px] text-center">
        {count}
      </div>
    </div>
  );
}
