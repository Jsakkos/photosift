import { useState } from "react";
import { useProjectStore } from "../stores/projectStore";
import { thumbUrl } from "../hooks/useImageLoader";

export function GroupStrip() {
  const { displayItems, currentIndex, groups, currentView, images, setCurrentIndex } =
    useProjectStore();

  const item = displayItems[currentIndex];
  if (!item?.groupId || currentView !== "select") return null;

  const group = groups.find((g) => g.id === item.groupId);
  if (!group) return null;

  const visibleMembers = group.members.filter((m) => {
    const img = images.find((i) => i.id === m.photoId);
    return img && img.flag !== "reject";
  });

  if (visibleMembers.length <= 1) return null;

  return (
    <div className="bg-[#0d0d0d] border-t border-white/5">
      <div className="text-center py-1 text-[11px] text-[var(--text-secondary)]">
        Group: {group.groupType === "near_duplicate" ? "Near-duplicate" : "Related"} ·{" "}
        {visibleMembers.length} photos — <span className="text-[var(--accent)]">Tab</span> for 2-up
        comparison
      </div>
      <div className="flex gap-1 justify-center pb-2 px-4">
        {visibleMembers.map((member) => {
          const isActive = member.photoId === item.image.id;
          return (
            <GroupThumb
              key={member.photoId}
              photoId={member.photoId}
              isActive={isActive}
              onClick={() => {
                const idx = displayItems.findIndex(
                  (d) => d.image.id === member.photoId,
                );
                if (idx >= 0) setCurrentIndex(idx);
              }}
            />
          );
        })}
      </div>
    </div>
  );
}

function GroupThumb({
  photoId,
  isActive,
  onClick,
}: {
  photoId: number;
  isActive: boolean;
  onClick: () => void;
}) {
  const [loaded, setLoaded] = useState(false);

  return (
    <div
      className={`cursor-pointer rounded overflow-hidden transition-all ${
        isActive
          ? "ring-2 ring-[var(--accent)] shadow-[0_0_8px_rgba(59,130,246,0.3)]"
          : "brightness-75 hover:brightness-90"
      }`}
      style={{ width: 72, height: 54 }}
      onClick={onClick}
    >
      <img
        src={thumbUrl(photoId)}
        alt=""
        className={`w-full h-full object-cover ${loaded ? "opacity-100" : "opacity-30"}`}
        loading="lazy"
        draggable={false}
        onLoad={(e) => {
          if (e.currentTarget.naturalWidth > 1) setLoaded(true);
        }}
      />
    </div>
  );
}
