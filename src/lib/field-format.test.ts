import { describe, expect, it } from "vitest";
import type { FieldFormat, FormField, RangeRule } from "../bindings";
import { allowInput, editText, makeNumber, numberText, parseInput } from "./field-format";

const field = (format: FieldFormat | null, range: RangeRule | null = null): FormField => ({
  id: "f", label: null, kind: { type: "text", multiline: false, password: false, comb: false, maxLen: null }, value: [""], defaultValue: [""],
  readOnly: false, required: false, fontSize: 0, align: 0, font: "sans", widgets: [], format, range, calc: null, customScript: false,
});
const eur: FieldFormat = { type: "number", decimals: 2, sepStyle: 2, negStyle: 0, currency: " €", prepend: false };
const usd: FieldFormat = { type: "number", decimals: 2, sepStyle: 0, negStyle: 0, currency: "$", prepend: true };
const now = new Date(2026, 9, 2);

describe("champs à format", () => {
  it("nombres : séparateurs, monnaie, parenthèses", () => {
    expect(parseInput(field(eur), "1.234,5 €")).toEqual({ ok: true, value: "1234.5" });
    expect(parseInput(field(eur), "12,50")).toEqual({ ok: true, value: "12.5" });
    expect(parseInput(field(usd), "$1,234.56")).toEqual({ ok: true, value: "1234.56" });
    expect(parseInput(field(usd), "(12)")).toEqual({ ok: true, value: "-12" });
    expect(parseInput(field(usd), "12,5")).toEqual({ ok: true, value: "12.5" });
    expect(parseInput(field(eur), "douze").ok).toBe(false);
    expect(parseInput(field(eur), "  ")).toEqual({ ok: true, value: "" });
    expect(editText(field(eur), "1234.5")).toBe("1234,5");
  });
  it("pourcentages : saisis en %, stockés en fraction", () => {
    const pct: FieldFormat = { type: "percent", decimals: 1, sepStyle: 3 };
    expect(parseInput(field(pct), "15,5")).toEqual({ ok: true, value: "0.155" });
    expect(parseInput(field(pct), "15,5 %")).toEqual({ ok: true, value: "0.155" });
    expect(editText(field(pct), "0.155")).toBe("15,5");
  });
  it("dates et heures", () => {
    const d: FieldFormat = { type: "date", format: "dd/mm/yyyy" };
    expect(parseInput(field(d), "2/10/26", now)).toEqual({ ok: true, value: "02/10/2026" });
    expect(parseInput(field(d), "31/02/2026", now).ok).toBe(false);
    expect(parseInput(field(d), "2/10", now)).toEqual({ ok: true, value: "02/10/2026" });
    expect(parseInput(field({ type: "date", format: "mmm d, yyyy" }), "oct 2 2026", now)).toEqual({ ok: true, value: "Oct 2, 2026" });
    expect(parseInput(field({ type: "date", format: "d-mmm-yy" }), "2 février 2027", now)).toEqual({ ok: true, value: "2-Feb-27" });
    expect(parseInput(field({ type: "time", format: "HH:MM" }), "3:05 pm", now)).toEqual({ ok: true, value: "15:05" });
    expect(parseInput(field({ type: "time", format: "h:MM tt" }), "15h05", now)).toEqual({ ok: true, value: "3:05 pm" });
    expect(parseInput(field({ type: "time", format: "HH:MM" }), "25:00", now).ok).toBe(false);
  });
  it("formats spéciaux et masques", () => {
    expect(parseInput(field({ type: "special", kind: 2 }), "(061) 234-5678")).toEqual({ ok: true, value: "0612345678" });
    expect(parseInput(field({ type: "special", kind: 0 }), "1234").ok).toBe(false);
    expect(parseInput(field({ type: "mask", mask: "AA-999" }), "ab123")).toEqual({ ok: true, value: "ab-123" });
    expect(parseInput(field({ type: "mask", mask: "AA-999" }), "a1-123").ok).toBe(false);
  });
  it("plage autorisée", () => {
    const f = field(eur, { min: 0, max: 10000 });
    expect(parseInput(f, "10000").ok).toBe(true);
    expect(parseInput(f, "10001")).toMatchObject({ ok: false, error: "range" });
    expect(parseInput(field(null, { min: 1, max: null }), "0")).toMatchObject({ ok: false, error: "range" });
  });
  it("frappe filtrée", () => {
    expect(allowInput(field(eur), "12,5 €")).toBe(true);
    expect(allowInput(field(eur), "a")).toBe(false);
    expect(allowInput(field({ type: "special", kind: 2 }), "06 12")).toBe(true);
    expect(allowInput(field(null), "a")).toBe(true);
  });
  it("nombres en texte", () => {
    expect(numberText(0.1 + 0.2)).toBe("0.3");
    expect(numberText(-0)).toBe("0");
    expect(makeNumber("1 234,5")).toBe(1234.5);
  });
});
