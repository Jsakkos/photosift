import { useRef } from "react";
import type { FolderTemplate } from "../stores/settingsStore";
import { DEFAULT_FOLDER_TEMPLATE } from "../stores/settingsStore";
import {
  BUCKET_LABELS,
  BUCKET_ORDER,
  PATH_TOKENS,
  previewBucketTree,
  validateFolderTemplate,
  type BucketKey,
} from "../lib/folderTemplate";

interface Props {
  value: FolderTemplate;
  onChange: (next: FolderTemplate) => void;
}

/// Polished editor for the configurable shoot-folder layout (#10):
/// a path-template field with insert-at-cursor token chips, per-field
/// validation badges, a live on-disk preview tree, and reset-to-defaults.
/// Validation logic is shared with the Settings save-gate via
/// `validateFolderTemplate`.
export function FolderLayoutEditor({ value, onChange }: Props) {
  const pathRef = useRef<HTMLInputElement | null>(null);
  const v = validateFolderTemplate(value);
  const tree = previewBucketTree(value);

  const setPath = (pathTemplate: string) => onChange({ ...value, pathTemplate });
  const setBucket = (key: BucketKey, name: string) =>
    onChange({ ...value, buckets: { ...value.buckets, [key]: name } });

  const insertToken = (token: string) => {
    const el = pathRef.current;
    if (!el) {
      setPath(value.pathTemplate + token);
      return;
    }
    const start = el.selectionStart ?? value.pathTemplate.length;
    const end = el.selectionEnd ?? value.pathTemplate.length;
    const next =
      value.pathTemplate.slice(0, start) + token + value.pathTemplate.slice(end);
    setPath(next);
    // Restore caret just after the inserted token on the next tick.
    requestAnimationFrame(() => {
      el.focus();
      const caret = start + token.length;
      el.setSelectionRange(caret, caret);
    });
  };

  const isDefault =
    JSON.stringify(value) === JSON.stringify(DEFAULT_FOLDER_TEMPLATE);

  return (
    <div className="mb-4 pt-4 border-t border-white/5">
      <div className="flex items-center justify-between mb-1">
        <h3 className="text-sm font-semibold text-fg">
          Folder layout
        </h3>
        <button
          type="button"
          onClick={() => onChange(DEFAULT_FOLDER_TEMPLATE)}
          disabled={isDefault}
          className="text-xs text-fg-dim hover:text-fg underline disabled:opacity-40 disabled:no-underline"
        >
          Reset to defaults
        </button>
      </div>
      <p className="text-xs text-fg-dim mb-3">
        Where shoots are created and what the cull buckets are named. The path
        template applies to new imports only â€” existing shoots stay where they
        are. Renaming a bucket relocates that bucket's files on the next layout
        sync (the old, now-empty folder is left behind).
      </p>

      {/* Path template */}
      <label className="block text-sm text-fg-dim mb-1">
        Shoot path template
      </label>
      <input
        ref={pathRef}
        type="text"
        value={value.pathTemplate}
        onChange={(e) => setPath(e.target.value)}
        spellCheck={false}
        className={`w-full px-3 py-2 rounded-lg bg-bg text-fg border text-sm font-mono ${
          v.pathError ? "border-red-500/70" : "border-white/10"
        }`}
      />
      <div className="flex flex-wrap gap-1 mt-2">
        {PATH_TOKENS.map((tok) => (
          <button
            key={tok}
            type="button"
            onClick={() => insertToken(tok)}
            title={`Insert ${tok}`}
            className="px-2 py-1 rounded bg-bg3 hover:bg-white/10 text-fg-dim hover:text-fg text-xs font-mono transition-colors"
          >
            {tok}
          </button>
        ))}
      </div>
      {v.pathError && (
        <p className="text-xs text-red-400 mt-1">{v.pathError}</p>
      )}

      {/* Bucket names */}
      <div className="mt-4 grid grid-cols-2 gap-x-3 gap-y-2">
        {BUCKET_ORDER.map((key) => (
          <div key={key}>
            <label className="block text-xs text-fg-dim mb-1">
              {BUCKET_LABELS[key]}
            </label>
            <input
              type="text"
              value={value.buckets[key]}
              onChange={(e) => setBucket(key, e.target.value)}
              spellCheck={false}
              className={`w-full px-2.5 py-1.5 rounded-lg bg-bg text-fg border text-sm font-mono ${
                v.bucketErrors[key] ? "border-red-500/70" : "border-white/10"
              }`}
            />
            {v.bucketErrors[key] && (
              <p className="text-2xs text-red-400 mt-0.5">
                {v.bucketErrors[key]}
              </p>
            )}
          </div>
        ))}
      </div>

      {/* Live preview */}
      <div className="mt-4 p-3 rounded-lg bg-bg border border-white/5">
        <div className="text-2xs uppercase tracking-wide text-fg-dim mb-2">
          Preview (sample shoot)
        </div>
        <div className="font-mono text-[11px] leading-relaxed text-fg-dim space-y-0.5">
          {tree.map((row) => (
            <div key={row.label} className="flex gap-2">
              <span className="text-fg-dim/50 w-14 shrink-0">
                {row.label}
              </span>
              <span className="text-fg break-all">
                {row.path}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
