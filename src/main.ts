import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./styles/base.css";
import { bootDone, bootLog } from "./boot";

bootLog("démarrage : module principal chargé");
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
