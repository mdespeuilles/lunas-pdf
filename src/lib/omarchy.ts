// Couleurs de l'app tirées du thème d'Omarchy (`colors.toml`, formats récent et antérieur) :
// mêmes rôles que tokens.css (fonds, surfaces, textes, accent, états), dérivés de la palette.
import type { OmarchyTheme } from "../bindings";

/** Luminance relative (0 noir, 1 blanc) d'une couleur « #rgb » ou « #rrggbb ». */
export function luminance(hex: string): number {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h;
  const [r, g, b] = [0, 2, 4].map((i) => {
    const v = parseInt(full.slice(i, i + 2), 16) / 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Rapport de contraste WCAG entre deux couleurs. */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const mix = (a: string, pa: number, b: string) => `color-mix(in oklab, ${a} ${pa}%, ${b})`;
const tint = (c: string, p: number) => `color-mix(in oklab, ${c} ${p}%, transparent)`;

/** « tokyo-night » → « Tokyo Night ». */
export function themeLabel(name: string): string {
  return name
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1))
    .join(" ");
}

export interface OmarchyStyle {
  dark: boolean;
  vars: Record<string, string>;
}

export function omarchyStyle(theme: OmarchyTheme): OmarchyStyle | null {
  const c = theme.colors as Record<string, string | undefined>;
  const pick = (...keys: string[]) => keys.map((k) => c[k]).find((v): v is string => !!v);
  const bg = pick("background");
  const fg = pick("foreground");
  if (!bg || !fg) return null;
  const dark = theme.dark ?? luminance(bg) < 0.4;
  const accent = pick("accent", "blue", "color4") ?? "#3466f6";
  const lighter = pick("lighter_background");
  const darkBg = pick("dark_background") ?? mix(bg, dark ? 80 : 94, "black");
  const darker = pick("darker_background") ?? mix(bg, dark ? 65 : 88, "black");
  const red = pick("red", "color1") ?? "#c02a2a";
  const green = pick("green", "color2") ?? "#167a3d";
  const yellow = pick("orange", "yellow", "color3") ?? "#9a5300";

  // Surfaces (onglet actif, dialogues, cartes) : plus claires que le fond, dans les deux modes.
  const lightest = lighter && luminance(lighter) > luminance(bg) ? lighter : bg;
  const text = dark ? pick("bright_foreground", "color15") ?? fg : fg;
  const vars: Record<string, string> = dark
    ? {
        "--bg": bg,
        "--canvas": darker,
        "--canvas-2": darker,
        "--sidebar": lighter ? mix(bg, 75, lighter) : mix(bg, 94, fg),
        "--chrome": lighter ? mix(bg, 45, lighter) : mix(bg, 90, fg),
        "--surface": lightest === bg ? mix(bg, 86, fg) : lightest,
        "--surface-2": mix(lightest === bg ? mix(bg, 86, fg) : lightest, 86, fg),
        "--tip-bg": mix(lightest, 80, fg),
        "--tip-fg": text,
      }
    : {
        "--bg": lightest,
        "--canvas": darker,
        "--canvas-2": darker,
        "--sidebar": darkBg,
        "--chrome": mix(lightest, 55, darkBg),
        "--surface": mix(lightest, 55, "white"),
        "--surface-2": mix(lightest, 70, darkBg),
        "--tip-bg": text,
        "--tip-fg": lightest,
      };
  Object.assign(vars, {
    "--text": text,
    "--text-2": mix(text, 78, bg),
    "--text-3": mix(text, 62, bg),
    "--line": tint(fg, dark ? 14 : 16),
    "--line-2": tint(fg, dark ? 24 : 28),
    "--hover": tint(fg, dark ? 9 : 8),
    "--press": tint(fg, dark ? 15 : 13),
    "--accent": accent,
    // Texte sur l'accent : blanc, ou foncé si l'accent est clair (jaune, vert pâle…).
    "--on-accent": contrast(accent, "#ffffff") >= contrast(accent, dark ? bg : text) ? "#fff" : dark ? bg : text,
    "--ok": green,
    "--warn": yellow,
    "--bad": red,
    "--ok-bg": tint(green, 15),
    "--warn-bg": tint(yellow, 15),
    "--bad-bg": tint(red, 15),
  });
  return { dark, vars };
}
