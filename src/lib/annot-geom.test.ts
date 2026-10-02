import { describe, expect, it } from "vitest";
import type { Annot } from "../bindings";
import { canCopy, canMove, canResize, handlePos, hitRects, kindKey, moved, pasted, pdfDateTime, rectFrom, resized, snap45 } from "./annot-geom";

const base = (body: Annot["body"], rect = { x: 10, y: 20, w: 100, h: 50 }): Annot => ({
  id: "a", page: 0, rect, color: "#e03131", opacity: 1, width: 2, contents: null, author: null, modified: null, excerpt: null, body, hidden: false,
});

describe("géométrie des annotations", () => {
  it("rectangle entre deux points", () => {
    expect(rectFrom({ x: 50, y: 40 }, { x: 10, y: 60 })).toEqual({ x: 10, y: 40, w: 40, h: 20 });
  });
  it("contrainte à 45°", () => {
    const p = snap45({ x: 0, y: 0 }, { x: 10, y: 9 });
    expect(p.x).toBeCloseTo(p.y);
  });
  it("déplacement : rectangle, ligne et zones", () => {
    const l = moved(base({ type: "line", from: { x: 0, y: 0 }, to: { x: 10, y: 10 }, arrow: true }), 5, 5);
    expect(l.body).toMatchObject({ from: { x: 5, y: 5 }, to: { x: 15, y: 15 } });
    expect(l.rect.x).toBe(15);
    const r = moved(base({ type: "redact", quads: [{ x: 10, y: 20, w: 100, h: 50 }] }), -10, 0);
    expect(r.body).toEqual({ type: "redact", quads: [{ x: 0, y: 20, w: 100, h: 50 }] });
  });
  it("redimensionnement par poignée, proportions des images", () => {
    expect(resized(base({ type: "square" }), "se", { x: 210, y: 220 }).rect).toEqual({ x: 10, y: 20, w: 200, h: 200 });
    expect(resized(base({ type: "square" }), "nw", { x: 500, y: 500 }).rect.w).toBe(4);
    const img = resized(base({ type: "image", image: "k" }), "se", { x: 210, y: 0 });
    expect(img.rect).toEqual({ x: 10, y: 20, w: 200, h: 100 });
    expect(handlePos({ x: 0, y: 0, w: 10, h: 20 }, "s")).toEqual({ x: 5, y: 20 });
  });
  it("droits de déplacement et zones cliquables", () => {
    const hl = base({ type: "highlight", quads: [{ x: 0, y: 0, w: 5, h: 5 }, { x: 0, y: 10, w: 5, h: 5 }] });
    expect(canMove(hl)).toBe(false);
    expect(hitRects(hl)).toHaveLength(2);
    expect(canResize(base({ type: "note" }))).toBe(false);
    expect(canMove(base({ type: "note" }))).toBe(true);
    expect(kindKey(base({ type: "line", from: { x: 0, y: 0 }, to: { x: 1, y: 1 }, arrow: true }))).toBe("arrow");
  });
  it("dates PDF", () => {
    expect(pdfDateTime(null, "fr")).toBeNull();
    expect(pdfDateTime("D:20200102030405Z", "fr")).toMatch(/2 janv\.?/);
  });
});

describe("copier-coller", () => {
  it("copiable : ni marquage ni annotation externe", () => {
    expect(canCopy(base({ type: "square" }))).toBe(true);
    expect(canCopy(base({ type: "highlight", quads: [] }))).toBe(false);
    expect(canCopy(base({ type: "other", subtype: "Ink" }))).toBe(false);
    expect(canCopy(base({ type: "image", image: "ap" }))).toBe(false);
    expect(canCopy(base({ type: "image", image: "img1" }))).toBe(true);
  });
  it("collée : nouvel identifiant, décalée, gardée dans la page", () => {
    const a = { ...base({ type: "line", from: { x: 10, y: 20 }, to: { x: 110, y: 70 }, arrow: false }), author: "X", modified: "D:2020" };
    const p = pasted(a, 3, 600, 800, 12);
    expect(p).toMatchObject({ page: 3, author: null, modified: null, rect: { x: 22, y: 32 }, body: { from: { x: 22, y: 32 } } });
    expect(p.id).not.toBe(a.id);
    const edge = pasted(a, 0, 115, 75, 12);
    expect(edge.rect).toMatchObject({ x: 15, y: 25 });
    expect(edge.body).toMatchObject({ to: { x: 115, y: 75 } });
  });
});
