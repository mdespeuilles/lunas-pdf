import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./styles/base.css";

createApp(App).use(createPinia()).use(i18n).mount("#app");

if (import.meta.env.DEV && import.meta.env.VITE_SELFTEST) {
  void import("./dev/selftest").then((m) =>
    import.meta.env.VITE_SELFTEST === "2" ? m.runSelftestAnnot(import.meta.env.VITE_SELFTEST_FILE) : m.runSelftest(import.meta.env.VITE_SELFTEST_FIXTURES),
  );
}
