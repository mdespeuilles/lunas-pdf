import { expect, type Page, test } from "@playwright/test";

async function start(page: Page, agent: "claude" | "none" = "claude") {
  await page.addInitScript((a) => {
    (window as unknown as Record<string, unknown>).__LUNAS_PDF_E2E__ = { pending: ["/docs/formulaire.pdf"], settings: { aiAgent: a } };
  }, agent);
  await page.goto("/");
  await expect(page.getByText("Ce document contient un formulaire.")).toBeVisible();
}

const fields = (page: Page) =>
  page.evaluate(() => Object.fromEntries(((window as any).__LUNAS_PDF_E2E__.fields["/docs/formulaire.pdf"] ?? []).map((f: any) => [f.id, f.value])) as Record<string, string[]>);

test("panneau IA : suggestion, réponse, champ rempli, annulation, lien de page", async ({ page }) => {
  await start(page);
  const button = page.getByRole("button", { name: "Assistant IA (Ctrl J)" });
  await button.click();
  const panel = page.getByRole("complementary", { name: "Assistant IA" });
  await expect(panel).toBeVisible();
  await expect(panel.getByPlaceholder("Demander à l’IA…")).toBeFocused();

  await panel.getByRole("button", { name: "Remplis ce document" }).click();
  await expect(panel.getByText("Remplissage d’un champ")).toBeVisible();
  await expect(panel.locator(".answer")).toContainText("J’ai rempli le champ Nom");
  await expect.poll(async () => (await fields(page)).nom).toEqual(["Dupont"]);
  await expect(page.getByRole("textbox", { name: "Nom", exact: true })).toHaveValue("Dupont");
  await expect(page.locator(".tab.on .dirty")).toBeVisible();

  // Proposition de mémorisation : décocher une information, mémoriser le reste.
  const card = panel.getByRole("group", { name: "Mémoriser pour les prochains documents ?" });
  await card.getByLabel("E-mail : dupont@example.com").uncheck();
  await card.getByRole("button", { name: "Mémoriser" }).click();
  await expect(card).toContainText("Mémorisé pour les prochains documents.");
  await expect.poll(() => page.evaluate(() => (window as any).__LUNAS_PDF_E2E__.memory)).toEqual(["Nom : Dupont"]);

  // L'ajout de l'IA s'annule comme une saisie.
  await page.locator(".answer").click();
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await fields(page)).nom).toEqual([""]);

  // Lien vers une page dans la réponse.
  await panel.getByText("page 2").click();
  await expect(page.getByLabel("Aller à la page")).toHaveValue("2");

  // Ctrl J ferme le panneau ; la conversation reste pour l'onglet.
  await page.keyboard.press("Control+j");
  await expect(panel).toHaveCount(0);
  await page.keyboard.press("Control+j");
  await expect(page.getByRole("complementary", { name: "Assistant IA" }).locator(".answer")).toContainText("Nom");
});

test("sans agent choisi, pas de panneau IA", async ({ page }) => {
  await start(page, "none");
  await expect(page.getByRole("button", { name: "Assistant IA (Ctrl J)" })).toHaveCount(0);
  await page.keyboard.press("Control+j");
  await expect(page.getByRole("complementary", { name: "Assistant IA" })).toHaveCount(0);
});

test("informations mémorisées : listées et oubliées dans les préférences", async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__LUNAS_PDF_E2E__ = { pending: [], settings: { aiAgent: "claude" }, memory: ["Nom : Dupont", "Ville : Paris"] };
  });
  await page.goto("/");
  await page.keyboard.press("Control+,");
  const dialog = page.getByRole("dialog", { name: "Préférences" });
  await expect(dialog.getByText("Informations mémorisées")).toBeVisible();
  await dialog.getByRole("button", { name: "Oublier « Ville : Paris »" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__LUNAS_PDF_E2E__.memory)).toEqual(["Nom : Dupont"]);
  await dialog.getByRole("button", { name: "Tout oublier" }).click();
  await expect.poll(() => page.evaluate(() => (window as any).__LUNAS_PDF_E2E__.memory)).toEqual([]);
  await expect(dialog.getByText("Après un remplissage, l’IA propose de retenir")).toBeVisible();
});
