import { useEffect, useState } from "react";

import { previewUrl } from "../api/client";
import type { ReviewPhoto } from "../api/types";
import { OUTCOME_LABEL, momentTitle } from "./model";
import type { Prefetcher } from "./prefetch";

interface Props {
  id: number;
  photo: ReviewPhoto | undefined;
  position: number;
  total: number;
  prefetcher: Prefetcher;
}

/**
 * The full-size view. It keeps showing the previous frame until the next one is
 * decoded, so stepping never flashes blank; prefetched neighbours make that
 * wait zero. It has no key handling of its own: the page binds the keys, so
 * they work wherever focus is.
 */
export function Loupe({ id, photo, position, total, prefetcher }: Props) {
  const [shown, setShown] = useState(id);
  useEffect(() => {
    let current = true;
    void prefetcher.ready(id).then(() => {
      if (current) setShown(id);
    });
    return () => {
      current = false;
    };
  }, [id, prefetcher]);

  return (
    <div className="loupe" data-testid="loupe" role="dialog" aria-label="Full-size photo">
      <img
        className="loupe-image"
        data-testid="loupe-image"
        data-photo-id={shown}
        src={previewUrl(shown)}
        alt=""
        draggable={false}
      />
      <div className="loupe-hud">
        <span>
          {position} / {total}
        </span>
        {photo && <span className="muted">{momentTitle(photo.capture_time)}</span>}
        {photo && photo.outcome !== "unreviewed" && (
          <span className={`badge badge-${photo.outcome}`}>{OUTCOME_LABEL[photo.outcome]}</span>
        )}
        {photo?.blooper && <span className="badge badge-blooper">blooper</span>}
      </div>
      <div className="loupe-keys muted">
        ← → photo · A album · L library · R archive · X delete? · B blooper · C compare · Esc close
      </div>
    </div>
  );
}
