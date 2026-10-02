// Plages de pages saisies (« 1-3, 5, 8- ») : même règle que `export::parse_range` côté Rust.

/** Indices (0…n-1) dans l'ordre, sans doublon ; `null` si la saisie est invalide. */
export function parseRange(spec: string, count: number): number[] | null {
  const out: number[] = [];
  const parts = spec.split(/[,;]/).map((p) => p.trim()).filter(Boolean);
  for (const part of parts) {
    const m = part.match(/^(\d*)\s*[-–]\s*(\d*)$/);
    let a: number;
    let b: number;
    if (m) {
      a = m[1] ? Number(m[1]) : 1;
      b = m[2] ? Number(m[2]) : count;
    } else if (/^\d+$/.test(part)) {
      a = b = Number(part);
    } else {
      return null;
    }
    if (a < 1 || b < 1 || a > b || b > count) return null;
    for (let p = a; p <= b; p++) if (!out.includes(p - 1)) out.push(p - 1);
  }
  return out.length ? out : null;
}

/** Solidité d'un mot de passe : 0 (vide ou trop court) à 4. */
export function passwordStrength(pw: string): number {
  if (pw.length < 6) return 0;
  let s = 1;
  if (pw.length >= 10) s++;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) s++;
  if (/\d/.test(pw) && /[^A-Za-z0-9]/.test(pw)) s++;
  if (pw.length >= 16) s++;
  return Math.min(4, s);
}

/** Taille lisible : « 1,8 Mo ». */
export function formatSize(bytes: number, locale: string): string {
  const units = ["o", "Ko", "Mo", "Go"];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  const digits = u === 0 || v >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(v)} ${units[u]}`;
}
