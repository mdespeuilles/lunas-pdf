import { expect, type Locator, type Page, test } from "@playwright/test";

async function open(page: Page, paths: string[], extra: Record<string, unknown> = {}) {
  await page.addInitScript(
    ([p, x]) => {
      (window as unknown as Record<string, unknown>).__FEUILLET_E2E__ = { pending: p, ...x };
    },
    [paths, extra] as const,
  );
  await page.goto("/");
  for (const p of paths) await expect(page.getByRole("tab", { name: new RegExp(p.split("/").pop()!.replace(".", "\\.")) })).toBeVisible();
}

const order = (page: Page, path = "/docs/contrat.pdf") => page.evaluate((p) => ((window as any).__FEUILLET_E2E__.pages?.[p] ?? []) as string[], path);
const thumbs = (page: Page) => page.locator("[data-thumbs]:visible");
const thumb = (page: Page, n: number) => thumbs(page).getByRole("button", { name: `Page ${n}`, exact: true });

/** Glisse jusqu'au tiers haut de `to` (insertion avant), en vérifiant l'étiquette affichée. */
async function drag(page: Page, from: Locator, to: Locator | { x: number; y: number }, pill?: string, wait = 0) {
  const a = (await from.boundingBox())!;
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 3);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 10, a.y + a.height / 3 + 12, { steps: 3 });
  const b = "x" in to ? { x: to.x, y: to.y, width: 0, height: 0 } : (await to.boundingBox())!;
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 4, { steps: 8 });
  if (wait) await page.waitForTimeout(wait);
  if (pill) await expect(page.locator(".pghost .dpill")).toHaveText(pill);
  return async () => page.mouse.up();
}

test("miniatures : réordonner par glisser, pivoter, copier-coller, supprimer", async ({ page }) => {
  await open(page, ["/docs/contrat.pdf"]);
  await expect(thumb(page, 3)).toBeVisible();

  const drop = await drag(page, thumb(page, 3), thumb(page, 1), "Déplacer avant la page 1");
  await expect(page.locator(".drop-line:visible")).toHaveCount(1);
  await drop();
  await expect.poll(async () => (await order(page)).slice(0, 4)).toEqual(["3", "1", "2", "4"]);
  await expect(page.getByRole("textbox", { name: "Aller à la page (Ctrl G)" })).toHaveValue("1");

  // Rotation par les boutons de survol.
  await thumb(page, 2).hover();
  await thumbs(page).getByRole("button", { name: "Pivoter la page 2 à droite (Ctrl R)" }).click();
  await expect.poll(async () => (await order(page))[1]).toBe("1@90");

  // Copier la page 1, coller après la page 3 ; couper la page 2.
  await thumb(page, 1).focus();
  await page.keyboard.press("Control+c");
  await expect(page.getByText("1 page copiée")).toBeVisible();
  await thumb(page, 3).focus();
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await order(page)).slice(0, 5)).toEqual(["3", "1@90", "2", "3", "4"]);
  await thumb(page, 2).focus();
  await page.keyboard.press("Control+x");
  await expect.poll(async () => (await order(page)).slice(0, 4)).toEqual(["3", "2", "3", "4"]);
  await thumb(page, 1).focus();
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await order(page)).slice(0, 3)).toEqual(["3", "1@90", "2"]);

  await thumb(page, 1).focus();
  await page.keyboard.press("Delete");
  await expect.poll(async () => (await order(page))[0]).toBe("1@90");
  await page.keyboard.press("Control+r");
  await expect.poll(async () => (await order(page))[0]).toBe("1@180");
});

test("glisser une page vers un autre onglet (survol de l'onglet), déposer un fichier", async ({ page }) => {
  await open(page, ["/docs/formulaire.pdf", "/docs/contrat.pdf"]);
  await expect(page.getByRole("tab", { name: /contrat\.pdf/ })).toHaveAttribute("aria-selected", "true");
  const target = page.getByRole("tab", { name: /formulaire\.pdf/ });
  const tb = (await target.boundingBox())!;
  const drop = await drag(page, thumb(page, 2), { x: tb.x + tb.width / 2, y: tb.y + tb.height / 2 }, undefined, 800);
  await expect(target).toHaveAttribute("aria-selected", "true");
  const first = (await thumb(page, 1).boundingBox())!;
  await page.mouse.move(first.x + first.width / 2, first.y + first.height / 4, { steps: 6 });
  await expect(page.locator(".pghost .dpill")).toHaveText("Copier avant la page 1");
  await drop();
  await expect.poll(() => order(page, "/docs/formulaire.pdf")).toEqual(["contrat.pdf:2", "1", "2"]);

  // Fichier PDF déposé sur les miniatures : ses pages sont insérées à cet endroit.
  const t2 = (await thumb(page, 2).boundingBox())!;
  const at = { x: t2.x + t2.width / 2, y: t2.y + t2.height * 0.75 };
  await page.evaluate((p) => (window as any).__FEUILLET_E2E__.emitFileDrop({ type: "over", ...p }), at);
  await expect(page.locator(".drop-line:visible")).toHaveCount(1);
  await page.evaluate((p) => (window as any).__FEUILLET_E2E__.emitFileDrop({ type: "drop", paths: ["/docs/signe.pdf"], ...p }), at);
  await expect.poll(() => order(page, "/docs/formulaire.pdf")).toEqual(["contrat.pdf:2", "1", "signe.pdf:1", "signe.pdf:2", "2"]);
});

test("mode Organiser : copier-coller des pages entre documents", async ({ page }) => {
  await open(page, ["/docs/formulaire.pdf", "/docs/contrat.pdf"]);
  await page.keyboard.press("Control+Shift+O");
  const grid = page.getByRole("listbox", { name: "Pages de contrat.pdf" });
  await grid.getByRole("option", { name: "Page 4", exact: true }).click();
  await grid.getByRole("option", { name: "Page 5", exact: true }).click({ modifiers: ["Shift"] });
  await page.keyboard.press("Control+c");
  await expect(page.getByText("2 pages copiées")).toBeVisible();
  await grid.getByRole("option", { name: "Page 1", exact: true }).click();
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await order(page)).slice(0, 4)).toEqual(["1", "4", "5", "2"]);
  await expect(grid.getByRole("option", { name: "Page 2", exact: true })).toHaveAttribute("aria-selected", "true");
});
