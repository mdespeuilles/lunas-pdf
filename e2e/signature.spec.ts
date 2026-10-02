import { expect, type Page, test } from "@playwright/test";

async function open(page: Page, path: string, extra: Record<string, unknown> = {}) {
  await page.addInitScript(
    ([p, x]) => {
      (window as unknown as Record<string, unknown>).__FEUILLET_E2E__ = { pending: [p], ...x };
    },
    [path, extra] as const,
  );
  await page.goto("/");
  // Document prêt (nombre de pages affiché), sinon les raccourcis sont ignorés.
  await expect(page.getByRole("toolbar", { name: path.split("/").pop() }).getByText(/^sur \d+$/)).toBeVisible();
}

/** Pose l'image qui suit le curseur, par un clic sur la première page. */
async function placeAt(page: Page, x = 250, y = 300) {
  const box = (await page.locator(".page[data-page='0']").boundingBox())!;
  await page.mouse.move(box.x + x, box.y + y, { steps: 3 });
  await expect(page.locator(".page[data-page='0'] .ghost")).toBeVisible();
  await page.mouse.click(box.x + x, box.y + y);
}

async function annotate(page: Page) {
  await page.keyboard.press("Control+Shift+A");
  await expect(page.getByRole("toolbar", { name: "Outils d’annotation" })).toBeVisible();
}

const model = (page: Page, path: string) => page.evaluate((p) => ((window as any).__FEUILLET_E2E__.saved[p] ?? []) as any[], path);
const calls = (page: Page) => page.evaluate(() => (window as any).__FEUILLET_E2E__.calls as string[]);

test.describe("Signatures numériques", () => {
  test("valide : bandeau, panneau de détails, copie", async ({ page }) => {
    await open(page, "/docs/signe.pdf");
    const banner = page.getByRole("status").filter({ hasText: "Signé numériquement" });
    await expect(banner).toContainText("par Claire Martin · signature valide, document intact");
    await banner.getByRole("button", { name: "Détails de la signature" }).click();
    const panel = page.getByRole("complementary", { name: "Signature numérique" });
    await expect(panel).toContainText("Signature valide");
    await expect(panel).toContainText("Le document n’a pas été modifié depuis sa signature.");
    await expect(panel).toContainText("Atelier Vauban SARL");
    await expect(panel).toContainText("Bon pour accord");
    await expect(panel).toContainText("Approuvée");
    await expect(panel).toContainText("4F:2A:91:C3 … 7C:0E");
    await expect(panel).toContainText("(UTC+2)");
    await panel.getByRole("button", { name: "Afficher le certificat" }).click();
    await expect(page.getByRole("dialog", { name: "Certificat" })).toContainText("CN=Claire Martin,O=Atelier Vauban SARL,C=FR");
    await page.keyboard.press("Escape");
    await panel.getByRole("button", { name: "Fermer le panneau (Échap)" }).click();
    await expect(panel).toHaveCount(0);
    // La signature visible sur la page rouvre le panneau.
    await page.getByRole("button", { name: "Signature de Claire Martin : afficher les détails" }).click();
    await expect(panel).toBeVisible();
  });

  test("invalide et non vérifiable", async ({ page }) => {
    await open(page, "/docs/signe-altere.pdf");
    await expect(page.getByRole("status").filter({ hasText: "Signature invalide" })).toContainText("le document a été modifié après la signature de Claire Martin");
    await page.goto("about:blank");
    await open(page, "/docs/signe-inconnu.pdf");
    await expect(page.getByRole("status").filter({ hasText: "Signature non vérifiable" })).toContainText("le certificat du signataire n’est pas reconnu par ce système");
  });

  test("faire confiance à l'autorité d'une signature non vérifiable, puis la retirer", async ({ page }) => {
    await open(page, "/docs/signe-inconnu.pdf");
    await page.getByRole("button", { name: "Détails de la signature" }).click();
    const panel = page.getByRole("complementary", { name: "Signature numérique" });
    await panel.getByRole("button", { name: "Faire confiance à « Autorité de test »…" }).click();
    const dlg = page.getByRole("dialog", { name: "Faire confiance à cette autorité ?" });
    await expect(dlg).toContainText("AB:CD:EF");
    await dlg.getByRole("button", { name: "Faire confiance", exact: true }).click();
    await expect(page.getByRole("status").filter({ hasText: "Signé numériquement" })).toBeVisible();
    await expect(panel.getByRole("button", { name: /Faire confiance/ })).toHaveCount(0);

    await page.keyboard.press("Control+,");
    const prefs = page.getByRole("dialog", { name: "Préférences" });
    await prefs.getByRole("button", { name: "Ne plus faire confiance à « Autorité de test »" }).click();
    await expect(page.getByRole("status").filter({ hasText: "Signature non vérifiable" })).toBeVisible();
  });

  test("avertissement avant d'annoter : annuler, annoter quand même, annoter une copie", async ({ page }) => {
    await open(page, "/docs/signe.pdf", { savePath: "/docs/signe (copie).pdf" });
    const annotate = page.getByRole("button", { name: "Annoter (Ctrl Maj A)" });
    const dlg = page.getByRole("alertdialog", { name: "Annoter un document signé ?" });
    await page.keyboard.press("Control+Shift+A");
    await expect(dlg).toContainText("après la signature numérique de Claire Martin");
    await dlg.getByRole("button", { name: "Annuler" }).click();
    await expect(annotate).toHaveAttribute("aria-pressed", "false");

    await annotate.click();
    await dlg.getByRole("button", { name: "Annoter une copie" }).click();
    await expect(annotate).toHaveAttribute("aria-pressed", "true");
    expect(await calls(page)).toContain("save:/docs/signe (copie).pdf");
    await expect(page.getByRole("tab", { name: /signe \(copie\)\.pdf/ })).toBeVisible();

    // Une fois accepté, plus de question pour cet onglet.
    await annotate.click();
    await annotate.click();
    await expect(dlg).toHaveCount(0);
    await expect(annotate).toHaveAttribute("aria-pressed", "true");
  });
});

