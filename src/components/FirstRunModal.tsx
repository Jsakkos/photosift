import { useEffect } from "react";
import { useSettingsStore } from "../stores/settingsStore";
import type { Settings } from "../stores/settingsStore";
import type { CullView } from "../types";
import { Kbd } from "./primitives";

/// First-run-only explainer modal for each culling view (#13). Shown
/// the first time the user lands on Triage / Select / Route, dismissed
/// via Esc or "Got it", and never shown again until "Replay tour" in
/// the shortcuts overlay flips the `onboarded*` flags back to false.
///
/// Copy is kept in sync with the keyboard map in `PhotoSift_Spec.md`
/// and the shortcuts overlay — if a binding changes, update all three.

interface ViewGuide {
  /// The settings flag that gates this view's modal.
  flag: keyof Pick<Settings, "onboardedTriage" | "onboardedSelect" | "onboardedRoute">;
  eyebrow: string;
  title: string;
  /// One-sentence "what this view is for".
  blurb: string;
  /// Primary actions: each `keys` renders as a row of <Kbd>.
  keys: { keys: string[]; label: string }[];
  /// What "done" looks like — what flows you to the next stage.
  done: string;
}

const GUIDES: Record<CullView, ViewGuide> = {
  triage: {
    flag: "onboardedTriage",
    eyebrow: "Pass 1",
    title: "Triage — keep or toss?",
    blurb:
      "The fast first pass: one keep/toss decision per photo. Near-duplicate bursts collapse to a single thumbnail, so one keystroke can decide the whole burst.",
    keys: [
      { keys: ["P"], label: "Keep (pick)" },
      { keys: ["X"], label: "Reject" },
      { keys: ["Space"], label: "Skip to next unreviewed" },
      { keys: ["Shift", "P"], label: "Keep everything in this group" },
    ],
    done: "Done when every photo is reviewed — your kept photos then flow into Select.",
  },
  select: {
    flag: "onboardedSelect",
    eyebrow: "Pass 2",
    title: "Select — which are the best?",
    blurb:
      "Narrow your keepers down through rating passes: rate the ones worth keeping at ★1, raise the floor so only ★1+ show, rate the best of those at ★2, and on up to ★5.",
    keys: [
      { keys: ["1"], label: "Rate ★1 (through ★5)" },
      { keys: ["]"], label: "Raise the pass floor" },
      { keys: ["["], label: "Lower the pass floor" },
      { keys: ["Tab"], label: "2-up comparison" },
      { keys: ["X"], label: "Reject (out of Select)" },
    ],
    done: "Done when you've visited every pick and raised the floor at least once — your picks then flow into Route.",
  },
  route: {
    flag: "onboardedRoute",
    eyebrow: "Pass 3",
    title: "Route — edit, or publish as-is?",
    blurb:
      "Give each pick a destination: into your editor (Capture One / DxO) for work, or straight to publish. Route is mouse-driven — select photos, then pick a destination from the Route dropdown.",
    keys: [],
    done: "Done when every pick has a destination.",
  },
};

export function FirstRunModal({ view }: { view: CullView }) {
  const settings = useSettingsStore((s) => s.settings);
  const isLoaded = useSettingsStore((s) => s.isLoaded);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const guide = GUIDES[view];
  // Don't flash a modal before settings load (the default flags are
  // `false`, which would briefly show the modal then hide it once the
  // real DB values arrive).
  const open = isLoaded && !settings[guide.flag];

  const dismiss = () => {
    void updateSettings({ [guide.flag]: true } as Partial<Settings>);
  };

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        dismiss();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, view]);

  if (!open) return null;

  return (
    <div
      data-testid="first-run-modal"
      data-view={view}
      className="fixed inset-0 z-[60] flex items-center justify-center p-6"
      style={{ background: "rgba(0,0,0,0.72)" }}
      onClick={dismiss}
      role="dialog"
      aria-modal="true"
      aria-label={`About the ${view} view`}
    >
      <div
        className="w-[460px] max-w-[92vw] rounded-md overflow-hidden"
        style={{
          background: "var(--color-bg2)",
          border: "1px solid var(--color-border)",
          color: "var(--color-fg)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="px-5 py-4 border-b"
          style={{ borderColor: "var(--color-border)" }}
        >
          <div
            className="text-[9px] uppercase tracking-[1.4px]"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {guide.eyebrow}
          </div>
          <div className="text-[16px] font-semibold mt-[2px]">{guide.title}</div>
        </div>

        <div className="px-5 py-4 flex flex-col gap-4">
          <p className="text-[13px] leading-relaxed" style={{ color: "var(--color-fg)" }}>
            {guide.blurb}
          </p>

          {guide.keys.length > 0 && (
            <div className="flex flex-col gap-[6px]">
              {guide.keys.map((row, i) => (
                <div key={i} className="flex items-center justify-between gap-3 text-[12px]">
                  <span style={{ color: "var(--color-fg)" }}>{row.label}</span>
                  <span className="flex items-center gap-[3px]">
                    {row.keys.map((k, ki) => (
                      <Kbd key={ki}>{k}</Kbd>
                    ))}
                  </span>
                </div>
              ))}
            </div>
          )}

          <p
            className="text-[12px] leading-relaxed pt-2 border-t"
            style={{
              color: "var(--color-fg-dim)",
              borderColor: "var(--color-border)",
            }}
          >
            {guide.done}
          </p>
        </div>

        <div
          className="px-5 py-3 border-t flex items-center justify-between"
          style={{ borderColor: "var(--color-border)" }}
        >
          <span className="text-[11px]" style={{ color: "var(--color-fg-dim)" }}>
            Press <Kbd>?</Kbd> any time for all shortcuts
          </span>
          <button
            type="button"
            onClick={dismiss}
            className="text-[12px] px-3 py-1.5 rounded-sm cursor-pointer border-0"
            style={{ background: "var(--color-accent-blue)", color: "#fff" }}
          >
            Got it · <Kbd>Esc</Kbd>
          </button>
        </div>
      </div>
    </div>
  );
}
