import type { ReactNode } from "react";
import type { GroupTrayPosition } from "../lib/groupTray";

/// Wraps expanded group members inside a continuous tinted container
/// with an accent stripe on the left. First/solo members render a
/// header row; rounded corners cap the ends. Non-end members have
/// neither corner rounded nor header, so the run reads as a single
/// tray even though `react-window` renders each cell independently.
///
/// Must be rendered with a fixed parent height (set by the list cell's
/// `style.height`). The tray fills the cell vertically so the accent
/// stripe is continuous across the run.
export function GroupTray({
  position,
  memberCount,
  children,
}: {
  position: Exclude<GroupTrayPosition, "none">;
  memberCount: number;
  children: ReactNode;
}) {
  const showHeader = position === "first" || position === "solo";
  const roundTop = position === "first" || position === "solo";
  const roundBottom = position === "last" || position === "solo";

  return (
    <div
      className={`h-full bg-[var(--accent)]/10 border-l-[3px] border-[var(--accent)] flex flex-col ${
        roundTop ? "rounded-tr-md" : ""
      } ${roundBottom ? "rounded-br-md mb-1" : ""}`}
    >
      {showHeader && (
        <div className="px-2 pt-1.5 pb-1 flex items-baseline justify-between">
          <span className="text-[10px] font-semibold uppercase tracking-wide text-[var(--accent)]">
            Group · {memberCount}
          </span>
          <span className="text-[9px] text-white/40">Tab</span>
        </div>
      )}
      <div className="flex-1 flex items-center justify-center">{children}</div>
    </div>
  );
}
