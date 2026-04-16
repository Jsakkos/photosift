import { useProjectStore } from "../stores/projectStore";

export function RatingBar() {
  const { displayItems, currentIndex, setRating } = useProjectStore();
  const image = displayItems[currentIndex]?.image;
  if (!image) return null;

  return (
    <div className="flex items-center justify-center gap-4 py-2 bg-[var(--bg-secondary)] border-t border-white/10">
      <div className="flex items-center gap-1">
        {[1, 2, 3, 4, 5].map((star) => (
          <button
            key={star}
            onClick={() => setRating(image.starRating === star ? 0 : star)}
            className="text-lg transition-colors hover:scale-110"
            style={{
              color: star <= image.starRating ? "var(--star-filled)" : "var(--star-empty)",
            }}
          >
            ★
          </button>
        ))}
      </div>
      <span className="text-xs text-[var(--text-secondary)]">{image.filename}</span>
    </div>
  );
}
