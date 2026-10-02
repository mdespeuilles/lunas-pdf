import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import type { Settings as RawSettings } from "../bindings";
import { commands, inTauri, unwrap } from "../lib/api";
import { resolveLocale, setLocale } from "../i18n";

/** Les champs sont optionnels côté Rust (valeurs par défaut) mais toujours présents ici. */
export type Settings = Required<RawSettings>;

const DEFAULTS: Settings = {
  theme: "system",
  accent: "#3466f6",
  language: "system",
  windowControls: true,
  recentsView: "grid",
  sidebarOpen: true,
  authorName: "",
};

/** Accents proposés par le design (props de la planche). */
export const ACCENTS = ["#3466f6", "#7048e8", "#e8590c", "#0c8a6b"];

export const useSettings = defineStore("settings", () => {
  const settings = ref<Settings>({ ...DEFAULTS });
  const systemLocale = ref("fr");
  const systemDark = ref(window.matchMedia("(prefers-color-scheme: dark)").matches);
  const loaded = ref(false);

  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => (systemDark.value = e.matches));

  const dark = computed(() => (settings.value.theme === "system" ? systemDark.value : settings.value.theme === "dark"));
  const locale = computed(() => resolveLocale(settings.value.language, systemLocale.value));

  function apply() {
    const root = document.documentElement;
    root.classList.toggle("dark", dark.value);
    root.style.setProperty("--accent", settings.value.accent);
    root.style.colorScheme = dark.value ? "dark" : "light";
    setLocale(locale.value);
  }

  async function load() {
    if (inTauri || "__FEUILLET_E2E__" in window) {
      settings.value = { ...DEFAULTS, ...(await commands.getSettings()) } as Settings;
      systemLocale.value = await commands.systemLocale();
    } else {
      systemLocale.value = navigator.language;
    }
    apply();
    loaded.value = true;
  }

  async function update(patch: Partial<Settings>) {
    settings.value = { ...settings.value, ...patch };
    if (inTauri) await unwrap(commands.setSettings(settings.value));
  }

  watch([dark, locale, () => settings.value.accent], apply);

  return { settings, dark, locale, loaded, load, update };
});
