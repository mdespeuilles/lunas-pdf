import { i18n } from "../i18n";

/** « Il y a 2 h », « Hier », « Lundi », « 28 sept. » (planche 01). */
export function relativeDate(ms: number, locale: "fr" | "en", now = Date.now()): string {
  const t = i18n.global.t;
  const diff = now - ms;
  const min = Math.floor(diff / 60000);
  if (min < 1) return t("dates.justNow");
  if (min < 60) return t("dates.minutesAgo", { n: min });
  const d = new Date(ms);
  const today = new Date(now);
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOfDay(today) - startOfDay(d)) / 86400000);
  if (days === 0) return t("dates.hoursAgo", { n: Math.floor(min / 60) });
  if (days === 1) return t("dates.yesterday");
  if (days < 7) {
    const w = new Intl.DateTimeFormat(locale, { weekday: "long" }).format(d);
    return w.charAt(0).toUpperCase() + w.slice(1);
  }
  const sameYear = d.getFullYear() === today.getFullYear();
  return new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: sameYear ? undefined : "numeric" }).format(d);
}

/** « 125 % » (fr, espace fine insécable) ou « 125% » (en). */
export function percent(zoom: number, locale: "fr" | "en"): string {
  return new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 0 }).format(zoom);
}
