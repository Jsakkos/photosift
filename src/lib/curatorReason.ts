/// Replace `[N]` photo-id markers in a Curator judgment reason with the
/// corresponding filenames. The Curator's `tech_line` prompts (see
/// `src-tauri/src/curator/prompts.rs`) feed the LLM lines like
/// `[42] sharpness=8/10 faces=2 eyes_open=4/4`, and the model often
/// reuses that bracket form in its prose ("Photo [42] is sharpest").
/// Without substitution the user sees an opaque integer that doesn't
/// match anything visible in the UI.
///
/// Filenames stay on the device — this transformation is rendered into
/// the UI only. The IPC payload sent to the cloud LLM still carries
/// just the integer photo_id, so no filenames leak across the wire.
///
/// Bare numbers and unmatched ids are left untouched so a focal length
/// like "100mm" stays "100mm" and a `[99]` that isn't in the current
/// shoot's images keeps its bracket form (the user can spot the
/// missing reference rather than getting a silently mangled string).
export function formatCuratorReason(
  reason: string,
  idToFilename: Map<number, string>,
): string {
  if (idToFilename.size === 0) return reason;
  return reason.replace(/\[(\d+)\]/g, (match, idStr) => {
    const id = Number.parseInt(idStr, 10);
    const filename = idToFilename.get(id);
    return filename ?? match;
  });
}
