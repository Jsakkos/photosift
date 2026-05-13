import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useSettingsStore } from "../stores/settingsStore";
import { useAiStore } from "../stores/aiStore";
import {
  getCuratorApiKeyStatus,
  setCuratorApiKey,
  testCuratorConnection,
} from "../lib/curatorApi";
import { FolderLayoutEditor } from "./FolderLayoutEditor";
import { Kbd } from "./primitives";

/// First-run onboarding wizard (#9). Gets a fresh user from "app installed"
/// to "ready to import" without spelunking through Settings: library root +
/// folder layout, the cloud Curator, on-device AI, and a quick three-pass
/// tour. Each step persists its slice immediately via the existing
/// `updateSettings(partial)` (optimistic). Re-openable from the `?` overlay
/// ("Take the tour"), which jumps straight to the three-pass step.
///
/// Gating lives in `App.tsx`: shown when `isLoaded && !onboardedWizard`, or
/// when `wizardReplay` is set. `onClose` clears `wizardReplay`; the first-run
/// path additionally writes `onboardedWizard: true` before closing.

type StepId = 0 | 1 | 2 | 3;
const LAST_STEP: StepId = 3;
const STEP_TITLES = ["Library", "Curator (cloud AI)", "On-device AI", "The three passes"];

const CUDA_DOC_URL = "https://github.com/Jsakkos/photosift#cuda-runtime-dlls";

