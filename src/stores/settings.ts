import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import { events, type OmarchyTheme, type Settings as RawSettings } from "../bindings";
import { omarchyStyle } from "../lib/omarchy";
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
  highlightFields: true,
  signedOk: [],
  aiAgent: "none",
  aiPath: "",
  aiModel: "",
};

/** Accents proposés par le design (props de la planche). */
export const ACCENTS = ["#3466f6", "#7048e8", "#e8590c", "#0c8a6b"];

export const useSettings = defineStore("settings", () => {
  const settings = ref<Settings>({ ...DEFAULTS });
  const systemLocale = ref("fr");
  const systemDark = ref(window.matchMedia("(prefers-color-scheme: dark)").matches);
  const loaded = ref(false);

  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => (systemDark.value = e.matches));

  /** Thème d'Omarchy (Linux), suivi quand le thème est « Système ». */
  const omarchy = ref<OmarchyTheme | null>(null);
  const omarchyApplied = computed(() => (settings.value.theme === "system" && omarchy.value ? omarchyStyle(omarchy.value) : null));
  const dark = computed(() =>
    omarchyApplied.value ? omarchyApplied.value.dark : settings.value.theme === "system" ? systemDark.value : settings.value.theme === "dark",
  );
  const locale = computed(() => resolveLocale(settings.value.language, systemLocale.value));
  let inlineVars: string[] = [];

  function apply() {
    const root = document.documentElement;
    root.classList.toggle("dark", dark.value);
    // Couleurs d'Omarchy par-dessus les tokens (styles en ligne), retirées sinon.
    for (const v of inlineVars) root.style.removeProperty(v);
    const vars = omarchyApplied.value?.vars ?? { "--accent": settings.value.accent };
    for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
    inlineVars = Object.keys(vars);
    root.style.colorScheme = dark.value ? "dark" : "light";
    setLocale(locale.value);
  }

  async function load() {
    if (inTauri || "__LUNAS_PDF_E2E__" in window) {
      settings.value = { ...DEFAULTS, ...(await commands.getSettings()) } as Settings;
      systemLocale.value = await commands.systemLocale();
      omarchy.value = await commands.getOmarchyTheme();
      await events.omarchyThemeEvent.listen((e) => (omarchy.value = e.payload.theme));
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

  watch([dark, locale, () => settings.value.accent, omarchyApplied], apply);

  return { settings, dark, locale, loaded, load, update, omarchy, omarchyApplied };
});
