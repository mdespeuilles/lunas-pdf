import { describe, expect, it } from "vitest";
import { luminance, omarchyStyle, themeLabel } from "./omarchy";

const tokyo = {
  name: "tokyo-night",
  dark: true,
  colors: {
    accent: "#7aa2f7", background: "#1a1b26", dark_background: "#13141c", darker_background: "#0e0e14", lighter_background: "#24283b",
    foreground: "#a9b1d6", bright_foreground: "#c0caf5", red: "#f7768e", yellow: "#e0af68", orange: "#eb927b", green: "#9ece6a",
  },
};
const latte = {
  name: "catppuccin-latte",
  dark: false,
  colors: { accent: "#1e66f5", background: "#eff1f5", dark_background: "#e3e4e8", darker_background: "#d7d8dc", lighter_background: "#dce0e8", foreground: "#4c4f69" },
};

describe("thème Omarchy", () => {
  it("format récent, sombre : fonds et textes du thème", () => {
    const s = omarchyStyle(tokyo)!;
    expect(s.dark).toBe(true);
    expect(s.vars["--bg"]).toBe("#1a1b26");
    expect(s.vars["--canvas"]).toBe("#0e0e14");
    expect(s.vars["--surface"]).toBe("#24283b");
    expect(s.vars["--text"]).toBe("#c0caf5");
    expect(s.vars["--accent"]).toBe("#7aa2f7");
    expect(s.vars["--warn"]).toBe("#eb927b");
    expect(s.vars["--bad"]).toBe("#f7768e");
  });

  it("format récent, clair : surfaces plus claires que le fond", () => {
    const s = omarchyStyle(latte)!;
    expect(s.dark).toBe(false);
    // `lighter_background` de Catppuccin est plus foncé que le fond : le fond reste la base.
    expect(s.vars["--bg"]).toBe("#eff1f5");
    expect(s.vars["--surface"]).toContain("white");
    expect(s.vars["--sidebar"]).toBe("#e3e4e8");
    expect(s.vars["--text"]).toBe("#4c4f69");
  });

  it("format antérieur : couleurs ANSI, mode déduit du fond", () => {
    const s = omarchyStyle({ name: "rose-pine", dark: null, colors: { background: "#faf4ed", foreground: "#575279", color1: "#b4637a", color4: "#286983" } })!;
    expect(s.dark).toBe(false);
    expect(s.vars["--accent"]).toBe("#286983");
    expect(s.vars["--bad"]).toBe("#b4637a");
    expect(omarchyStyle({ name: "x", dark: null, colors: { background: "#101010", foreground: "#eeeeee" } })!.dark).toBe(true);
  });

  it("accent clair : texte foncé dessus ; palette incomplète : ignorée", () => {
    const s = omarchyStyle({ name: "gruvbox", dark: true, colors: { background: "#282828", foreground: "#d4be98", accent: "#d8a657" } })!;
    expect(s.vars["--on-accent"]).toBe("#282828");
    expect(omarchyStyle({ name: "vide", dark: true, colors: { accent: "#ffffff" } })).toBeNull();
  });

  it("utilitaires", () => {
    expect(luminance("#ffffff")).toBeCloseTo(1);
    expect(luminance("#000")).toBeCloseTo(0);
    expect(themeLabel("tokyo-night")).toBe("Tokyo Night");
  });
});
