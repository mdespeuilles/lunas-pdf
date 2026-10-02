import { describe, expect, it } from "vitest";
import { fitTextBox, textWidth, winansi, wrap } from "./afm";

describe("métriques des polices standard", () => {
  it("largeurs Helvetica (AFM)", () => {
    // « Montant » en Helvetica : M 833, o 556, n 556, t 278, a 556, n 556, t 278 = 3613.
    expect(textWidth("Montant", "sans", 1000)).toBe(3613);
    expect(textWidth("iii", "mono", 10)).toBe(18);
  });
  it("WinAnsi : accents et caractères spéciaux", () => {
    expect(winansi("é")).toBe(0xe9);
    expect(winansi("€")).toBe(0x80);
    expect(winansi("œ")).toBe(0x9c);
    expect(winansi("漢")).toBe(63);
  });
  it("coupure identique à l'apparence Rust", () => {
    const lines = wrap("Montant à confirmer avec la direction financière", "sans", 12, 120);
    expect(lines.length).toBeGreaterThanOrEqual(2);
    for (const l of lines) expect(textWidth(l, "sans", 12)).toBeLessThanOrEqual(120);
    expect(wrap("a\n\nb", "sans", 12, 100)).toEqual(["a", "", "b"]);
  });
  it("zone ajustée au texte", () => {
    const one = fitTextBox("km", "sans", 12, 500);
    expect(one.h).toBeCloseTo(12 * 1.2 + 4);
    expect(one.w).toBeCloseTo(textWidth("km", "sans", 12) + 5);
    expect(fitTextBox("ligne 1\nligne 2", "sans", 12, 500).h).toBeCloseTo(2 * 14.4 + 4);
    // Bornée par la largeur disponible : le texte revient à la ligne.
    const long = fitTextBox("mot ".repeat(40).trim(), "sans", 12, 150);
    expect(long.w).toBe(150);
    expect(long.h).toBeGreaterThan(3 * 14.4);
  });
});
