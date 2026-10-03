import { type Plugin, defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

/**
 * En développement, aucune réponse n'est mise en cache par la webview. Vite marque les
 * dépendances préparées comme « immutable » ; le cache disque de WebKitGTK les garde alors
 * d'un lancement à l'autre et réclame des fragments supprimés depuis (404, page blanche).
 */
function noStoreInDev(): Plugin {
  return {
    name: "lunas-pdf-no-store",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((_req, res, next) => {
        const setHeader = res.setHeader.bind(res);
        res.setHeader = (name: string, value: number | string | readonly string[]) =>
          name.toLowerCase() === "cache-control" ? setHeader(name, "no-store") : setHeader(name, value);
        res.setHeader("Cache-Control", "no-store");
        next();
      });
    },
  };
}

// Mode « e2e » : le backend Tauri est remplacé par une simulation (tests Playwright).
export default defineConfig(({ mode }) => ({
  plugins: [vue(), noStoreInDev()],
  clearScreen: false,
  // Cache des dépendances propre au mode e2e : partagé, il était invalidé à chaque passage
  // des tests, et le lancement suivant de `tauri dev` repartait sur une ré-optimisation
  // (anciens fragments réclamés par la webview, page blanche).
  cacheDir: mode === "e2e" ? "node_modules/.vite-e2e" : "node_modules/.vite",
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/target/**"] } },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  resolve: {
    alias:
      mode === "e2e"
        ? [{ find: /^(\.\.?\/)+bindings$/, replacement: fileURLToPath(new URL("./e2e/mock/bindings.ts", import.meta.url)) }]
        : [],
  },
  build: {
    target: "safari16",
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
}));
