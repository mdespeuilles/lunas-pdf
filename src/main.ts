import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./styles/base.css";
import { bootDone, bootLog } from "./boot";
import { openUrl } from "./lib/window";

bootLog("démarrage : module principal chargé");
// Un lien de l'interface ne remplace jamais l'application : il s'ouvre dans le navigateur.
// (La fenêtre native bloque aussi toute navigation externe, voir src-tauri/src/navigation.rs.)
for (const type of ["click", "auxclick"] as const) {
  document.addEventListener(type, (e) => {
    const a = (e.target as HTMLElement | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (!a || e.defaultPrevented) return;
    const href = a.getAttribute("href") ?? "";
    if (href.startsWith("#")) return;
    e.preventDefault();
    void openUrl(href);
  });
}
const app = createApp(App).use(createPinia()).use(i18n);
// Erreurs internes de Vue (rendu, setup) : invisibles pour window.onerror.
app.config.errorHandler = (err, _vm, info) => bootLog(`erreur Vue (${info}) : ${(err as Error)?.stack ?? String(err)}`);
app.mount("#app");
bootLog("démarrage : interface montée");
bootDone();

if (import.meta.env.DEV && import.meta.env.VITE_SELFTEST) {
  void import("./dev/selftest").then((m) =>
    import.meta.env.VITE_SELFTEST === "2" ? m.runSelftestAnnot(import.meta.env.VITE_SELFTEST_FILE) : m.runSelftest(import.meta.env.VITE_SELFTEST_FIXTURES),
  );
}
