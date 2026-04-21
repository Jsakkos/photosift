import { thumbUrl } from "../hooks/useImageLoader";
import { AiPickBadge } from "./AiPickBadge";

interface GroupStackProps {
  imageId: number;
  filename: string;
  count: number;
  isCurrent: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  isAiPick?: boolean;
  /// Cover image dimensions; shadow layers render behind at the same
  /// size with small offsets. Defaults match the previous hard-coded
  /// 84×60 if the caller doesn't override.
  coverW?: number;
  coverH?: number;
}

const SHADOW_OFFSET = 4;

export function GroupStack({
  imageId,
  filename,
  count,
  isCurrent,
  onClick,
  onDoubleClick,
  isAiPick,
  coverW = 84,
  coverH = 60,
}: GroupStackProps) {
  // Outer bounding box accommodates the shadow layers' diagonal offset.
  const outerW = coverW + SHADOW_OFFSET * 2;
  const outerH = coverH + SHADOW_OFFSET * 2;

  return (
    <div
      className="relative cursor-pointer"
      style={{ width: outerW, height: outerH }}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      title={`Group of ${count} similar photos\nDouble-click (or Enter) to drill in.\nAI pick is shown as the cover.`}
      aria-label={`Group of ${count} photos`}
    >
      {/* Shadow layers: back-left and back-right for the 3D stack look */}
      <div
        className="absolute rounded border border-white/10"
        style={{ top: 0, left: SHADOW_OFFSET * 2, width: coverW, height: coverH, background: "#252525" }}
      />
      <div
        className="absolute rounded border border-white/10"
        style={{ top: SHADOW_OFFSET / 2, left: SHADOW_OFFSET, width: coverW, height: coverH, background: "#222" }}
      />
      {/* Cover image */}
      <div
        className={`absolute rounded overflow-hidden ${
          isCurrent
            ? "ring-2 ring-[var(--accent)] brightness-100"
            : "brightness-75 hover:brightness-90"
        }`}
        style={{ top: SHADOW_OFFSET, left: 0, width: coverW, height: coverH }}
      >
        <img
          key={imageId}
          src={thumbUrl(imageId)}
          alt={filename}
          className="w-full h-full object-cover"
          draggable={false}
        />
      </div>
      {/* Count badge */}
      <div
        className="absolute -top-1 -right-1 bg-[var(--accent)] text-white text-[10px] font-semibold px-1.5 py-0.5 rounded-full z-10 min-w-[20px] text-center"
        title={`${count} photos in this group`}
        aria-hidden="true"
      >
        {count}
      </div>
      {/* AI pick badge */}
      {isAiPick && <AiPickBadge />}
    </div>
  );
}
