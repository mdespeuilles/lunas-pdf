import { describe, expect, it } from "vitest";
import { GAP, PAD_SIDE, PAD_TOP, PT_TO_PX, computeLayout, currentPage, fitZoom, rows, scrollTarget, tilesFor, visiblePages, zoomStep } from "./layout";

const A4 = { width: 595.28, height: 841.89 };
const pages = (n: number) => Array.from({ length: n }, () => A4);

describe("rows", () => {
  it("groupe les pages par mode", () => {
    expect(rows(5, "continuous", 0)).toEqual([[0], [1], [2], [3], [4]]);
    expect(rows(5, "double", 0)).toEqual([[0, 1], [2, 3], [4]]);
    expect(rows(5, "single", 3)).toEqual([[3]]);
    expect(rows(5, "single", 99)).toEqual([[4]]);
  });
});

describe("computeLayout", () => {
  it("empile les pages en continu, centrées", () => {
    const l = computeLayout(pages(3), "continuous", 1, 1200);
    const w = Math.round(A4.width * PT_TO_PX);
    const h = Math.round(A4.height * PT_TO_PX);
    expect(l.boxes[0]).toEqual({ index: 0, x: Math.round((1200 - w) / 2), y: PAD_TOP, w, h });
    expect(l.boxes[1].y).toBe(PAD_TOP + h + GAP);
    expect(l.width).toBe(1200);
  });

  it("élargit le contenu quand la page dépasse la vue (zoom fort)", () => {
    const l = computeLayout(pages(1), "continuous", 4, 800);
    expect(l.width).toBe(Math.round(A4.width * 4 * PT_TO_PX) + 2 * PAD_SIDE);
    expect(l.boxes[0].x).toBe(PAD_SIDE);
  });

  it("place deux pages côte à côte en double page", () => {
    const l = computeLayout(pages(3), "double", 1, 2000);
    expect(l.boxes[0].y).toBe(l.boxes[1].y);
    expect(l.boxes[1].x).toBeGreaterThan(l.boxes[0].x + l.boxes[0].w);
    expect(l.boxes[2].y).toBeGreaterThan(l.boxes[0].y);
  });
});

describe("fitZoom", () => {
  it("ajuste à la largeur sans débordement", () => {
    const z = fitZoom(pages(2), "continuous", "width", 1000, 800);
    const l = computeLayout(pages(2), "continuous", z, 1000);
    expect(l.width).toBe(1000);
  });
  it("ajuste à la page : la page tient en hauteur", () => {
    const z = fitZoom(pages(2), "continuous", "page", 1600, 700);
    expect(A4.height * z * PT_TO_PX).toBeLessThanOrEqual(700 - PAD_TOP);
  });
});

describe("navigation", () => {
  const l = computeLayout(pages(10), "continuous", 1, 1000);
  it("page courante et pages visibles", () => {
    const b3 = l.boxes[3];
    expect(currentPage(l, b3.y, 800)).toBe(3);
    const vis = visiblePages(l, b3.y, 1400, 0);
    expect(vis[0]).toBe(3);
    expect(vis).toContain(4);
    expect(vis).not.toContain(0);
  });
  it("cible de défilement", () => {
    expect(scrollTarget(l, 2, 1)).toBe(l.boxes[2].y - PAD_TOP);
    expect(scrollTarget(l, 42, 1)).toBeNull();
  });
});

describe("zoomStep et tuiles", () => {
  it("paliers de zoom", () => {
    expect(zoomStep(1, 1)).toBe(1.1);
    expect(zoomStep(1, -1)).toBe(0.9);
    expect(zoomStep(4, 1)).toBe(4);
    expect(zoomStep(1.37, 1)).toBe(1.5);
  });
  it("découpe la zone visible en tuiles de 1024 px", () => {
    const t = tilesFor(3000, 2500, { x0: 1000, y0: 0, x1: 2100, y1: 900 });
    expect(t).toEqual([
      { x: 0, y: 0, w: 1024, h: 1024 },
      { x: 1024, y: 0, w: 1024, h: 1024 },
      { x: 2048, y: 0, w: 952, h: 1024 },
    ]);
  });
});
