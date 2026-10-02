import { defineConfig, devices } from "@playwright/test";

// Parcours UI clés sur le moteur WebKit (le plus proche de WebKitGTK), backend simulé.
export default defineConfig({
  testDir: "e2e",
  timeout: 20_000,
  fullyParallel: true,
  reporter: process.env.CI ? "github" : "list",
  use: { baseURL: "http://localhost:1421", viewport: { width: 1280, height: 820 }, locale: "fr-FR" },
  // WebKit nécessite les bibliothèques d'Ubuntu (CI) ; Chromium sert en local sur les autres distributions.
  projects: [
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport: { width: 1280, height: 820 } } },
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 820 } } },
  ],
  webServer: { command: "bunx vite --mode e2e --port 1421 --strictPort", port: 1421, reuseExistingServer: !process.env.CI },
});
