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
  const selectMinStar = useProjectStore((s) => s.selectMinStar);
  const routeMinStar = useSettingsStore(
    (s) => s.settings.routeMinStar ?? 0,
  );
  const { title, body, hint } = copy(view, images, routeMinStar, selectMinStar);
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
  selectMinStar: number,
): { title: string; body: string; hint?: string } {
  switch (view) {
    case "triage":
      return {
        title: "Triage complete",
        body:
          "Every photo has been reviewed. Switch to Select to compare picks within groups, or to Route to mark them for edit/publish.",
      };
    case "select": {
      // Two reasons Select can be empty:
      //   (a) literally no picks yet — user hasn't triaged
      //   (b) picks exist but were rejected or filtered (e.g. hide-soft
      //       threshold is hiding them)
      const picks = images.filter((i) => i.flag === "pick").length;
      // Most common "empty Select" is the multi-pass floor sitting above
      // every rated photo. Point the user at the chips / bracket keys
      // instead of a filter they can't see.
      if (picks > 0 && selectMinStar > 0) {
        return {
          title: `No photos at ${selectMinStar}★+`,
          body: `You have ${picks} pick${picks === 1 ? "" : "s"}, but none have been rated ${selectMinStar}★ or higher. Rate a photo up to ${selectMinStar}★ to promote it into this pass, or press [ to step back to a lower tier.`,
          hint: "Use the pass chips above the filmstrip to jump between tiers.",
        };
      }
      if (picks > 0) {
        return {
          title: "No picks shown",
          body: `You have ${picks} pick${picks === 1 ? "" : "s"} but none match the current filter. Flip "Select view requires pick" off in Settings to include unreviewed photos, or check the Show-all toggle.`,
        };
      }
      return {
        title: "Nothing to select",
        body:
          "No picks yet \u2014 run Triage first, then come back to Select to compare winners within each burst.",
      };
    }
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
