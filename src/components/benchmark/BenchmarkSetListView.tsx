import { useEffect } from "react";
import { useBenchmarkStore } from "../../stores/benchmarkStore";

interface Props {
  onOpen: (slug: string) => void;
  onNew: () => void;
}

/// Lists existing benchmark sets read from
/// `~/.photosift-dev/benchmarks/*.json`. Each row has a small judged/
/// total badge so the user can spot which sets need more work without
/// opening them.
export function BenchmarkSetListView({ onOpen, onNew }: Props) {
  const listings = useBenchmarkStore((s) => s.listings);
  const isLoading = useBenchmarkStore((s) => s.isLoadingList);
  const loadError = useBenchmarkStore((s) => s.loadListError);
  const refresh = useBenchmarkStore((s) => s.refreshListings);
  const deleteSet = useBenchmarkStore((s) => s.deleteSet);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="flex flex-col gap-4 p-6 max-w-3xl mx-auto">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-base font-medium" style={{ color: "var(--color-fg)" }}>
            AI quality benchmark sets
          </h1>
          <p className="text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
            Dev-only tool — judge face / eye / smile predictions against your eye.
            Data lives under{" "}
            <code className="font-mono text-[10px]">~/.photosift-dev/benchmarks/</code>.
          </p>
        </div>
        <button
          type="button"
          onClick={onNew}
          className="px-3 py-1.5 rounded-md text-xs font-medium"
          style={{
            background: "var(--color-accent)",
            color: "var(--color-bg)",
          }}
        >
          + New set
        </button>
      </div>

      {isLoading && (
        <div className="text-[11px]" style={{ color: "var(--color-fg-mute)" }}>
          Loading…
        </div>
      )}
      {loadError && (
        <div
          className="text-[11px] px-3 py-2 rounded-md"
          style={{
            background: "var(--color-bg2)",
            color: "var(--color-danger)",
            border: "1px solid var(--color-danger)",
          }}
        >
          Couldn't list benchmark sets: {loadError}
        </div>
      )}
      {!isLoading && !loadError && listings.length === 0 && (
        <div
          className="text-[12px] py-10 text-center rounded-md"
          style={{
            background: "var(--color-bg2)",
            color: "var(--color-fg-mute)",
            border: "1px dashed var(--color-border)",
          }}
        >
          No sets yet. Create one to start judging predictions.
        </div>
      )}

      <ul className="flex flex-col gap-2">
        {listings.map((listing) => (
          <li
            key={listing.slug}
            className="rounded-md px-4 py-3 flex items-center justify-between gap-3"
            style={{
              background: "var(--color-bg2)",
              border: "1px solid var(--color-border)",
            }}
          >
            <button
              type="button"
              onClick={() => onOpen(listing.slug)}
              className="flex flex-col items-start gap-0.5 text-left flex-1 min-w-0"
            >
              <span
                className="text-[13px] font-medium truncate w-full"
                style={{ color: "var(--color-fg)" }}
              >
                {listing.name}
              </span>
              <span
                className="font-mono text-2xs"
                style={{ color: "var(--color-fg-mute)" }}
              >
                {listing.slug} · {listing.judgedCount}/{listing.photoCount} judged ·{" "}
                {listing.createdAt.slice(0, 10)}
              </span>
            </button>
            <button
              type="button"
              onClick={() => {
                if (window.confirm(`Delete "${listing.name}"? This removes the JSON on disk.`)) {
                  void deleteSet(listing.slug);
                }
              }}
              className="text-[11px] px-2 py-1 rounded-sm"
              style={{
                background: "var(--color-bg)",
                color: "var(--color-fg-mute)",
                border: "1px solid var(--color-border)",
              }}
            >
              Delete
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
