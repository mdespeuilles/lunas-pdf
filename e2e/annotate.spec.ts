import { expect, type Page, test } from "@playwright/test";

async function start(page: Page) {
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__FEUILLET_E2E__ = { pending: ["/docs/contrat.pdf"], image: "/img/tampon.png", savePath: "/docs/copie.pdf" };
  });
  await page.goto("/");
  await expect(page.getByRole("toolbar", { name: "contrat.pdf" }).getByText("sur 12", { exact: true })).toBeVisible();
  await page.keyboard.press("Control+Shift+A");
  await expect(page.getByRole("toolbar", { name: "Outils d’annotation" })).toBeVisible();
  await page.getByRole("tab", { name: "Annotations" }).click();
}

const model = (page: Page) => page.evaluate(() => ((window as any).__FEUILLET_E2E__.saved["/docs/contrat.pdf"] ?? []) as any[]);
const calls = (page: Page) => page.evaluate(() => (window as any).__FEUILLET_E2E__.calls as string[]);

async function drag(page: Page, from: [number, number], to: [number, number]) {
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  await page.mouse.move(box.x + from[0], box.y + from[1]);
  await page.mouse.down();
  await page.mouse.move(box.x + (from[0] + to[0]) / 2, box.y + (from[1] + to[1]) / 2, { steps: 4 });
  await page.mouse.move(box.x + to[0], box.y + to[1], { steps: 4 });
  await page.mouse.up();
}

test("barre d'annotation : bouton Annoter, outils au clavier, infobulle", async ({ page }) => {
  await start(page);
  await expect(page.getByRole("button", { name: "Annoter (Ctrl Maj A)" })).toHaveAttribute("aria-pressed", "true");
  const bar = page.getByRole("toolbar", { name: "Outils d’annotation" });
  await bar.getByRole("button", { name: "Surligner (H)" }).hover();
  await expect(page.getByRole("tooltip")).toHaveText(/Surligner\s*H/);
  await page.keyboard.press("r");
  await expect(bar.getByRole("button", { name: "Rectangle (R)" })).toHaveAttribute("aria-pressed", "true");
  await expect(bar.getByText("Trait")).toBeVisible();
  await page.keyboard.press("t");
  await expect(bar.getByRole("combobox", { name: "Police" })).toBeVisible();
  await bar.getByRole("button", { name: "Terminé" }).click();
  await expect(bar).toHaveCount(0);
});

test("rectangle : création, poignées, mini-barre, suppression, annuler / rétablir", async ({ page }) => {
  await start(page);
  await page.keyboard.press("r");
  await drag(page, [300, 300], [450, 380]);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "square").length).toBe(1);
  await expect(page.locator(".ai", { hasText: "Rectangle" })).toBeVisible();
  await expect(page.locator(".page[data-page='0'] .hd")).toHaveCount(8);
  const mini = page.getByRole("toolbar", { name: "Rectangle" });
  await expect(mini.getByRole("button", { name: "Supprimer (Suppr)" })).toBeVisible();
  // Couleur depuis la mini-barre.
  await mini.getByRole("button", { name: "Bleu" }).click();
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "square")?.color).toBe("#1f6fe0");
  await expect(page.locator(".tab.on .dirty")).toBeVisible();
  await page.keyboard.press("Delete");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "square").length).toBe(0);
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "square").length).toBe(1);
  await page.keyboard.press("Control+Shift+z");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "square").length).toBe(0);
});

test("surlignage par glisser sur le texte", async ({ page }) => {
  await start(page);
  await page.keyboard.press("h");
  const k = (await page.locator(".page[data-page='0']").boundingBox())!.width / 595.28;
  // Ligne 2 du texte simulé (y = 98 pt).
  await drag(page, [80 * k, 102 * k], [300 * k, 102 * k]);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "highlight" && a.page === 0).length).toBe(1);
  const hl = (await model(page)).find((a) => a.body.type === "highlight" && a.page === 0);
  expect(hl.color).toBe("#ffd43b");
  expect(hl.body.quads[0].y).toBe(98);
});

