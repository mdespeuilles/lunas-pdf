// Mise en page des pages dans la zone de lecture (fonctions pures, testées).

export type ViewMode = "single" | "continuous" | "double";
export type FitMode = "width" | "page" | null;

/** 1 pt PDF = 1/72 po ; 1 px CSS = 1/96 po. À 100 %, la page a sa taille réelle. */
export const PT_TO_PX = 96 / 72;
export const PAD_TOP = 28;
export const PAD_SIDE = 24;
export const PAD_BOTTOM = 72;
export const GAP = 16;
export const SPREAD_GAP = 12;
export const ZOOM_MIN = 0.25;
export const ZOOM_MAX = 4;
export const ZOOM_PRESETS = [0.5, 0.75, 1, 1.5, 2, 4];
const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4];

export interface PageSize {
  width: number;
  height: number;
}

export interface PageBox {
  index: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Layout {
  boxes: PageBox[];
  width: number;
  height: number;
}

/** Groupes de pages affichés côte à côte (double page : 1-2, 3-4…). */
export function rows(count: number, mode: ViewMode, current: number): number[][] {
  if (mode === "single") return count ? [[Math.min(Math.max(current, 0), count - 1)]] : [];
  const out: number[][] = [];
  const step = mode === "double" ? 2 : 1;
  for (let i = 0; i < count; i += step) out.push(step === 2 && i + 1 < count ? [i, i + 1] : [i]);
  return out;
}

export function computeLayout(pages: PageSize[], mode: ViewMode, zoom: number, viewportWidth: number, current = 0): Layout {
  const k = zoom * PT_TO_PX;
  const groups = rows(pages.length, mode, current);
  // Tailles arrondies au pixel dès le départ pour que positions et dimensions restent entières.
  const size = (i: number) => ({ w: Math.round(pages[i].width * k), h: Math.round(pages[i].height * k) });
  const rowWidths = groups.map((g) => g.reduce((s, i) => s + size(i).w, 0) + (g.length - 1) * SPREAD_GAP);
  const width = Math.max(viewportWidth, Math.max(0, ...rowWidths) + 2 * PAD_SIDE);
  const boxes: PageBox[] = [];
  let y = PAD_TOP;
  groups.forEach((g, r) => {
    let x = Math.round((width - rowWidths[r]) / 2);
    const rowH = Math.max(...g.map((i) => size(i).h));
    for (const i of g) {
      const { w, h } = size(i);
      boxes.push({ index: i, x, y: y + Math.round((rowH - h) / 2), w, h });
      x += w + SPREAD_GAP;
    }
    y += rowH + GAP;
  });
  return { boxes, width, height: y - GAP + PAD_BOTTOM };
}

/** Zoom qui fait tenir la page la plus large (ou la plus haute) dans la zone visible. */
export function fitZoom(pages: PageSize[], mode: ViewMode, fit: Exclude<FitMode, null>, vw: number, vh: number): number {
  if (!pages.length) return 1;
  const perRow = mode === "double" ? 2 : 1;
  const maxW = Math.max(...pages.map((p) => p.width)) * perRow;
  const maxH = Math.max(...pages.map((p) => p.height));
  const gaps = (perRow - 1) * SPREAD_GAP;
  // Arrondi vers le bas : un pixel de trop ferait apparaître une barre de défilement horizontale.
  const floor = (z: number) => Math.floor(z * 1000) / 1000;
  const byWidth = floor((vw - 2 * PAD_SIDE - gaps - 2) / (maxW * PT_TO_PX));
  if (fit === "width") return clampZoom(byWidth);
  const byHeight = floor((vh - PAD_TOP - GAP) / (maxH * PT_TO_PX));
  return clampZoom(Math.min(byWidth, byHeight));
}

export function clampZoom(z: number): number {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(z * 1000) / 1000));
}

export function zoomStep(z: number, dir: 1 | -1): number {
  if (dir > 0) return ZOOM_STEPS.find((s) => s > z + 0.001) ?? ZOOM_MAX;
  return [...ZOOM_STEPS].reverse().find((s) => s < z - 0.001) ?? ZOOM_MIN;
}

/** Page courante : la première dont le bas dépasse le tiers supérieur de la vue. */
export function currentPage(layout: Layout, scrollTop: number, vh: number): number {
  const line = scrollTop + vh * 0.33;
  const hit = layout.boxes.find((b) => b.y + b.h + GAP / 2 > line);
  return hit ? hit.index : (layout.boxes.at(-1)?.index ?? 0);
}

/** Pages intersectant [scrollTop - marge, scrollTop + vh + marge], par ordre de proximité. */
export function visiblePages(layout: Layout, scrollTop: number, vh: number, margin: number): number[] {
  const top = scrollTop - margin;
  const bottom = scrollTop + vh + margin;
  const mid = scrollTop + vh / 2;
  return layout.boxes
    .filter((b) => b.y + b.h > top && b.y < bottom)
    .sort((a, b) => Math.abs(a.y + a.h / 2 - mid) - Math.abs(b.y + b.h / 2 - mid))
    .map((b) => b.index);
}

/** Position de défilement pour amener une page (et un point de la page, en pt) en haut. */
export function scrollTarget(layout: Layout, page: number, zoom: number, yPt = 0): number | null {
  const b = layout.boxes.find((x) => x.index === page);
  if (!b) return null;
  return Math.max(0, b.y - (yPt ? 48 : PAD_TOP) + yPt * zoom * PT_TO_PX);
}

/** Taille en pixels écran d'une page, et découpage en tuiles au-delà d'un seuil. */
export const MAX_FULL_BITMAP_PX = 12_000_000;
export const TILE = 1024;

export function tilesFor(pxW: number, pxH: number, visible: { x0: number; y0: number; x1: number; y1: number }) {
  const out: { x: number; y: number; w: number; h: number }[] = [];
  const tx0 = Math.max(0, Math.floor(visible.x0 / TILE));
  const ty0 = Math.max(0, Math.floor(visible.y0 / TILE));
  const tx1 = Math.min(Math.ceil(pxW / TILE) - 1, Math.floor(visible.x1 / TILE));
  const ty1 = Math.min(Math.ceil(pxH / TILE) - 1, Math.floor(visible.y1 / TILE));
  for (let ty = ty0; ty <= ty1; ty++)
    for (let tx = tx0; tx <= tx1; tx++) {
      const x = tx * TILE;
      const y = ty * TILE;
      out.push({ x, y, w: Math.min(TILE, pxW - x), h: Math.min(TILE, pxH - y) });
    }
  return out;
}
