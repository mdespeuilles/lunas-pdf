// Notation des raccourcis selon le système : « Ctrl Maj S » ailleurs, « ⌘⇧S » sur macOS (les
// raccourcis acceptent Ctrl et ⌘, voir composables/shortcuts.ts).
import { inTauri } from "../lib/api";

/** App lancée sur macOS (hors tests et aperçu navigateur, qui gardent la notation Ctrl). */
export const isMac = inTauri && typeof navigator !== "undefined" && /Macintosh/.test(navigator.userAgent);

/** « Annoter (Ctrl Maj A) » → « Annoter (⌘⇧A) ». Seuls les raccourcis sont touchés. */
export function macKeys(s: string): string {
  return s
    .replace(/Ctrl (?:Maj|Shift) /g, "⌘⇧")
    .replace(/Ctrl /g, "⌘")
    .replace(/\((?:Maj|Shift) (?:Entrée|Enter)\)/g, "(⇧↩)")
    .replace(/\((?:Entrée|Enter)\)/g, "(↩)")
    .replace(/^(?:Maj|Shift) Tab$/, "⇧Tab")
    .replace(/\((?:Maj|Shift) Tab\)/g, "(⇧Tab)")
    .replace(/\((?:Suppr|Del)\)/g, "(⌫)");
}

/** Raccourci écrit dans un composant (`kbd("Ctrl S")`). */
export function kbd(s: string): string {
  return isMac ? macKeys(s) : s;
}

/** Messages avec la notation macOS. */
export function macMessages<T>(m: T): T {
  if (typeof m === "string") return macKeys(m) as T;
  if (Array.isArray(m)) return m.map(macMessages) as T;
  if (m && typeof m === "object") return Object.fromEntries(Object.entries(m).map(([k, v]) => [k, macMessages(v)])) as T;
  return m;
}