test("zone de texte : saisie en place puis modification", async ({ page }) => {
  await start(page);
  await page.keyboard.press("t");
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  await page.mouse.click(box.x + 300, box.y + 500);
  const editor = page.getByRole("textbox", { name: "Modifier le texte" });
  await expect(editor).toBeFocused();
  await editor.fill("Montant à confirmer");
  await editor.press("Control+Enter");
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "freeText")?.body.text).toBe("Montant à confirmer");
  // Le clic marque le bord gauche de la zone, centré verticalement ; une ligne de 12 pt garde sa hauteur.
  const created = (await model(page)).find((a) => a.body.type === "freeText");
  const kk = box.width / 595.28;
  expect(created.rect.x + 2).toBeCloseTo(300 / kk, 0);
  expect(created.rect.h).toBeCloseTo(12 * 1.2 + 4, 0);
  expect(created.rect.y + created.rect.h / 2).toBeCloseTo(500 / kk, 0);
  await expect(page.locator(".ai", { hasText: "Montant à confirmer" })).toBeVisible();
  // Près du bord droit : la zone commence quand même au clic (rétrécie, pas décalée).
  await page.keyboard.press("t");
  const nearRight = 595.28 - 80;
  await page.mouse.click(box.x + nearRight * kk, box.y + 300 * kk);
  await editor.fill("km");
  await editor.press("Control+Enter");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "freeText").length).toBe(2);
  const right = (await model(page)).find((a) => a.body.type === "freeText" && a.body.text === "km");
  expect(right.rect.x + 2).toBeCloseTo(nearRight, 0);
  expect(right.rect.x + right.rect.w).toBeLessThanOrEqual(595.28 + 0.01);
  await page.keyboard.press("v");
  // Double-clic : édition de l'annotation existante.
  const ft = (await model(page)).find((a) => a.body.type === "freeText");
  const k = box.width / 595.28;
  await page.mouse.dblclick(box.x + (ft.rect.x + 5) * k, box.y + (ft.rect.y + 5) * k);
  await expect(editor).toBeVisible();
  await editor.fill("Montant validé");
  await editor.press("Control+Enter");
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "freeText" && a.id === ft.id)?.body.text).toBe("Montant validé");
});

test("note, case à cocher, duplication et déplacement au clavier", async ({ page }) => {
  await start(page);
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  await page.keyboard.press("n");
  await page.mouse.click(box.x + 500, box.y + 200);
  const note = page.getByRole("textbox", { name: "Note" });
  await note.fill("Proposer quinze jours ?");
  await note.press("Control+Enter");
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "note" && a.page === 0)?.contents).toBe("Proposer quinze jours ?");

  await page.keyboard.press("c");
  await page.getByRole("radio", { name: "Croix" }).click();
  await page.mouse.click(box.x + 200, box.y + 600);
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "check")?.body.style).toBe("cross");
  expect((await model(page)).find((a) => a.body.type === "check")?.color).toBe("#1b1b20");

  await page.keyboard.press("Control+d");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "check").length).toBe(2);
  const before = (await model(page)).filter((a) => a.body.type === "check").at(-1).rect.x;
  await page.keyboard.press("Shift+ArrowRight");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "check").at(-1).rect.x).toBe(before + 10);
});

test("fermeture d'un document modifié : planche 12", async ({ page }) => {
  await start(page);
  await page.keyboard.press("o");
  await drag(page, [300, 300], [400, 380]);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "circle").length).toBe(1);
  await page.keyboard.press("Control+w");
  const dlg = page.getByRole("alertdialog", { name: "Enregistrer les modifications ?" });
  await expect(dlg).toContainText("« contrat.pdf » contient 1 modification non enregistrée");
  await dlg.getByRole("button", { name: "Annuler" }).click();
  await expect(dlg).toHaveCount(0);
  await expect(page.getByRole("tab", { name: /contrat\.pdf/ })).toBeVisible();
  await page.keyboard.press("Control+w");
  await page.getByRole("button", { name: /^Enregistrer/ }).click();
  await expect(page.getByRole("heading", { name: "Déposez un PDF ici" })).toBeVisible();
  expect(await calls(page)).toContain("save:/docs/contrat.pdf");
});

