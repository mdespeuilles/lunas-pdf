// Bitmaps des pages via le protocole lunas-pdf:// (voir src-tauri/src/protocol.rs).

const isWindows = typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
export const PROTOCOL_BASE = isWindows ? "http://lunas-pdf.localhost" : "lunas-pdf://localhost";

export interface TileRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface RenderParams {
  doc: number;
  page: number;
  /** Taille en pixels de la page entière. */
  width: number;
  height: number;
  tile?: TileRect;
  /** 0–255, plus grand = plus urgent. */
  priority: number;
  epoch: number;
  /** Révision de la page (change après chaque modification d'annotation). */
  rev?: number;
}

export function renderUrl(p: RenderParams): string {
  let url = `${PROTOCOL_BASE}/render/${p.doc}/${p.page}?w=${p.width}&h=${p.height}&p=${p.priority}&e=${p.epoch}&r=${p.rev ?? 0}`;
  if (p.tile) url += `&tx=${p.tile.x}&ty=${p.tile.y}&tw=${p.tile.w}&th=${p.tile.h}`;
  return url;
}

export function thumbUrl(id: string): string {
  return `${PROTOCOL_BASE}/thumb/${id}.png`;
}

/** Rendu simulé hors Tauri : page blanche avec des lignes grises. */
async function mockBitmap(p: RenderParams): Promise<ImageBitmap> {
  const t = p.tile ?? { x: 0, y: 0, w: p.width, h: p.height };
  const c = new OffscreenCanvas(t.w, t.h);
  const g = c.getContext("2d")!;
  g.fillStyle = "#fff";
  g.fillRect(0, 0, t.w, t.h);
  g.translate(-t.x, -t.y);
  const s = p.width / 600;
  g.fillStyle = "#8f8f98";
  g.fillRect(66 * s, 60 * s, 300 * s, 14 * s);
  g.fillStyle = "#d6d6dc";
  for (let i = 0; i < 40; i++) g.fillRect(66 * s, (100 + i * 18) * s, (i % 7 === 6 ? 260 : 468) * s, 6 * s);
  return createImageBitmap(c);
}

/** Récupère une bitmap ; `null` si la demande a été abandonnée (époque périmée). */
export async function fetchBitmap(p: RenderParams, inTauri: boolean): Promise<ImageBitmap | null> {
  if (!inTauri) return mockBitmap(p);
  const res = await fetch(renderUrl(p));
  if (res.status === 204) return null;
  if (!res.ok) throw new Error(`rendu page ${p.page + 1} : HTTP ${res.status}`);
  const w = Number(res.headers.get("x-w"));
  const h = Number(res.headers.get("x-h"));
  const buf = await res.arrayBuffer();
  const data = new ImageData(new Uint8ClampedArray(buf), w, h);
  return createImageBitmap(data);
}
