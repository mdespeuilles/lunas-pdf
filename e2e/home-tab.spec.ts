import { expect, test } from "@playwright/test";

test("« + » ouvre l'onglet Accueil ; un document ouvert depuis l'accueil le remplace", async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__LUNAS_PDF_E2E__ = { pending: ["/docs/contrat.pdf"] };
  });
  await page.goto("/");
  const bar = page.locator("header.titlebar");
  const tabs = bar.getByRole("tab");
  await expect(tabs).toHaveCount(1);

  await page.getByRole("button", { name: "Nouvel onglet (Ctrl T)" }).click();
  await expect(page.getByText("Déposez un PDF ici")).toBeVisible();
  await expect(bar.getByRole("tab", { name: /Accueil/ })).toHaveAttribute("aria-selected", "true");
  await expect(tabs).toHaveCount(2);

  // Retour au document, puis à nouveau l'accueil au clavier.
  await bar.getByRole("tab", { name: /contrat\.pdf/ }).click();
  await expect(page.getByText("Déposez un PDF ici")).toHaveCount(0);
  await page.keyboard.press("Control+t");
  await expect(page.getByText("Déposez un PDF ici")).toBeVisible();
  await expect(tabs).toHaveCount(2);

  // Ctrl W ferme l'onglet Accueil.
  await page.keyboard.press("Control+w");
  await expect(tabs).toHaveCount(1);
  await expect(bar.getByRole("tab", { name: /contrat\.pdf/ })).toHaveAttribute("aria-selected", "true");

  // Document ouvert depuis l'accueil : il remplace l'onglet Accueil.
  await page.keyboard.press("Control+t");
  await page.evaluate(() => (window as any).__LUNAS_PDF_E2E__.emitOpen(["/docs/formulaire.pdf"]));
  await expect(bar.getByRole("tab", { name: /formulaire\.pdf/ })).toHaveAttribute("aria-selected", "true");
  await expect(bar.getByRole("tab", { name: /Accueil/ })).toHaveCount(0);
  await expect(tabs).toHaveCount(2);
});
