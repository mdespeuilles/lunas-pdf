// Géométrie des annotations côté interface (points d'affichage), fonctions pures.
import type { Annot, Point, Rect } from "../bindings";

export type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "from" | "to";

export const BOX_HANDLES: Handle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

export function rectFrom(a: Point, b: Point): Rect {
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
}

export function clampRect(r: Rect, pageW: number, pageH: number): Rect {
  const w = Math.min(r.w, pageW);
  const h = Math.min(r.h, pageH);
  return { x: Math.min(Math.max(0, r.x), pageW - w), y: Math.min(Math.max(0, r.y), pageH - h), w, h };
}

/** Contrainte à 45° (Maj) pour les lignes. */
export function snap45(from: Point, to: Point): Point {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy);
  const ang = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
  return { x: from.x + len * Math.cos(ang), y: from.y + len * Math.sin(ang) };
}

export function canMove(a: Annot): boolean {
  const t = a.body.type;
  return !["highlight", "underline", "strikeOut"].includes(t) && !(t === "redact" && a.body.quads.length > 1);
}

export function canResize(a: Annot): boolean {
  return canMove(a) && !["note", "other"].includes(a.body.type);
}

export function keepsAspect(a: Annot): boolean {
  return a.body.type === "image" || a.body.type === "check";
}

/** Annotation déplacée de (dx, dy). */
export function moved(a: Annot, dx: number, dy: number): Annot {
  const shift = (r: Rect): Rect => ({ ...r, x: r.x + dx, y: r.y + dy });
  const b = a.body;
  const body =
    b.type === "line"
      ? { ...b, from: { x: b.from.x + dx, y: b.from.y + dy }, to: { x: b.to.x + dx, y: b.to.y + dy } }
      : "quads" in b
        ? { ...b, quads: b.quads.map(shift) }
        : b;
  return { ...a, rect: shift(a.rect), body } as Annot;
}

/** Annotation redimensionnée en tirant une poignée jusqu'à `p`. */
export function resized(a: Annot, handle: Handle, p: Point, keepAspect = keepsAspect(a)): Annot {
  const b = a.body;
  if (b.type === "line") {
    return { ...a, body: handle === "from" ? { ...b, from: p } : { ...b, to: p } };
  }
  const r = a.rect;
  let x0 = r.x;
  let y0 = r.y;
  let x1 = r.x + r.w;
  let y1 = r.y + r.h;
  if (handle.includes("w")) x0 = Math.min(p.x, x1 - 4);
  if (handle.includes("e")) x1 = Math.max(p.x, x0 + 4);
  if (handle.includes("n")) y0 = Math.min(p.y, y1 - 4);
  if (handle.includes("s")) y1 = Math.max(p.y, y0 + 4);
  if (keepAspect && handle.length === 2 && r.w > 0 && r.h > 0) {
    const ratio = r.w / r.h;
    const w = x1 - x0;
    const h = w / ratio;
    if (handle.includes("n")) y0 = y1 - h;
    else y1 = y0 + h;
  }
  const rect = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
  const body = b.type === "redact" ? { ...b, quads: [rect] } : b;
  return { ...a, rect, body } as Annot;
}

export function handlePos(r: Rect, h: Handle): Point {
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  switch (h) {
    case "nw": return { x: r.x, y: r.y };
    case "n": return { x: cx, y: r.y };
    case "ne": return { x: r.x + r.w, y: r.y };
    case "e": return { x: r.x + r.w, y: cy };
    case "se": return { x: r.x + r.w, y: r.y + r.h };
    case "s": return { x: cx, y: r.y + r.h };
    case "sw": return { x: r.x, y: r.y + r.h };
    case "w": return { x: r.x, y: cy };
    default: return { x: cx, y: cy };
  }
}

/** Zones cliquables d'une annotation (une par ligne de texte pour le marquage). */
export function hitRects(a: Annot): Rect[] {
  return "quads" in a.body && a.body.quads.length ? a.body.quads : [a.rect];
}

/** Titre de liste : « Flèche » pour une ligne fléchée. */
export function kindKey(a: Annot): string {
  if (a.body.type === "line" && a.body.arrow) return "arrow";
  return a.body.type;
}

/** Heure d'une date PDF « D:AAAAMMJJHHmmSS… » (UTC si « Z »). */
export function pdfDateTime(d: string | null, locale: string): string | null {
  const m = d?.match(/^D:(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})?(Z|[+-]\d{2}'?\d{2}'?)?/);
  if (!m) return null;
  const [, y, mo, da, h, mi, s, tz] = m;
  let ms = Date.UTC(+y, +mo - 1, +da, +h, +mi, +(s ?? 0));
  if (tz && tz !== "Z") {
    const sign = tz[0] === "-" ? -1 : 1;
    const [th, tm] = tz.slice(1).replace(/'/g, "").match(/\d{2}/g)!.map(Number);
    ms -= sign * (th * 60 + (tm ?? 0)) * 60000;
  } else if (!tz) {
    ms = new Date(+y, +mo - 1, +da, +h, +mi, +(s ?? 0)).getTime();
  }
  const date = new Date(ms);
  const today = new Date();
  const sameDay = date.toDateString() === today.toDateString();
  return new Intl.DateTimeFormat(locale, sameDay ? { hour: "2-digit", minute: "2-digit" } : { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }).format(date);
}
