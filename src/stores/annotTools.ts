import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { CheckStyle, FontFamily } from "../bindings";

/** Outils de la barre d'annotation (planche 03) et leurs raccourcis. */
export type Tool = "select" | "highlight" | "underline" | "strike" | "text" | "note" | "rect" | "ellipse" | "line" | "arrow" | "check" | "signature" | "image" | "redact";

export const TOOL_KEYS: Record<string, Tool> = {
  v: "select",
  h: "highlight",
  u: "underline",
  k: "strike",
  t: "text",
  n: "note",
  r: "rect",
  o: "ellipse",
  l: "line",
  a: "arrow",
  c: "check",
  s: "signature",
  i: "image",
  x: "redact",
};

/** Palette de la planche 03 (ordre et teintes du design). */
export const PALETTE = [
  { key: "black", color: "#1b1b20", highlight: "#ced4da" },
  { key: "red", color: "#e03131", highlight: "#ffa8a8" },
  { key: "blue", color: "#1f6fe0", highlight: "#a5d8ff" },
  { key: "green", color: "#2f9e44", highlight: "#b2f2bb" },
  { key: "yellow", color: "#f59f00", highlight: "#ffd43b" },
  { key: "pink", color: "#e64980", highlight: "#fcc2d7" },
] as const;
export type PaletteKey = (typeof PALETTE)[number]["key"];

export const WIDTHS = [1, 2, 3, 5];
export const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 24, 32, 48];

export function colorFor(key: PaletteKey, tool: Tool): string {
  const p = PALETTE.find((c) => c.key === key)!;
  return tool === "highlight" ? p.highlight : p.color;
}

/** Clé de palette la plus proche d'une couleur (pour l'état des pastilles). */
export function paletteKeyOf(hex: string): PaletteKey | null {
  const h = hex.toLowerCase();
  return PALETTE.find((p) => p.color === h || p.highlight === h)?.key ?? null;
}

export const useAnnotTools = defineStore("annotTools", () => {
  const tool = ref<Tool>("select");
  /** Couleur choisie par famille d'outils (texte, marquage, formes…). */
  const colors = ref<Record<string, PaletteKey>>({ highlight: "yellow", underline: "blue", strike: "red", text: "black", note: "yellow", shape: "red", check: "blue" });
  const width = ref(2);
  const font = ref<FontFamily>("sans");
  const size = ref(12);
  const checkStyle = ref<CheckStyle>("check");
  /** Outil demandé au clavier qui nécessite la barre (image, signature). */
  const request = ref<{ tool: Tool; seq: number } | null>(null);

  const family = computed(() => familyOf(tool.value));
  const colorKey = computed<PaletteKey>({
    get: () => colors.value[family.value] ?? "red",
    set: (k) => (colors.value = { ...colors.value, [family.value]: k }),
  });
  const color = computed(() => colorFor(colorKey.value, tool.value));

  return { tool, colors, width, font, size, checkStyle, request, family, colorKey, color };
});

export function familyOf(tool: Tool | string): string {
  if (["rect", "ellipse", "line", "arrow", "square", "circle"].includes(tool)) return "shape";
  if (tool === "freeText") return "text";
  if (tool === "strikeOut") return "strike";
  return tool;
}
