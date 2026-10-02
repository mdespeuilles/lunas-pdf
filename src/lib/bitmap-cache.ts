// Cache LRU des bitmaps rendues, borné en mémoire, avec déduplication des demandes en cours.
import { fetchBitmap, type RenderParams } from "./protocol";
import { inTauri } from "./api";

const BUDGET_BYTES = 512 * 1024 * 1024;

interface Entry {
  bmp: ImageBitmap;
  bytes: number;
}

class BitmapCache {
  private map = new Map<string, Entry>();
  private inflight = new Map<string, Promise<ImageBitmap | null>>();
  private bytes = 0;

  static key(p: Omit<RenderParams, "priority" | "epoch">): string {
    const t = p.tile ? `@${p.tile.x},${p.tile.y}` : "";
    return `${p.doc}:${p.page}:r${p.rev ?? 0}:${p.width}x${p.height}${t}`;
  }

  get(key: string): ImageBitmap | undefined {
    const e = this.map.get(key);
    if (!e) return undefined;
    // Rafraîchit la position LRU.
    this.map.delete(key);
    this.map.set(key, e);
    return e.bmp;
  }

  /** Meilleure bitmap déjà en cache pour une page entière (la plus large ≤ largeur voulue, sinon la plus proche). */
  bestFor(doc: number, page: number, width: number): ImageBitmap | undefined {
    let best: ImageBitmap | undefined;
    const prefix = `${doc}:${page}:`;
    for (const [k, e] of this.map) {
      if (!k.startsWith(prefix) || k.includes("@")) continue;
      const w = e.bmp.width;
      if (!best || (w <= width && w > best.width) || (best.width > width && w < best.width)) best = e.bmp;
    }
    return best;
  }

  async load(p: RenderParams): Promise<ImageBitmap | null> {
    const key = BitmapCache.key(p);
    const hit = this.get(key);
    if (hit) return hit;
    const pending = this.inflight.get(key);
    if (pending) return pending;
    const promise = fetchBitmap(p, inTauri)
      .then((bmp) => {
        if (bmp) this.put(key, bmp);
        return bmp;
      })
      .finally(() => this.inflight.delete(key));
    this.inflight.set(key, promise);
    return promise;
  }

  private put(key: string, bmp: ImageBitmap) {
    const bytes = bmp.width * bmp.height * 4;
    this.map.set(key, { bmp, bytes });
    this.bytes += bytes;
    for (const [k, e] of this.map) {
      if (this.bytes <= BUDGET_BYTES) break;
      if (k === key) continue;
      e.bmp.close();
      this.bytes -= e.bytes;
      this.map.delete(k);
    }
  }

  dropDoc(doc: number) {
    for (const [k, e] of this.map) {
      if (k.startsWith(`${doc}:`)) {
        e.bmp.close();
        this.bytes -= e.bytes;
        this.map.delete(k);
      }
    }
  }
}

export const bitmaps = new BitmapCache();
export { BitmapCache };
