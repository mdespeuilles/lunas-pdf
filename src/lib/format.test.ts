import { describe, expect, it } from "vitest";
import { percent, relativeDate } from "./format";
import { i18n, resolveLocale } from "../i18n";

describe("dates relatives (planche 01)", () => {
  const now = new Date(2026, 9, 2, 15, 0).getTime();
  it("en français", () => {
    i18n.global.locale.value = "fr";
    expect(relativeDate(now - 20e3, "fr", now)).toBe("À l’instant");
    expect(relativeDate(now - 5 * 60e3, "fr", now)).toBe("Il y a 5 min");
    expect(relativeDate(now - 2 * 3600e3, "fr", now)).toBe("Il y a 2 h");
    expect(relativeDate(new Date(2026, 9, 1, 10).getTime(), "fr", now)).toBe("Hier");
    expect(relativeDate(new Date(2026, 8, 28, 10).getTime(), "fr", now)).toBe("Lundi");
    expect(relativeDate(new Date(2026, 8, 14, 10).getTime(), "fr", now)).toMatch(/^14 sept\.?$/);
  });
  it("en anglais", () => {
    i18n.global.locale.value = "en";
    expect(relativeDate(now - 2 * 3600e3, "en", now)).toBe("2 h ago");
    expect(relativeDate(new Date(2026, 9, 1, 10).getTime(), "en", now)).toBe("Yesterday");
  });
});

describe("pourcentages et langue", () => {
  it("formate le zoom", () => {
    expect(percent(1.25, "fr").replace(/\s/g, " ")).toBe("125 %");
    expect(percent(1.25, "en")).toBe("125%");
  });
  it("choisit la langue : préférence, sinon OS, sinon anglais", () => {
    expect(resolveLocale("en", "fr-FR")).toBe("en");
    expect(resolveLocale("system", "fr-CA")).toBe("fr");
    expect(resolveLocale("system", "de-DE")).toBe("en");
  });
});

import { externalUrl } from "./window";

describe("liens externes", () => {
  it("schémas autorisés, adresses sans schéma", () => {
    expect(externalUrl("https://exemple.fr/a")).toBe("https://exemple.fr/a");
    expect(externalUrl(" mailto:a@b.fr")).toBe("mailto:a@b.fr");
    expect(externalUrl("www.mma.fr/contrat")).toBe("https://www.mma.fr/contrat");
    expect(externalUrl("file:///etc/passwd")).toBeNull();
    expect(externalUrl("javascript:alert(1)")).toBeNull();
    expect(externalUrl("#page=3")).toBeNull();
  });
});
