import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

// Mode « e2e » : le backend Tauri est remplacé par une simulation (tests Playwright).
export default defineConfig(({ mode }) => ({
  plugins: [vue()],
  clearScreen: false,
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
