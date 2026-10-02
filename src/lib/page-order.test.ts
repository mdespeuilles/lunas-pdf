import { describe, expect, it } from "vitest";
import { isNoop, movedIndices, reorder } from "./page-order";

describe("ordre des pages", () => {
  it("comme côté Rust", () => {
    const ids = (o: number[]) => o.map((i) => i + 1);
    expect(ids(reorder(5, [3], 1))).toEqual([1, 4, 2, 3, 5]);
    expect(ids(reorder(5, [0, 1], 4))).toEqual([3, 4, 1, 2, 5]);
    expect(ids(reorder(5, [0], 5))).toEqual([2, 3, 4, 5, 1]);
    expect(ids(reorder(5, [1, 2], 2))).toEqual([1, 2, 3, 4, 5]);
    expect(ids(reorder(5, [4, 0], 2))).toEqual([2, 1, 5, 3, 4]);
  });
  it("sélection après déplacement, déplacements sans effet", () => {
    expect(movedIndices(5, [0, 1], 4)).toEqual([2, 3]);
    expect(movedIndices(5, [3], 0)).toEqual([0]);
    expect(isNoop(5, [1, 2], 1)).toBe(true);
    expect(isNoop(5, [1, 2], 3)).toBe(true);
    expect(isNoop(5, [1], 3)).toBe(false);
  });
});
