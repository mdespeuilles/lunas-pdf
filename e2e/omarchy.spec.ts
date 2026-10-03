import { expect, type Page, test } from "@playwright/test";

const tokyo = { name: "tokyo-night", dark: true, colors: { accent: "#7aa2f7", background: "#1a1b26", darker_background: "#0e0e14", lighter_background: "#24283b", foreground: "#a9b1d6", bright_foreground: "#c0caf5" } };
const latte = { name: "catppuccin-latte", dark: false, colors: { accent: "#1e66f5", background: "#eff1f5", dark_background: "#e3e4e8", darker_background: "#d7d8dc", foreground: "#4c4f69" } };

const rootVar = (page: Page, v: string) => page.evaluate((v) => document.documentElement.style.getPropertyValue(v), v);
const isDark = (page: Page) => page.evaluate(() => document.documentElement.classList.contains("dark"));

test("Omarchy : l'app suit le thème de l'OS, en direct, sauf thème choisi", async ({ page }) => {
  await page.addInitScript((o) => {
    (window as unknown as Record<string, unknown>).__LUNAS_PDF_E2E__ = { pending: [], settings: { theme: "system" }, omarchy: o };
  }, tokyo);
  await page.goto("/");
  await expect.poll(() => rootVar(page, "--bg")).toBe("#1a1b26");
  expect(await rootVar(page, "--accent")).toBe("#7aa2f7");
  expect(await isDark(page)).toBe(true);

  // Changement de thème dans Omarchy : appliqué sans relancer l'app.
  await page.evaluate((o) => (window as any).__LUNAS_PDF_E2E__.emitOmarchy(o), latte);
  await expect.poll(() => rootVar(page, "--bg")).toBe("#eff1f5");
  expect(await isDark(page)).toBe(false);

  // Préférences : le thème suivi est indiqué ; « Sombre » reprend les couleurs de l'app.
  await page.keyboard.press("Control+,");
  const dialog = page.getByRole("dialog", { name: "Préférences" });
  await expect(dialog.getByText("Suit le thème Omarchy « Catppuccin Latte ».")).toBeVisible();
  await expect(dialog.getByText("Celle du thème Omarchy.")).toBeVisible();
  await dialog.getByRole("radio", { name: "Sombre" }).click();
  await expect.poll(() => rootVar(page, "--bg")).toBe("");
  expect(await rootVar(page, "--accent")).toBe("#3466f6");
  expect(await isDark(page)).toBe(true);
  await expect(dialog.getByText("« Système » suit le thème Omarchy.")).toBeVisible();
});

test("sans Omarchy : rien ne change", async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__LUNAS_PDF_E2E__ = { pending: [], settings: { theme: "light" } };
  });
  await page.goto("/");
  await expect.poll(() => rootVar(page, "--accent")).toBe("#3466f6");
  expect(await rootVar(page, "--bg")).toBe("");
});
