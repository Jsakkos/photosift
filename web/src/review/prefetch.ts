import { previewUrl } from "../api/client";

const AHEAD = 3;
const BEHIND = 1;

/** The photos worth having decoded while `index` is on screen: next 3, previous 1. */
export function prefetchWindow(order: readonly number[], index: number): number[] {
  const ids: number[] = [];
  for (let k = 1; k <= AHEAD; k += 1) {
    const id = order[index + k];
    if (id !== undefined) ids.push(id);
  }
  for (let k = 1; k <= BEHIND; k += 1) {
    const id = order[index - k];
    if (id !== undefined) ids.push(id);
  }
  return ids;
}

/**
 * Keeps decoded previews of the neighbours alive. Holding the `Image` objects
 * is what keeps the decoded pixels around, so showing the same URL afterwards
 * paints within a frame.
 */
export class Prefetcher {
  private held = new Map<number, HTMLImageElement>();

  around(order: readonly number[], index: number): void {
    if (index < 0) return;
    const wanted = new Set(prefetchWindow(order, index));
    const current = order[index];
    if (current !== undefined) wanted.add(current);
    for (const id of [...this.held.keys()]) {
      if (!wanted.has(id)) this.held.delete(id);
    }
    for (const id of wanted) this.load(id);
  }

  /** Resolves once `id`'s preview is decoded (immediately when prefetched). */
  async ready(id: number): Promise<void> {
    try {
      await this.load(id).decode();
    } catch {
      // A failed decode still lets the <img> show its own error state.
    }
  }

  clear(): void {
    this.held.clear();
  }

  private load(id: number): HTMLImageElement {
    let img = this.held.get(id);
    if (!img) {
      img = new Image();
      img.src = previewUrl(id);
      img.decode().catch(() => {});
      this.held.set(id, img);
    }
    return img;
  }
}
