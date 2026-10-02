// Signatures numériques : état d'ensemble d'un document et mise en forme des détails.
import type { SignatureInfo } from "../bindings";

export type SigLevel = "ok" | "bad" | "warn";

/** Niveau du bandeau : une signature invalide l'emporte, puis une non vérifiable. */
export function sigLevel(list: SignatureInfo[]): SigLevel {
  if (list.some((s) => s.status === "invalid")) return "bad";
  if (list.some((s) => s.status === "unknown")) return "warn";
  return "ok";
}

/** Signataires, dans l'ordre, sans doublon. */
export function signers(list: SignatureInfo[]): string[] {
  return [...new Set(list.map((s) => s.signer ?? "").filter(Boolean))];
}

/** Date « 28 sept. 2026, 14:32 (UTC+2) » dans le fuseau déclaré par le signataire. */
export function sigDate(seconds: number | null, offset: number | null, locale: string): string {
  if (seconds === null) return "";
  if (offset === null) return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(seconds * 1000);
  const text = new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short", timeZone: "UTC" }).format((seconds + offset * 60) * 1000);
  const h = Math.trunc(Math.abs(offset) / 60);
  const m = Math.abs(offset) % 60;
  return `${text} (UTC${offset < 0 ? "−" : "+"}${h}${m ? ":" + String(m).padStart(2, "0") : ""})`;
}

export function shortDate(seconds: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "short" }).format(seconds * 1000);
}

/** Empreinte abrégée « 4F:2A:91:C3 … 7C:0E ». */
export function shortFingerprint(fp: string): string {
  const p = fp.split(":");
  return p.length > 6 ? `${p.slice(0, 4).join(":")} … ${p.slice(-2).join(":")}` : fp;
}