export function OnboardingWizard({
  replay,
  onClose,
}: {
  replay: boolean;
  onClose: () => void;
}) {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const aiProvider = useAiStore((s) => s.provider);
  // Replays land directly on the tour; first runs start at the top.
  const [step, setStep] = useState<StepId>(replay ? LAST_STEP : 0);

  const finish = (markDone: boolean) => {
    if (markDone && !settings.onboardedWizard) {
      void updateSettings({ onboardedWizard: true });
    }
    onClose();
  };

  const handleSkip = () => {
    if (!replay && settings.libraryRoot == null) {
      const ok = window.confirm(
        "PhotoSift needs a library folder before you can import photos.\n\nSkip the setup anyway? You can finish it later from the â€œ?â€ shortcuts overlay â†’ Take the tour.",
      );
      if (!ok) return;
    }
    finish(true);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        handleSkip();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
    // handleSkip closes over current settings; recreate on change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.libraryRoot, replay]);

  return (
    <div
      data-testid="onboarding-wizard"
      className="fixed inset-0 z-[60] flex items-center justify-center p-6"
      style={{ background: "rgba(0,0,0,0.78)" }}
      role="dialog"
      aria-modal="true"
      aria-label="Welcome to PhotoSift"
    >
      <div
        className="w-[680px] max-w-[95vw] max-h-[88vh] overflow-hidden flex flex-col rounded-md"
        style={{
          background: "var(--color-bg2)",
          border: "1px solid var(--color-border)",
          boxShadow: "0 24px 90px rgba(0,0,0,0.55)",
        }}
      >
        {/* Header + step rail */}
        <div
          className="px-6 pt-5 pb-4 border-b"
          style={{ borderColor: "var(--color-border)" }}
        >
          <div
            className="text-2xs uppercase tracking-[1.4px] mb-0.5"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {replay ? "Take the tour" : "Welcome to PhotoSift"}
          </div>
          <div className="text-lg font-semibold" style={{ color: "var(--color-fg)" }}>
            {STEP_TITLES[step]}
          </div>
          <div className="flex gap-1.5 mt-3">
            {([0, 1, 2, 3] as StepId[]).map((i) => (
              <div
                key={i}
                className="h-[3px] flex-1 rounded-[1px]"
                style={{
                  background:
                    i <= step ? "var(--color-accent-blue)" : "var(--color-bg3)",
                }}
              />
            ))}
          </div>
        </div>

        {/* Step body */}
        <div className="flex-1 overflow-y-auto px-6 py-5">
          {step === 0 && <LibraryStep />}
          {step === 1 && <CuratorStep />}
          {step === 2 && (
            <AiStep
              enabled={settings.enableAiOnImport}
              provider={aiProvider}
              onToggle={(v) => void updateSettings({ enableAiOnImport: v })}
            />
          )}
          {step === 3 && <TourStep />}
        </div>

        {/* Footer */}
        <div
          className="flex items-center justify-between px-6 py-4 border-t"
          style={{ borderColor: "var(--color-border)" }}
        >
          <button
            type="button"
            onClick={handleSkip}
            className="text-xs cursor-pointer bg-transparent border-0 underline"
            style={{ color: "var(--color-fg-dim)" }}
          >
            {replay ? "Close" : "Skip setup"}
          </button>
          <div className="flex items-center gap-2">
            {step > (replay ? LAST_STEP : 0) && (
              <button
                type="button"
                onClick={() => setStep((s) => Math.max(0, s - 1) as StepId)}
                className="px-3.5 py-1.5 rounded-md text-xs cursor-pointer"
                style={{
                  background: "transparent",
                  color: "var(--color-fg-dim)",
                  border: "1px solid var(--color-border)",
                }}
              >
                Back
              </button>
            )}
            {step < LAST_STEP ? (
              <button
                type="button"
                onClick={() => setStep((s) => Math.min(LAST_STEP, s + 1) as StepId)}
                className="px-4 py-1.5 rounded-md text-xs font-medium cursor-pointer"
                style={{ background: "var(--color-accent-blue)", color: "#fff", border: "none" }}
              >
                Next
              </button>
            ) : (
              <button
                type="button"
                onClick={() => finish(true)}
                className="px-4 py-1.5 rounded-md text-xs font-medium cursor-pointer"
                style={{ background: "var(--color-accent-blue)", color: "#fff", border: "none" }}
              >
                {replay ? "Done" : "Start culling"}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function SectionHint({ children }: { children: React.ReactNode }) {
  return (
    <p className="text-xs leading-[1.6] mb-4" style={{ color: "var(--color-fg-dim)" }}>
      {children}
    </p>
  );
}

function LibraryStep() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const pickRoot = async () => {
    const picked = await open({ directory: true });
    if (typeof picked === "string") void updateSettings({ libraryRoot: picked });
  };

  return (
    <div>
      <SectionHint>
        Pick where copied photos live. New imports land under this root using the
        template below â€” leave the defaults if youâ€™re not sure.
      </SectionHint>
      <div className="flex items-center gap-3 mb-5">
        <div
          className="flex-1 px-2.5 py-[7px] rounded-md text-xs font-mono truncate"
          style={{
            background: "var(--color-bg3)",
            border: "1px solid var(--color-border)",
            color: settings.libraryRoot ? "var(--color-fg)" : "var(--color-fg-mute)",
          }}
          title={settings.libraryRoot ?? undefined}
        >
          {settings.libraryRoot ?? "(system Pictures folder)"}
        </div>
        <button
          type="button"
          onClick={pickRoot}
          className="px-3.5 py-[7px] rounded-md text-xs cursor-pointer"
          style={{
            background: "transparent",
            color: "var(--color-fg)",
            border: "1px solid var(--color-border)",
          }}
        >
          Choose folderâ€¦
        </button>
        {settings.libraryRoot && (
          <button
            type="button"
            onClick={() => void updateSettings({ libraryRoot: null })}
            className="text-[11px] cursor-pointer bg-transparent border-0 underline"
            style={{ color: "var(--color-fg-dim)" }}
          >
            Reset
          </button>
        )}
      </div>
      <FolderLayoutEditor
        value={settings.folderTemplate}
        onChange={(ft) => void updateSettings({ folderTemplate: ft })}
      />
    </div>
  );
}

const PROVIDERS: { id: "anthropic" | "gemini" | "local"; label: string }[] = [
  { id: "anthropic", label: "Anthropic" },
  { id: "gemini", label: "Gemini" },
  { id: "local", label: "Local" },
];

const MODEL_OPTIONS: Record<"anthropic" | "gemini", { value: string; label: string }[]> = {
  anthropic: [
    { value: "claude-sonnet-4-6", label: "claude-sonnet-4-6 (recommended)" },
    { value: "claude-opus-4-7", label: "claude-opus-4-7 (more thorough, ~5Ã— cost)" },
    { value: "claude-haiku-4-5", label: "claude-haiku-4-5 (cheaper, less nuance)" },
  ],
  gemini: [
    { value: "gemini-2.5-flash", label: "gemini-2.5-flash (recommended)" },
    { value: "gemini-2.5-pro", label: "gemini-2.5-pro (more thorough, ~5Ã— cost)" },
    { value: "gemini-2.0-flash", label: "gemini-2.0-flash (cheapest)" },
  ],
};

function CuratorStep() {
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const provider = settings.curatorProvider;
  const [keyInput, setKeyInput] = useState("");
  const [keyStatus, setKeyStatus] = useState<{ configured: boolean; suffix: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    setKeyInput("");
    setMsg(null);
    if (provider === "local") {
      setKeyStatus(null);
      return;
    }
    getCuratorApiKeyStatus(provider)
      .then(setKeyStatus)
      .catch(() => setKeyStatus(null));
  }, [provider]);

  const saveKey = async () => {
    if (!keyInput.trim()) return;
    setBusy(true);
    setMsg(null);
    try {
      await setCuratorApiKey(provider, keyInput.trim());
      setKeyStatus(await getCuratorApiKeyStatus(provider));
      setKeyInput("");
      setMsg("Key saved to your OS keychain.");
    } catch (e) {
      setMsg(`Couldnâ€™t save the key: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const testLocal = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await testCuratorConnection();
      setMsg("Connected.");
    } catch (e) {
      setMsg(`Connection failed: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const modelValue =
    provider === "anthropic" ? settings.curatorModelAnthropic : settings.curatorModelGemini;
  const setModel = (value: string) =>
    void updateSettings(
      provider === "anthropic"
        ? { curatorModelAnthropic: value }
        : { curatorModelGemini: value },
    );

  return (
    <div>
      <SectionHint>
        The Curator is an optional cloud LLM that characterises a shoot and
        ranks each cluster on composition + aesthetics. Skip this if youâ€™d
        rather cull on your own â€” PhotoSift works fully without it.
      </SectionHint>
      <div className="flex gap-2 mb-5">
        {PROVIDERS.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => void updateSettings({ curatorProvider: p.id })}
            className="px-3.5 py-1.5 rounded-md text-xs cursor-pointer"
            style={{
              background: provider === p.id ? "var(--color-accent-blue)" : "transparent",
              color: provider === p.id ? "#fff" : "var(--color-fg-dim)",
              border: `1px solid ${provider === p.id ? "var(--color-accent-blue)" : "var(--color-border)"}`,
            }}
          >
            {p.label}
          </button>
        ))}
      </div>

      {provider !== "local" ? (
        <div className="space-y-3">
          <div>
            <div className="text-[11px] mb-[5px]" style={{ color: "var(--color-fg-dim)" }}>
              {PROVIDERS.find((p) => p.id === provider)!.label} API key
              {keyStatus?.configured && (
                <span className="ml-2 font-mono text-2xs" style={{ color: "var(--color-success)" }}>
                  configured â€¢â€¢â€¢â€¢{keyStatus.suffix}
                </span>
              )}
            </div>
            <div className="flex gap-2">
              <input
                type="password"
                value={keyInput}
                onChange={(e) => setKeyInput(e.target.value)}
                placeholder={keyStatus?.configured ? "Replace keyâ€¦" : "Paste your API key"}
                className="flex-1 px-2.5 py-1.5 rounded-md text-xs font-mono"
                style={{
                  background: "var(--color-bg3)",
                  border: "1px solid var(--color-border)",
                  color: "var(--color-fg)",
                }}
              />
              <button
                type="button"
                onClick={() => void saveKey()}
                disabled={busy || !keyInput.trim()}
                className="px-3.5 py-1.5 rounded-md text-xs cursor-pointer disabled:opacity-50"
                style={{ background: "transparent", color: "var(--color-fg)", border: "1px solid var(--color-border)" }}
              >
                Save key
              </button>
            </div>
          </div>
          <div>
            <div className="text-[11px] mb-[5px]" style={{ color: "var(--color-fg-dim)" }}>
              Model
            </div>
            <select
              value={modelValue}
              onChange={(e) => setModel(e.target.value)}
              className="px-2.5 py-1.5 rounded-md text-xs"
              style={{ background: "var(--color-bg3)", border: "1px solid var(--color-border)", color: "var(--color-fg)" }}
            >
              {MODEL_OPTIONS[provider].map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </select>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <div>
            <div className="text-[11px] mb-[5px]" style={{ color: "var(--color-fg-dim)" }}>
              OpenAI-compatible base URL (Ollama, LM Studio, vLLMâ€¦)
            </div>
            <input
              type="text"
              value={settings.curatorLocalBaseUrl}
              onChange={(e) => void updateSettings({ curatorLocalBaseUrl: e.target.value })}
              className="w-full px-2.5 py-1.5 rounded-md text-xs font-mono"
              style={{ background: "var(--color-bg3)", border: "1px solid var(--color-border)", color: "var(--color-fg)" }}
            />
          </div>
          <div>
            <div className="text-[11px] mb-[5px]" style={{ color: "var(--color-fg-dim)" }}>
              Model name
            </div>
            <div className="flex gap-2">
              <input
                type="text"
                value={settings.curatorModelLocal}
                onChange={(e) => void updateSettings({ curatorModelLocal: e.target.value })}
                placeholder="e.g. qwen2.5vl:7b"
                className="flex-1 px-2.5 py-1.5 rounded-md text-xs font-mono"
                style={{ background: "var(--color-bg3)", border: "1px solid var(--color-border)", color: "var(--color-fg)" }}
              />
              <button
                type="button"
                onClick={() => void testLocal()}
                disabled={busy}
                className="px-3.5 py-1.5 rounded-md text-xs cursor-pointer disabled:opacity-50"
                style={{ background: "transparent", color: "var(--color-fg)", border: "1px solid var(--color-border)" }}
              >
                Test
              </button>
            </div>
          </div>
        </div>
      )}
      {msg && (
        <p className="text-[11px] mt-3" style={{ color: "var(--color-fg-dim)" }}>
          {msg}
        </p>
      )}
    </div>
  );
}

function AiStep({
  enabled,
  provider,
  onToggle,
}: {
  enabled: boolean;
  provider: "cuda" | "cpu" | "disabled";
  onToggle: (v: boolean) => void;
}) {
  return (
    <div>
      <SectionHint>
        On-device AI runs face / eye-state / sharpness detection on each photo
        at import â€” no data leaves your machine. Itâ€™s fast on a CUDA GPU and
        still usable on CPU, just slower.
      </SectionHint>
      <label className="flex items-start gap-3 cursor-pointer mb-4">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => onToggle(e.target.checked)}
          className="mt-0.5 cursor-pointer"
        />
        <div>
          <div className="text-xs" style={{ color: "var(--color-fg)" }}>
            Run on-device AI on import
          </div>
          <div className="text-2xs" style={{ color: "var(--color-fg-dim)" }}>
            You can re-run analysis any time from Settings.
          </div>
        </div>
      </label>
      {enabled && (
        <div
          className="text-[11px] rounded-sm px-2.5 py-2"
          style={{ background: "var(--color-bg)", border: "1px solid var(--color-border)", color: "var(--color-fg-dim)" }}
        >
          {provider === "cuda" && (
            <span style={{ color: "var(--color-success)" }}>GPU (CUDA) detected â€” analysis will be fast.</span>
          )}
          {provider === "cpu" && (
            <>
              CUDA isnâ€™t available, so analysis runs on CPU (a few seconds per
              photo).{" "}
              <a href={CUDA_DOC_URL} target="_blank" rel="noreferrer" className="underline" style={{ color: "var(--color-fg)" }}>
                CUDA runtime setup
              </a>
              .
            </>
          )}
          {provider === "disabled" && (
            <span style={{ color: "var(--color-warning)" }}>
              The AI models couldnâ€™t be loaded â€” on-device scoring will be skipped until thatâ€™s resolved.
            </span>
          )}
        </div>
      )}
    </div>
  );
}

const PASSES: { eyebrow: string; title: string; blurb: string; keys: [string, string][] }[] = [
  {
    eyebrow: "Pass 1",
    title: "Triage â€” keep or toss",
    blurb: "Fast first cut: a quick yes/no on every frame.",
    keys: [
      ["P", "pick"],
      ["X", "reject"],
      ["Space", "next"],
    ],
  },
  {
    eyebrow: "Pass 2",
    title: "Select â€” best of the burst",
    blurb: "Compare near-duplicates and crown the keeper of each cluster.",
    keys: [
      ["P", "pick (auto-rejects the rest of the group)"],
      ["Tab", "2-up compare"],
    ],
  },
  {
    eyebrow: "Pass 3",
    title: "Route â€” where it goes",
    blurb: "Send picks to Capture One or to a publish-direct export.",
    keys: [
      ["Click", "select photos, choose a destination from the Route menu"],
      ["Ctrl+E", "write XMP sidecars"],
    ],
  },
];

function TourStep() {
  return (
    <div className="space-y-4">
      <SectionHint>
        Culling is three quick passes. Each is just a view preset â€” a filter
        plus the keys that matter there. You can re-open this tour any time from
        the â€œ?â€ shortcuts overlay.
      </SectionHint>
      {PASSES.map((p) => (
        <div
          key={p.title}
          className="rounded-sm px-4 py-3"
          style={{ background: "var(--color-bg)", border: "1px solid var(--color-border)" }}
        >
          <div className="text-3xs uppercase tracking-[1.2px]" style={{ color: "var(--color-fg-mute)" }}>
            {p.eyebrow}
          </div>
          <div className="text-[13px] font-semibold mt-[1px]" style={{ color: "var(--color-fg)" }}>
            {p.title}
          </div>
          <div className="text-[11px] mt-0.5 mb-2" style={{ color: "var(--color-fg-dim)" }}>
            {p.blurb}
          </div>
          <div className="flex flex-col gap-[5px]">
            {p.keys.map(([k, desc]) => (
              <div key={k} className="flex items-center gap-2 text-[11px]" style={{ color: "var(--color-fg-dim)" }}>
                <Kbd>{k}</Kbd>
                <span>{desc}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
