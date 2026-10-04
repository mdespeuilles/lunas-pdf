import { describe, expect, it } from "vitest";
import { macKeys } from "./keys";

describe("raccourcis en notation macOS", () => {
  it("Ctrl, Maj, Shift", () => {
    expect(macKeys("Annoter (Ctrl Maj A)")).toBe("Annoter (⌘⇧A)");
    expect(macKeys("Annotate (Ctrl Shift A)")).toBe("Annotate (⌘⇧A)");
    expect(macKeys("Zoom avant (Ctrl +)")).toBe("Zoom avant (⌘+)");
    expect(macKeys("Ctrl Z les annule ; rien n’est enregistré")).toBe("⌘Z les annule ; rien n’est enregistré");
  });
  it("Entrée, Tab, Suppr seulement dans les raccourcis", () => {
    expect(macKeys("Résultat précédent (Maj Entrée)")).toBe("Résultat précédent (⇧↩)");
    expect(macKeys("Next result (Enter)")).toBe("Next result (↩)");
    expect(macKeys("Maj Tab")).toBe("⇧Tab");
    expect(macKeys("Supprimer (Suppr)")).toBe("Supprimer (⌫)");
    expect(macKeys("Enter a password")).toBe("Enter a password");
    expect(macKeys("Supprimer la signature")).toBe("Supprimer la signature");
    expect(macKeys("Barre latérale (F9)")).toBe("Barre latérale (F9)");
  });
});
