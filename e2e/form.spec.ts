import { expect, type Page, test } from "@playwright/test";

async function start(page: Page) {
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__FEUILLET_E2E__ = { pending: ["/docs/formulaire.pdf"] };
  });
  await page.goto("/");
  await expect(page.getByText("Ce document contient un formulaire.")).toBeVisible();
}

const fields = (page: Page) =>
  page.evaluate(() => Object.fromEntries(((window as any).__FEUILLET_E2E__.fields["/docs/formulaire.pdf"] ?? []).map((f: any) => [f.id, f.value])) as Record<string, string[]>);
const pill = (page: Page) => page.getByRole("navigation", { name: "Navigation entre les champs" });

test("saisie, Tab / Maj Tab, cases, radios, liste, annuler", async ({ page }) => {
  await start(page);
  await expect(pill(page)).toContainText("Champ – sur 9");
  // Champ en lecture seule : pas de contrôle de saisie.
  await expect(page.getByLabel("fixe")).toHaveCount(0);

  await page.getByRole("textbox", { name: "Nom", exact: true }).click();
  await page.keyboard.type("Durand");
  await page.keyboard.press("Tab");
  await expect.poll(async () => (await fields(page)).nom).toEqual(["Durand"]);
  await expect(page.getByRole("textbox", { name: "prenom" })).toBeFocused();
  await expect(pill(page)).toContainText("Champ 2 sur 9");
  await expect(page.locator(".tab.on .dirty")).toBeVisible();

  // Multiligne : Entrée ajoute une ligne, Tab valide.
  await page.keyboard.press("Tab");
  await page.keyboard.type("Ligne 1");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Ligne 2");
  await page.keyboard.press("Tab");
  await expect.poll(async () => (await fields(page)).commentaire).toEqual(["Ligne 1\nLigne 2"]);

  await expect(page.getByRole("checkbox", { name: "accepte" })).toBeFocused();
  await page.keyboard.press("Space");
  await expect.poll(async () => (await fields(page)).accepte).toEqual(["Yes"]);
  await expect(page.getByRole("checkbox", { name: "accepte" })).toHaveAttribute("aria-checked", "true");

  await page.getByRole("radio", { name: "formule : annuelle" }).click();
  await expect.poll(async () => (await fields(page)).formule).toEqual(["annuelle"]);

  await page.getByRole("combobox", { name: "pays" }).selectOption("Belgique");
  await expect.poll(async () => (await fields(page)).pays).toEqual(["Belgique"]);

  // Annuler depuis le document (hors champ).
  await page.locator(".scroller").focus();
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await fields(page)).pays).toEqual(["France"]);

  // Maj Tab depuis le premier champ : dernier champ, en page 2.
  await page.getByRole("textbox", { name: "Nom", exact: true }).focus();
  await page.keyboard.press("Shift+Tab");
  await expect(page.getByRole("textbox", { name: "ville" })).toBeFocused();
  await expect(pill(page)).toContainText("Champ 9 sur 9");
});

test("bandeau : surligner les champs, effacer le formulaire, masquer", async ({ page }) => {
  await start(page);
  const sw = page.getByRole("switch", { name: "Surligner les champs" });
  await expect(sw).toHaveAttribute("aria-checked", "true");
  await expect(page.locator(".flayer.hlf").first()).toBeVisible();
  await sw.click();
  await expect(sw).toHaveAttribute("aria-checked", "false");
  await expect(page.locator(".flayer.hlf")).toHaveCount(0);

  const reset = page.getByRole("button", { name: "Effacer le formulaire" });
  await expect(reset).toBeDisabled();
  await page.getByRole("textbox", { name: "Nom", exact: true }).fill("Durand");
  await page.getByRole("radio", { name: "formule : a_vie" }).click();
  await expect.poll(async () => (await fields(page)).nom).toEqual(["Durand"]);
  await reset.click();
  await expect.poll(async () => fields(page)).toMatchObject({ nom: [""], formule: ["mensuelle"] });
  // Un seul pas d'annulation pour l'effacement.
  await page.locator(".scroller").focus();
  await page.keyboard.press("Control+z");
  await expect.poll(async () => fields(page)).toMatchObject({ nom: ["Durand"], formule: ["a_vie"] });

  await page.keyboard.press("Control+s");
  await expect(page.locator(".tab.on .dirty")).toHaveCount(0);

  await page.getByRole("button", { name: "Masquer le bandeau" }).click();
  await expect(page.getByText("Ce document contient un formulaire.")).toHaveCount(0);
  // Mode annotation : les champs cèdent la place aux outils.
  await page.keyboard.press("Control+Shift+A");
  await expect(page.getByRole("textbox", { name: "Nom", exact: true })).toHaveCount(0);
  await expect(pill(page)).toHaveCount(0);
});

test("formats standard : saisie filtrée, plage, date, scripts personnalisés signalés", async ({ page }) => {
  await start(page);
  await expect(page.getByText("Scripts personnalisés non exécutés")).toBeVisible();
  const prix = page.getByRole("textbox", { name: "prix" });

  // Les lettres sont refusées à la frappe ; virgule décimale et espaces acceptés.
  await prix.click();
  await page.keyboard.type("1 2a34,5");
  await expect(prix).toHaveValue("1 234,5");
  await page.keyboard.press("Tab");
  await expect.poll(async () => (await fields(page)).prix).toEqual(["1234.5"]);
  await prix.focus();
  await expect(prix).toHaveValue("1234,5");

  // Hors plage : message, valeur précédente rétablie.
  await prix.fill("20000");
  await page.keyboard.press("Tab");
  await expect(page.getByText("« prix » : la valeur doit être comprise entre 0 et 10 000.")).toBeVisible();
  await expect.poll(async () => (await fields(page)).prix).toEqual(["1234.5"]);
  await prix.focus();
  await expect(prix).toHaveValue("1234,5");

  const date = page.getByRole("textbox", { name: "date" });
  await date.fill("2/10/26");
  await page.keyboard.press("Tab");
  await expect.poll(async () => (await fields(page)).date).toEqual(["02/10/2026"]);
  await date.fill("31/02/2026");
  await page.keyboard.press("Tab");
  await expect(page.getByText("« date » : date attendue au format dd/mm/yyyy.")).toBeVisible();
  await expect.poll(async () => (await fields(page)).date).toEqual(["02/10/2026"]);
});
