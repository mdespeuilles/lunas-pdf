// Saisie dans les champs à format (fonctions standard d'Acrobat, voir form_script.rs) :
// filtrage des caractères, lecture de la valeur saisie, plage autorisée. La mise en forme
// affichée hors focus est celle de l'apparence générée par le moteur.
import type { FieldFormat, FormField } from "../bindings";

export type Parsed = { ok: true; value: string } | { ok: false; error: "number" | "date" | "time" | "special" | "mask" | "range"; params: Record<string, string | number> };

/** Nombre en texte, à la manière de JavaScript (identique à `number_text` côté Rust). */
export function numberText(n: number): string {
  const r = Math.round(n * 1e10) / 1e10;
  const s = String(Object.is(r, -0) ? 0 : r);
  return s.includes("e") ? r.toFixed(10).replace(/\.?0+$/, "") : s;
}

/** Nombre d'une valeur de champ (`AFMakeNumber`). */
export function makeNumber(v: string): number | null {
  let s = v.replace(/[\s'  ]/g, "");
  if (!s) return null;
  s = s.includes(",") && !s.includes(".") ? s.replace(",", ".") : s.replace(/,/g, "");
  if (!/^[-+]?(\d+\.?\d*|\.\d+)(e[-+]?\d+)?$/i.test(s)) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

const commaDecimal = (f: FieldFormat) => (f.type === "number" || f.type === "percent") && (f.sepStyle === 2 || f.sepStyle === 3);

/** Texte proposé à l'édition (valeur brute, virgule décimale si le format l'utilise). */
export function editText(field: FormField, value: string): string {
  const f = field.format;
  if (!f || !value) return value;
  if (f.type === "percent") {
    const n = makeNumber(value);
    if (n === null) return value;
    const t = numberText(n * 100);
    return commaDecimal(f) ? t.replace(".", ",") : t;
  }
  if (f.type === "number" && commaDecimal(f)) return value.replace(".", ",");
  return value;
}

/** Caractères acceptés à la frappe. */
export function allowInput(field: FormField, data: string): boolean {
  const f = field.format;
  if (!f) return true;
  if (f.type === "number" || f.type === "percent") {
    const cur = f.type === "number" ? f.currency.trim() : "";
    return [...data].every((c) => /[\d.,'\s +\-()%]/.test(c) || (cur !== "" && cur.includes(c)));
  }
  if (f.type === "special") return /^[\d\s\-().+]*$/.test(data);
  return true;
}

function parseNumber(text: string, f: FieldFormat | null): number | null {
  let s = text.trim();
  if (f?.type === "number" && f.currency.trim()) s = s.split(f.currency.trim()).join("");
  s = s.replace(/[\s'  %]/g, "");
  let neg = false;
  const paren = s.match(/^\((.*)\)$/);
  if (paren) {
    s = paren[1];
    neg = true;
  }
  if (f && commaDecimal(f)) s = s.replace(/\./g, "").replace(",", ".");
  const n = makeNumber(s);
  return n === null ? null : neg ? -Math.abs(n) : n;
}

// --- Dates et heures (formats d'Acrobat : d dd m mm mmm mmmm yy yyyy H HH h hh MM ss tt) -----

const MONTHS_EN = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
const MONTHS_FR = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];

function tokens(fmt: string): string[] {
  return fmt.match(/yyyy|yy|mmmm|mmm|mm|m|dd|d|HH|H|hh|h|MM|ss|tt|./g) ?? [];
}

function monthOf(word: string): number | null {
  const w = word.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");
  if (w.length < 3) return null;
  for (const list of [MONTHS_EN, MONTHS_FR]) {
    const i = list.findIndex((m) => m.normalize("NFD").replace(/[̀-ͯ]/g, "").startsWith(w.slice(0, 3)));
    if (i >= 0) return i;
  }
  return null;
}

interface Parts {
  y: number;
  mo: number;
  d: number;
  h: number;
  mi: number;
  s: number;
}

/** Lit une date ou une heure selon l'ordre des éléments du format. */
export function parseDateTime(text: string, fmt: string, now = new Date()): Parts | null {
  const parts = text.match(/\d+|[a-zA-Zéûôàèç.]+/g) ?? [];
  const nums = parts.filter((p) => /^\d+$/.test(p)).map(Number);
  const words = parts.filter((p) => !/^\d+$/.test(p)).map((p) => p.replace(/\./g, ""));
  const out: Parts = { y: now.getFullYear(), mo: now.getMonth(), d: 1, h: 0, mi: 0, s: 0 };
  let pm: boolean | null = null;
  let hasDay = false;
  for (const t of tokens(fmt)) {
    if (t === "mmm" || t === "mmmm") {
      const w = words.find((x) => monthOf(x) !== null);
      if (w !== undefined) {
        out.mo = monthOf(w)!;
        words.splice(words.indexOf(w), 1);
      } else if (nums.length) out.mo = nums.shift()! - 1;
      else return null;
    } else if (/^(yyyy|yy|mm|m|dd|d|HH|H|hh|h|MM|ss)$/.test(t)) {
      const n = nums.shift();
      if (n === undefined) {
        if (t === "yyyy" || t === "yy" || t === "ss") continue;
        return null;
      }
      if (t === "yyyy" || t === "yy") out.y = n < 100 ? (n < 50 ? 2000 + n : 1900 + n) : n;
      else if (t === "mm" || t === "m") out.mo = n - 1;
      else if (t === "dd" || t === "d") {
        out.d = n;
        hasDay = true;
      } else if (t === "MM") out.mi = n;
      else if (t === "ss") out.s = n;
      else out.h = n;
    }
  }
  if (nums.length) return null;
  // « am » / « pm » acceptés même si le format est sur 24 h.
  const ampm = words.find((x) => /^(am|pm)$/i.test(x)) ?? (tokens(fmt).includes("tt") ? words.find((x) => /^[ap]$/i.test(x)) : undefined);
  if (ampm) pm = ampm[0].toLowerCase() === "p";
  if (pm !== null && out.h <= 12) out.h = (out.h % 12) + (pm ? 12 : 0);
  const date = new Date(out.y, out.mo, out.d);
  if (out.mo < 0 || out.mo > 11 || (hasDay && date.getDate() !== out.d) || out.h > 23 || out.mi > 59 || out.s > 59) return null;
  return out;
}

export function formatDateTime(p: Parts, fmt: string): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  const h12 = p.h % 12 === 0 ? 12 : p.h % 12;
  const cap = (s: string) => s[0].toUpperCase() + s.slice(1);
  return tokens(fmt)
    .map((t) => {
      switch (t) {
        case "yyyy": return String(p.y);
        case "yy": return pad(p.y % 100);
        case "mmmm": return cap(MONTHS_EN[p.mo]);
        case "mmm": return cap(MONTHS_EN[p.mo].slice(0, 3));
        case "mm": return pad(p.mo + 1);
        case "m": return String(p.mo + 1);
        case "dd": return pad(p.d);
        case "d": return String(p.d);
        case "HH": return pad(p.h);
        case "H": return String(p.h);
        case "hh": return pad(h12);
        case "h": return String(h12);
        case "MM": return pad(p.mi);
        case "ss": return pad(p.s);
        case "tt": return p.h < 12 ? "am" : "pm";
        default: return t;
      }
    })
    .join("");
}

// --- Masques ----------------------------------------------------------------------------------

const SPECIAL_LENGTHS: Record<number, number[]> = { 0: [5], 1: [9], 2: [10, 7], 3: [9] };

function applyMask(text: string, mask: string): string | null {
  const literals = new Set([...mask].filter((c) => !"9AOX".includes(c)));
  const input = [...text].filter((c) => !literals.has(c) && !/\s/.test(c));
  let out = "";
  for (const m of mask) {
    if (!"9AOX".includes(m)) {
      out += m;
      continue;
    }
    const c = input.shift();
    if (c === undefined) return null;
    const ok = m === "9" ? /\d/.test(c) : m === "A" ? /\p{L}/u.test(c) : m === "O" ? /[\p{L}\d]/u.test(c) : true;
    if (!ok) return null;
    out += c;
  }
  return input.length ? null : out;
}

// --- Validation -------------------------------------------------------------------------------

/** Valeur à enregistrer pour un texte saisi, ou l'erreur à signaler. */
export function parseInput(field: FormField, text: string, now = new Date()): Parsed {
  const f = field.format;
  const t = text.trim();
  let value = text;
  if (t === "") value = "";
  else if (f?.type === "number" || f?.type === "percent") {
    const n = parseNumber(t, f);
    if (n === null) return { ok: false, error: "number", params: {} };
    value = numberText(f.type === "percent" ? n / 100 : n);
  } else if (f?.type === "date" || f?.type === "time") {
    const p = parseDateTime(t, f.format, now);
    if (!p) return { ok: false, error: f.type, params: { format: f.format } };
    value = formatDateTime(p, f.format);
  } else if (f?.type === "special") {
    const d = t.replace(/\D/g, "");
    if (!SPECIAL_LENGTHS[f.kind]?.includes(d.length)) return { ok: false, error: "special", params: { n: SPECIAL_LENGTHS[f.kind]?.join(" / ") ?? "" } };
    value = d;
  } else if (f?.type === "mask") {
    const m = applyMask(t, f.mask);
    if (m === null) return { ok: false, error: "mask", params: { mask: f.mask } };
    value = m;
  }
  const r = field.range;
  if (r && value !== "") {
    const n = makeNumber(value);
    if (n === null || (r.min !== null && n < r.min) || (r.max !== null && n > r.max)) {
      return { ok: false, error: "range", params: { min: r.min ?? "", max: r.max ?? "" } };
    }
  }
  return { ok: true, value };
}
