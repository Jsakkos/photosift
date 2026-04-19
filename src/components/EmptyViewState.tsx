import type { CullView } from "../types";
import { useProjectStore } from "../stores/projectStore";
import { useSettingsStore } from "../stores/settingsStore";

interface Props {
  view: CullView;
}

/// Rendered in place of the loupe/filmstrip when the current view's
/// filter matches no photos. Each view gets a message tailored to the
/// expected next action so the blank screen doesn't read like a bug.
/// The Route branch is context-aware: if picks exist but the
/// `route_min_star` gate is hiding them, say so instead of claiming
/// everything is routed.
export function EmptyViewState({ view }: Props) {
  const images = useProjectStore((s) => s.images);
  const routeMinStar = useSettingsStore(
    (s) => s.settings.routeMinStar ?? 0,
  );
  const { title, body, hint } = copy(view, images, routeMinStar);
  return (
    <div className="flex-1 flex flex-col items-center justify-center gap-2 bg-[var(--bg-primary)] px-8">
      <p className="text-[var(--text-primary)] text-lg font-light">{title}</p>
      <p className="text-[var(--text-secondary)] text-sm text-center max-w-md">
        {body}
      </p>
      {hint && (
        <p className="text-[var(--text-secondary)]/60 text-xs mt-2">{hint}</p>
      )}
    </div>
  );
}

function copy(
  view: CullView,
  images: { flag: string; destination: string; starRating: number }[],
  routeMinStar: number,
): { title: string; body: string; hint?: string } {
  switch (view) {
    case "triage":
      return {
        title: "Triage complete",
        body:
          "Every photo has been reviewed. Switch to Select to compare picks within groups, or to Route to mark them for edit/publish.",
      };
    case "select":
      return {
        title: "Nothing to select",
        body:
          "No picks yet — run Triage first. If you already triaged, toggle the `select_requires_pick` setting to include unreviewed photos.",
      };
    case "route": {
      // Distinguish the two reasons Route can be empty: (a) no more
      // unrouted picks at all, or (b) unrouted picks exist but
      // `route_min_star` hides them. The spec's default is 3 stars, so
      // case (b) is the more common one in practice.
      const unroutedPicks = images.filter(
        (i) => i.flag === "pick" && i.destination === "unrouted",
      );
      if (unroutedPicks.length > 0 && routeMinStar > 0) {
        const below = unroutedPicks.filter(
          (i) => i.starRating < routeMinStar,
        ).length;
        if (below > 0) {
          return {
            title: `${below} pick${below === 1 ? "" : "s"} hidden by star gate`,
            body: `Route only shows picks rated ${routeMinStar}★ or higher. Rate more picks in Select, or lower the threshold in Settings.`,
            hint: "Set route_min_star to 0 to route every pick.",
          };
        }
      }
      return {
        title: "All picks routed",
        body:
          "Every pick has a destination. Press Ctrl+E to export XMP sidecars for this shoot, or switch back to Select to pick more.",
      };
    }
  }
}