test.describe("Signature manuscrite", () => {
  test("dessiner, enregistrer, réutiliser, supprimer", async ({ page }) => {
    await open(page, "/docs/contrat.pdf");
    await annotate(page);
    await page.keyboard.press("s");
    const menu = page.getByRole("menu", { name: "Signatures" });
    await menu.getByRole("menuitem", { name: "Créer une signature…" }).click();
    const dlg = page.getByRole("dialog", { name: "Nouvelle signature" });
    const add = dlg.getByRole("button", { name: "Ajouter au document" });
    await expect(add).toBeDisabled();
    const pad = (await dlg.getByRole("img", { name: "Zone de dessin de la signature" }).boundingBox())!;
    await page.mouse.move(pad.x + 60, pad.y + 110);
    await page.mouse.down();
    for (let i = 1; i <= 12; i++) await page.mouse.move(pad.x + 60 + i * 25, pad.y + 110 - Math.sin(i) * 30, { steps: 2 });
    await page.mouse.up();
    await dlg.getByRole("button", { name: "Encre bleue" }).click();
    await add.click();
    await expect(dlg).toHaveCount(0);
    await placeAt(page);
    await expect.poll(async () => (await model(page, "/docs/contrat.pdf")).filter((a) => a.body.type === "image").length).toBe(1);
    expect((await calls(page)).some((c) => c.startsWith("importImageBytes:"))).toBe(true);
    expect(await calls(page)).toContain("saveSignature:true");

    // Réutilisation depuis le menu.
    await page.keyboard.press("Escape");
    await page.keyboard.press("s");
    const saved = menu.getByRole("menuitem", { name: /Signature — Ajoutée le 14 sept/ });
    await saved.click();
    await placeAt(page, 300, 450);
    await expect.poll(async () => (await model(page, "/docs/contrat.pdf")).filter((a) => a.body.type === "image").length).toBe(2);
    await page.keyboard.press("Escape");
    await page.keyboard.press("s");
    await menu.getByRole("button", { name: "Supprimer la signature" }).click();
    await expect(saved).toHaveCount(0);
  });

  test("taper son nom, importer une image", async ({ page }) => {
    await open(page, "/docs/contrat.pdf", { image: "/img/signature-scan.png" });
    await annotate(page);
    await page.getByRole("button", { name: "Signature (S)" }).click();
    await page.getByRole("menuitem", { name: "Créer une signature…" }).click();
    const dlg = page.getByRole("dialog", { name: "Nouvelle signature" });
    await dlg.getByRole("tab", { name: "Taper" }).click();
    await dlg.getByRole("textbox", { name: "Votre nom" }).fill("Camille Durand");
    await expect(dlg.getByRole("radio")).toHaveCount(3);
    await dlg.getByRole("radio").nth(1).click();
    await expect(dlg.getByRole("radio").nth(1)).toHaveAttribute("aria-checked", "true");
    await dlg.getByRole("checkbox", { name: "" }).click(); // ne pas enregistrer
    await dlg.getByRole("button", { name: "Ajouter au document" }).click();
    await placeAt(page);
    await expect.poll(async () => (await model(page, "/docs/contrat.pdf")).filter((a) => a.body.type === "image").length).toBe(1);
    expect(await calls(page)).not.toContain("saveSignature:true");

    await page.keyboard.press("Escape");
    await page.keyboard.press("s");
    await page.getByRole("menuitem", { name: "Créer une signature…" }).click();
    await dlg.getByRole("tab", { name: "Importer" }).click();
    await dlg.getByRole("button", { name: "Choisir une image…" }).click();
    await expect(dlg).toContainText("signature-scan.png · 1 × 1 px");
    await expect(dlg.getByRole("switch", { name: "Supprimer le fond blanc" })).toHaveAttribute("aria-checked", "true");
    await dlg.getByRole("button", { name: "Ajouter au document" }).click();
    await placeAt(page, 300, 450);
    await expect.poll(async () => (await model(page, "/docs/contrat.pdf")).filter((a) => a.body.type === "image").length).toBe(2);
  });

  test("champ de signature d'un formulaire : cliquer pour signer", async ({ page }) => {
    await open(page, "/docs/formulaire.pdf");
    await page.getByRole("button", { name: "signature : Cliquer pour signer" }).click();
    await page.getByRole("menuitem", { name: "Créer une signature…" }).click();
    const dlg = page.getByRole("dialog", { name: "Nouvelle signature" });
    await dlg.getByRole("tab", { name: "Taper" }).click();
    await dlg.getByRole("textbox", { name: "Votre nom" }).fill("Camille Durand");
    await dlg.getByRole("button", { name: "Ajouter au document" }).click();
    await expect.poll(async () => (await model(page, "/docs/formulaire.pdf")).find((a) => a.body.type === "image")?.rect).toBeTruthy();
    const r = (await model(page, "/docs/formulaire.pdf")).find((a) => a.body.type === "image").rect;
    // Ajustée dans le champ (180, 500, 200 × 50).
    expect(r.x).toBeGreaterThanOrEqual(180);
    expect(r.x + r.w).toBeLessThanOrEqual(380);
    expect(r.y).toBeGreaterThanOrEqual(500);
    expect(r.y + r.h).toBeLessThanOrEqual(550);
  });
});