test("caviardage : confirmation avant l'enregistrement", async ({ page }) => {
  await start(page);
  await page.keyboard.press("x");
  await drag(page, [100, 120], [300, 140]);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "redact").length).toBe(1);
  await page.keyboard.press("Control+s");
  const dlg = page.getByRole("alertdialog", { name: "Appliquer le caviardage ?" });
  await expect(dlg).toBeVisible();
  await dlg.getByRole("button", { name: "Caviarder et enregistrer" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Enregistré" })).toBeVisible();
  expect(await calls(page)).toContain("save:/docs/contrat.pdf");
  await expect(page.locator(".tab.on .dirty")).toHaveCount(0);
});

test("tampon image (I) et Enregistrer sous", async ({ page }) => {
  await start(page);
  await page.keyboard.press("i");
  // L'image suit le curseur jusqu'au clic ; Échap annule.
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  await page.mouse.move(box.x + 200, box.y + 200);
  await expect(page.locator(".page[data-page='0'] .ghost")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".ghost")).toHaveCount(0);
  await page.mouse.click(box.x + 200, box.y + 200);
  expect((await model(page)).filter((a) => a.body.type === "image")).toHaveLength(0);
  await page.keyboard.press("i");
  await page.mouse.move(box.x + 300, box.y + 400, { steps: 3 });
  await page.mouse.click(box.x + 300, box.y + 400);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "image").length).toBe(1);
  // Centrée sous le curseur (coordonnées en points : px / échelle de la page).
  const k = box.width / 595.28;
  const r = (await model(page)).find((a) => a.body.type === "image").rect;
  expect(Math.abs(r.x + r.w / 2 - 300 / k)).toBeLessThan(1.5);
  expect(Math.abs(r.y + r.h / 2 - 400 / k)).toBeLessThan(1.5);
  await page.keyboard.press("Control+Shift+s");
  await expect(page.getByRole("tab", { name: /copie\.pdf/ })).toBeVisible();
  expect(await calls(page)).toContain("save:/docs/copie.pdf");
});

test("zone de texte : noir par défaut, largeur ajustée au texte, une ligne = une ligne", async ({ page }) => {
  await start(page);
  await page.keyboard.press("t");
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  const k = box.width / 595.28;
  await page.mouse.click(box.x + 100 * k, box.y + 300 * k);
  const editor = page.getByRole("textbox", { name: "Modifier le texte" });
  await editor.fill("km");
  const edit = page.locator(".ft-edit");
  const w1 = (await edit.boundingBox())!.width;
  await editor.fill("kilomètres parcourus depuis la souscription");
  const w2 = (await edit.boundingBox())!.width;
  expect(w2).toBeGreaterThan(w1 * 4);
  await editor.fill("km");
  expect((await edit.boundingBox())!.width).toBeCloseTo(w1, 0);
  // Validation par clic ailleurs : la hauteur reste celle d'une ligne.
  await page.mouse.click(box.x + 400 * k, box.y + 600 * k);
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "freeText")?.body.text).toBe("km");
  const ft = (await model(page)).find((a) => a.body.type === "freeText");
  expect(ft.color).toBe("#1b1b20");
  expect(ft.rect.h).toBeCloseTo(12 * 1.2 + 4, 1);
  expect(ft.rect.w).toBeLessThan(30);
});

test("déplacement : le contenu suit la souris, puis réapparaît à sa place", async ({ page }) => {
  await start(page);
  await page.keyboard.press("r");
  await drag(page, [300, 300], [400, 360]);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "square").length).toBe(1);
  await page.keyboard.press("v");
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  const sq = (await model(page)).find((a) => a.body.type === "square");
  const k = box.width / 595.28;
  // Saisie sur le bord du rectangle, puis glisser de 100 px.
  await page.mouse.move(box.x + (sq.rect.x + 1) * k, box.y + (sq.rect.y + 10) * k);
  await expect(page.locator(".page[data-page='0'] .hit.movable").first()).toHaveCSS("cursor", "grab");
  await page.mouse.down();
  await page.mouse.move(box.x + (sq.rect.x + 1) * k + 50, box.y + (sq.rect.y + 10) * k, { steps: 3 });
  await page.mouse.move(box.x + (sq.rect.x + 1) * k + 100, box.y + (sq.rect.y + 10) * k, { steps: 3 });
  // Pendant le glisser : main fermée, original masqué, aperçu présent.
  await expect(page.locator(".page[data-page='0'] .alayer")).toHaveCSS("cursor", "grabbing");
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "square").hidden).toBe(true);
  await expect(page.locator(".page[data-page='0'] .alayer svg.box rect")).toHaveCount(1);
  await page.mouse.up();
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "square").rect.x).toBeCloseTo(sq.rect.x + 100 / k, 0);
  expect((await model(page)).find((a) => a.body.type === "square").hidden).toBe(false);
  // Annuler ne fait pas réapparaître l'état masqué.
  await page.keyboard.press("Control+z");
  await expect.poll(async () => (await model(page)).find((a) => a.body.type === "square").rect.x).toBeCloseTo(sq.rect.x, 0);
  expect((await model(page)).find((a) => a.body.type === "square").hidden).toBe(false);
});

