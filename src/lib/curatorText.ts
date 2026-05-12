/// Curator reasons reference photos by their SQLite `photo_id` wrapped in
/// square brackets — e.g. `"[42] is the sharpest of the burst"`. The LLM only
/// ever sees DB ids, never filenames (filenames must not leave the device for
/// the inference endpoint — see `src-tauri/src/curator/prompts.rs`). So the
/// `[id] -> filename` mapping happens here, on the frontend, at render time.
///
/// `filenameFor` should return the photo's basename, or `null` if the id is no
/// longer in the store (deleted) — in which case the token degrades to
/// `(removed)` rather than leaving a bare integer the user can't resolve.
export function humanizeCuratorReason(
  reason: string,
  filenameFor: (photoId: number) => string | null,
): string {
  return reason.replace(/\[(\d+)\]/g, (_match, digits: string) => {
    return filenameFor(Number(digits)) ?? "(removed)";
  });
}
