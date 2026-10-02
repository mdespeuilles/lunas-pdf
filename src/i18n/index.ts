import { createI18n } from "vue-i18n";
import fr from "./fr";
import en from "./en";
import type { LanguagePref } from "../bindings";

export type Locale = "fr" | "en";

export const i18n = createI18n({
  legacy: false,
  locale: "fr",
  fallbackLocale: "fr",
  messages: { fr, en },
});

/** Langue effective : préférence explicite, sinon celle de l'OS (anglais hors français). */
export function resolveLocale(pref: LanguagePref, system: string): Locale {
  if (pref === "fr" || pref === "en") return pref;
  return system.toLowerCase().startsWith("fr") ? "fr" : "en";
}

export function setLocale(locale: Locale) {
  i18n.global.locale.value = locale;
  document.documentElement.lang = locale;
}
