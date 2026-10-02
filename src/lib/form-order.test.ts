import { describe, expect, it } from "vitest";
import type { FormField } from "../bindings";
import { fillable, stops } from "../composables/form";

const f = (id: string, page: number, x: number, y: number, extra: Partial<FormField> = {}): FormField => ({
  id, label: null, kind: { type: "text", multiline: false, password: false, comb: false, maxLen: null }, value: [""], defaultValue: [""],
  readOnly: false, required: false, fontSize: 0, align: 0, font: "sans", format: null, range: null, calc: null, customScript: false, widgets: [{ page, rect: { x, y, w: 100, h: 20 }, onState: null }], ...extra,
});

describe("ordre de tabulation", () => {
  it("par page, puis en lignes de haut en bas, de gauche à droite", () => {
    const list = [f("ville", 1, 50, 50), f("prenom", 0, 300, 102), f("nom", 0, 50, 100), f("adresse", 0, 50, 140)];
    expect(stops(list).map((s) => s.field.id)).toEqual(["nom", "prenom", "adresse", "ville"]);
  });
  it("lecture seule et signatures exclues ; radio : le bouton coché", () => {
    const radio = f("formule", 0, 50, 10, {
      kind: { type: "radio" }, value: ["b"],
      widgets: [{ page: 0, rect: { x: 50, y: 10, w: 16, h: 16 }, onState: "a" }, { page: 0, rect: { x: 150, y: 10, w: 16, h: 16 }, onState: "b" }],
    });
    const list = [f("ro", 0, 0, 0, { readOnly: true }), f("sig", 0, 0, 200, { kind: { type: "signature" } }), radio];
    expect(fillable(list[0])).toBe(false);
    const s = stops(list);
    expect(s.map((x) => x.field.id)).toEqual(["formule"]);
    expect(s[0].widget.onState).toBe("b");
  });
});