test("mini-barre : reste dans la page près du bord droit", async ({ page }) => {
  await start(page);
  const pg = page.locator(".page[data-page='0']");
  const box = (await pg.boundingBox())!;
  await page.keyboard.press("c");
  await page.mouse.click(box.x + box.width - 12, box.y + 300);
  const mini = page.getByRole("toolbar", { name: "Case à cocher" });
  await expect(mini).toBeVisible();
  const m = (await mini.boundingBox())!;
  expect(m.x + m.width).toBeLessThanOrEqual(box.x + box.width);
  expect(m.x).toBeGreaterThanOrEqual(box.x);
  await expect(mini.getByRole("button", { name: "Supprimer (Suppr)" })).toBeInViewport();
});

test("outils de pose à usage unique, marquage enchaînable", async ({ page }) => {
  await start(page);
  const bar = page.getByRole("toolbar", { name: "Outils d’annotation" });
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  const k = box.width / 595.28;
  await page.keyboard.press("c");
  await page.mouse.click(box.x + 200, box.y + 400);
  await expect(bar.getByRole("button", { name: "Sélection de texte (V)" })).toHaveAttribute("aria-pressed", "true");
  // Clic ailleurs : valide (désélection) sans poser de nouvelle case.
  await page.mouse.click(box.x + 300, box.y + 500);
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "check").length).toBe(1);
  await expect(page.locator(".page[data-page='0'] .hd")).toHaveCount(0);

  await page.keyboard.press("r");
  await drag(page, [300, 300], [400, 360]);
  await expect(bar.getByRole("button", { name: "Sélection de texte (V)" })).toHaveAttribute("aria-pressed", "true");

  // Surligner reste actif ; un simple clic ne crée rien.
  await page.keyboard.press("h");
  await page.mouse.click(box.x + 90 * k, box.y + 102 * k);
  await drag(page, [80 * k, 102 * k], [200 * k, 102 * k]);
  await expect(bar.getByRole("button", { name: "Surligner (H)" })).toHaveAttribute("aria-pressed", "true");
  await expect.poll(async () => (await model(page)).filter((a) => a.body.type === "highlight" && a.page === 0).length).toBe(1);
});

test("copier, couper, coller (Ctrl C / X / V), aussi sur une autre page", async ({ page }) => {
  await start(page);
  const squares = async () => (await model(page)).filter((a) => a.body.type === "square");
  await page.keyboard.press("r");
  await drag(page, [300, 300], [450, 380]);
  await expect.poll(async () => (await squares()).length).toBe(1);
  const [orig] = await squares();

  await page.keyboard.press("Control+c");
  await page.keyboard.press("Control+v");
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await squares()).length).toBe(3);
  const xs = (await squares()).map((a) => Math.round(a.rect.x - orig.rect.x));
  expect(xs).toEqual([0, 12, 24]);

  // Couper puis coller : même place.
  await page.keyboard.press("Control+x");
  await expect.poll(async () => (await squares()).length).toBe(2);
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await squares()).length).toBe(3);
  expect(Math.round((await squares())[2].rect.x - orig.rect.x)).toBe(24);

  // Sur la dernière page, à la même position.
  await page.keyboard.press("Escape");
  await page.keyboard.press("End");
  await expect(page.getByRole("toolbar", { name: "contrat.pdf" }).getByRole("textbox").first()).toHaveValue("12");
  await page.keyboard.press("Control+v");
  await expect.poll(async () => (await squares()).find((a) => a.page === 11)?.rect.x).toBeCloseTo(orig.rect.x + 24);
});
