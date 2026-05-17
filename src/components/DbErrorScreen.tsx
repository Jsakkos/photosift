interface Props {
  error: string;
}

/// Fatal-error screen shown when the library database cannot be opened —
/// most often a failed schema migration. Replaces the entire app shell so the
/// user sees the real cause, instead of `get_settings` silently falling back
/// to default settings and stranding them on an undismissable onboarding
/// wizard.
export function DbErrorScreen({ error }: Props) {
  return (
    <div className="h-screen w-screen flex flex-col items-center justify-center gap-3 bg-bg px-8">
      <p className="text-fg text-lg font-light">
        PhotoSift couldn&apos;t open its library database
      </p>
      <p className="text-fg-dim text-sm text-center max-w-md">
        The database failed to open, so PhotoSift can&apos;t start. Your photos
        and culling decisions are untouched — this is a startup problem, not
        data loss.
      </p>
      <pre
        className="text-fg-dim text-xs text-center max-w-lg whitespace-pre-wrap rounded-md px-3 py-2 font-mono"
        style={{ border: "1px solid var(--color-fg-dim)" }}
      >
        {error}
      </pre>
      <p className="text-fg-dim/60 text-xs mt-1 text-center max-w-md">
        The database lives at ~/.photosift/photosift.db. Restart PhotoSift once
        the issue is resolved, or report this with the message above.
      </p>
    </div>
  );
}
