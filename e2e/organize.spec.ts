import { expect, type Page, test } from "@playwright/test";

async function open(page: Page, path: string, extra: Record<string, unknown> = {}) {
  await page.addInitScript(
    ([p, x]) => {
      (window as unknown as Record<string, unknown>).__FEUILLET_E2E__ = { pending: [p], ...x };
    },
    [path, extra] as const,
  );
  await page.goto("/");
  await expect(page.getByRole("toolbar", { name: path.split("/").pop() }).getByText(/^sur \d+$/)).toBeVisible();
  await page.getByRole("button", { name: "Organiser les pages (Ctrl Maj O)" }).click();
  await expect(page.getByRole("toolbar", { name: "Organiser les pages" })).toBeVisible();
}

const order = (page: Page, path = "/docs/contrat.pdf") => page.evaluate((p) => ((window as any).__FEUILLET_E2E__.pages?.[p] ?? []) as string[], path);
const calls = (page: Page) => page.evaluate(() => (window as any).__FEUILLET_E2E__.calls as string[]);
const grid = (page: Page, name = "contrat.pdf") => page.getByRole("listbox", { name: `Pages de ${name}` });
const thumb = (page: Page, n: number, name = "contrat.pdf") => grid(page, name).getByRole("option", { name: `Page ${n}`, exact: true });

/** Glisse `from` sur la moitié gauche de `to` (insertion avant). */
async function dragBefore(page: Page, from: ReturnType<typeof thumb>, to: ReturnType<typeof thumb>, check?: (page: Page) => Promise<void>) {
  const a = (await from.boundingBox())!;
  const b = (await to.boundingBox())!;
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 20, a.y + a.height / 2 + 10, { steps: 3 });
  await page.mouse.move(b.x + b.width * 0.2, b.y + b.height / 2, { steps: 6 });
  if (check) await check(page);
  await page.mouse.up();
}

test("sélection, rotation, suppression, duplication, page blanche, annuler", async ({ page }) => {
  await open(page, "/docs/contrat.pdf");
  await expect(grid(page).getByRole("option")).toHaveCount(12);
  await expect(thumb(page, 1)).toHaveAttribute("aria-selected", "true");

  await thumb(page, 3).click();
  await thumb(page, 5).click({ modifiers: ["Control"] });
  await expect(page.getByText("2 pages sélectionnées")).toBeVisible();
  await page.getByRole("button", { name: "Pivoter à droite (Ctrl R)" }).click();
  await expect.poll(() => order(page)).toEqual(["1", "2", "3@90", "4", "5@90", "6", "7", "8", "9", "10", "11", "12"]);
  await expect(page.locator(".tab.on .dirty")).toBeVisible();

  await page.keyboard.press("Delete");
  await expect(grid(page).getByRole("option")).toHaveCount(10);
  await expect.poll(async () => (await order(page)).slice(0, 4)).toEqual(["1", "2", "4", "6"]);

  // Maj : plage ; Ctrl D : chaque copie suit son original.
  await thumb(page, 1).click();
  await thumb(page, 2).click({ modifiers: ["Shift"] });
  await page.keyboard.press("Control+d");
  await expect.poll(async () => (await order(page)).slice(0, 5)).toEqual(["1", "1", "2", "2", "4"]);
  await expect(thumb(page, 1)).toHaveAttribute("aria-selected", "true");
  await expect(thumb(page, 3)).toHaveAttribute("aria-selected", "true");

  await page.getByRole("button", { name: "Insérer une page blanche (Ctrl Maj N)" }).click();
  // Après le dernier original sélectionné (page 3).
  await expect.poll(async () => (await order(page)).slice(0, 5)).toEqual(["1", "1", "2", "blanche", "2"]);
  await expect(thumb(page, 4)).toHaveAttribute("aria-selected", "true");

  // Annuler jusqu'au document d'origine.
  for (let i = 0; i < 4; i++) await page.keyboard.press("Control+z");
  await expect.poll(() => order(page)).toEqual(["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]);
  await expect(page.locator(".tab.on .dirty")).toHaveCount(0);
});

test("glisser-déposer pour réordonner, Terminé revient à la lecture", async ({ page }) => {
  await open(page, "/docs/contrat.pdf");
  await thumb(page, 4).click();
  await dragBefore(page, thumb(page, 4), thumb(page, 2), async (p) => {
    await expect(p.locator(".dpill")).toHaveText("Déplacer avant la page 2");
    await expect(p.locator(".ins")).toHaveCount(1);
  });
  await expect.poll(async () => (await order(page)).slice(0, 4)).toEqual(["1", "4", "2", "3"]);
  await expect(thumb(page, 2)).toHaveAttribute("aria-selected", "true");

  await page.getByRole("button", { name: "Terminé" }).click();
  await expect(page.getByRole("toolbar", { name: "contrat.pdf" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Aller à la page (Ctrl G)" })).toHaveValue("2");
});

test("côte à côte : copier des pages d'un document à l'autre, extraire", async ({ page }) => {
  await open(page, "/docs/contrat.pdf", { sidePath: "/docs/formulaire.pdf", extractPath: "/docs/extrait.pdf" });
  await page.getByRole("button", { name: "Côte à côte" }).click();
  await expect(grid(page, "formulaire.pdf").getByRole("option")).toHaveCount(2);
  await dragBefore(page, thumb(page, 1, "formulaire.pdf"), thumb(page, 2), async (p) => {
    await expect(p.locator(".dpill")).toHaveText("Copier avant la page 2");
  });
  await expect.poll(async () => (await order(page)).slice(0, 3)).toEqual(["1", "formulaire.pdf:1", "2"]);
  await expect(grid(page).getByRole("option")).toHaveCount(13);
  // Le document source n'est pas modifié.
  await expect.poll(() => order(page, "/docs/formulaire.pdf")).toEqual(["1", "2"]);

  await thumb(page, 3).click();
  await thumb(page, 4).click({ modifiers: ["Shift"] });
  await page.getByRole("button", { name: "Extraire en PDF" }).click();
  await expect.poll(() => calls(page)).toContain("extract:/docs/extrait.pdf:2,3");
  await expect(page.getByText("2 pages extraites dans « extrait.pdf »")).toBeVisible();

  await page.getByRole("button", { name: "Fermer", exact: true }).click();
  await expect(grid(page, "formulaire.pdf")).toHaveCount(0);
});

test("document signé : avertissement avant de réorganiser", async ({ page }) => {
  await open(page, "/docs/signe.pdf");
  await thumb(page, 2, "signe.pdf").click();
  await page.keyboard.press("Control+r");
  const dlg = page.getByRole("alertdialog", { name: "Réorganiser un document signé ?" });
  await expect(dlg).toBeVisible();
  await dlg.getByRole("button", { name: "Réorganiser quand même" }).click();
  await expect.poll(() => order(page, "/docs/signe.pdf")).toEqual(["1", "2@90"]);
});
