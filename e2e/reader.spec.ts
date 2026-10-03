import { expect, type Page, test } from "@playwright/test";

/** Démarre l'app avec des fichiers « passés en ligne de commande ». */
async function start(page: Page, pending: string[] = [], settings: Record<string, unknown> = {}) {
  await page.addInitScript(([p, s]) => {
    (window as unknown as Record<string, unknown>).__LUNAS_PDF_E2E__ = { pending: p, settings: s };
  }, [pending, settings] as const);
  await page.goto("/");
}

test.describe("Accueil", () => {
  test("zone de dépôt, récents en grille puis en liste, effacer la liste", async ({ page }) => {
    await start(page);
    await expect(page.getByRole("heading", { name: "Déposez un PDF ici" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Ouvrir un fichier/ })).toBeVisible();
    await expect(page.getByText("contrat.pdf")).toBeVisible();
    await expect(page.getByText(/Hier · protégé/)).toBeVisible();
    await page.getByRole("radio", { name: "Vue en liste" }).click();
    await expect(page.locator(".list .row")).toHaveCount(2);
    await page.getByRole("button", { name: "Effacer la liste" }).click();
    await expect(page.getByText("Les documents ouverts apparaîtront ici.")).toBeVisible();
  });

  test("un clic sur un récent ouvre un onglet", async ({ page }) => {
    await start(page);
    await page.getByRole("button", { name: /contrat\.pdf/ }).first().click();
    await expect(page.getByRole("tab", { name: /contrat\.pdf/ })).toBeVisible();
    await expect(page.getByRole("toolbar").getByText("sur 12", { exact: true })).toBeVisible();
  });
});

test.describe("Lecture", () => {
  test.beforeEach(async ({ page }) => {
    await start(page, ["/docs/contrat.pdf"]);
    await expect(page.getByRole("toolbar").getByText("sur 12", { exact: true })).toBeVisible();
  });

  test("navigation au clavier et champ « Aller à la page »", async ({ page }) => {
    const input = page.getByRole("textbox", { name: "Aller à la page (Ctrl G)" });
    await expect(input).toHaveValue("1");
    await page.keyboard.press("ArrowRight");
    await expect(input).toHaveValue("2");
    await page.keyboard.press("Control+g");
    await expect(input).toBeFocused();
    await input.fill("7");
    await input.press("Enter");
    await expect(input).toHaveValue("7");
    await expect(page.getByRole("status").filter({ hasText: "Page 7" })).toBeVisible();
    await page.keyboard.press("End");
    await expect(input).toHaveValue("12");
  });

  test("zoom : menu, raccourcis, ajustements", async ({ page }) => {
    const zoom = page.getByRole("button", { name: "Niveau de zoom" });
    await zoom.click();
    await page.getByRole("menuitem", { name: /200/ }).click();
    await expect(zoom).toContainText("200");
    await expect(page.getByRole("radio", { name: "Ajuster à la largeur (W)" })).toHaveAttribute("aria-checked", "false");
    await page.keyboard.press("Control+0");
    await expect(zoom).toContainText("100");
    await page.keyboard.press("Control+=");
    await expect(zoom).toContainText("110");
    await page.keyboard.press("w");
    await expect(page.getByRole("radio", { name: "Ajuster à la largeur (W)" })).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("f");
    await expect(page.getByRole("radio", { name: "Ajuster à la page (F)" })).toHaveAttribute("aria-checked", "true");
  });

  test("modes d'affichage (Ctrl 1/2/3) et barre latérale (F9)", async ({ page }) => {
    await page.keyboard.press("Control+Digit3");
    await expect(page.getByRole("radio", { name: "Double page (Ctrl 3)" })).toHaveAttribute("aria-checked", "true");
    await expect(page.locator(".page[data-page='1']")).toBeVisible();
    await page.keyboard.press("Control+Digit1");
    await expect(page.locator(".content .page")).toHaveCount(1);
    await page.keyboard.press("F9");
    await expect(page.getByRole("complementary")).toHaveCount(0);
    await page.keyboard.press("F9");
    await expect(page.getByRole("complementary")).toBeVisible();
  });

  test("sommaire et annotations dans la barre latérale", async ({ page }) => {
    await page.getByRole("tab", { name: "Sommaire et signets" }).click();
    await page.getByRole("button", { name: /Prix et paiement/ }).click();
    await expect(page.getByRole("textbox", { name: "Aller à la page (Ctrl G)" })).toHaveValue("3");
    await page.getByRole("tab", { name: "Annotations" }).click();
    await expect(page.getByRole("button", { name: /Surlignage.*Claire/ })).toBeVisible();
    await page.getByRole("button", { name: /Note de relecture/ }).click();
    await expect(page.getByRole("textbox", { name: "Aller à la page (Ctrl G)" })).toHaveValue("6");
  });

  test("texte sélectionnable et lien interne", async ({ page }) => {
    await expect(page.locator(".page[data-page='0'] .textLayer span").first()).toHaveText("Article ");
    await page.locator(".page[data-page='0'] a.link").click();
    await expect(page.getByRole("textbox", { name: "Aller à la page (Ctrl G)" })).toHaveValue("9");
  });

  test("sélection du texte à la souris : mots et sauts de ligne", async ({ page }) => {
    const spans = page.locator(".page[data-page='0'] .textLayer span");
    await expect(spans.first()).toHaveText("Article ");
    const a = await spans.nth(0).boundingBox();
    const b = await spans.nth(5).boundingBox();
    await page.mouse.move(a!.x + 1, a!.y + a!.height / 2);
    await page.mouse.down();
    await page.mouse.move(b!.x + b!.width - 1, b!.y + b!.height / 2, { steps: 8 });
    await page.mouse.up();
    const selected = await page.evaluate(() => window.getSelection()!.toString());
    expect(selected.startsWith("Article 1\nLe prix")).toBe(true);
  });

  test("fermeture de l'onglet (Ctrl W) : retour à l'accueil", async ({ page }) => {
    await page.keyboard.press("Control+w");
    await expect(page.getByRole("heading", { name: "Déposez un PDF ici" })).toBeVisible();
  });
});

test.describe("Recherche", () => {
  test("occurrences, navigation, casse et résultats groupés", async ({ page }) => {
    await start(page, ["/docs/contrat.pdf"]);
    await expect(page.getByRole("toolbar").getByText("sur 12", { exact: true })).toBeVisible();
    await page.keyboard.press("Control+f");
    const box = page.getByRole("textbox", { name: "Rechercher dans le document (Ctrl F)" });
    await expect(box).toBeFocused();
    await box.fill("échéance");
    await expect(page.getByText("1 sur 4")).toBeVisible();
    await expect(page.getByText("4 occurrences de « échéance » dans 3 pages")).toBeVisible();
    await expect(page.locator(".rg")).toHaveCount(3);
    await box.press("Enter");
    await expect(page.getByText("2 sur 4")).toBeVisible();
    await box.press("Shift+Enter");
    await expect(page.getByText("1 sur 4")).toBeVisible();
    await page.keyboard.press("Alt+c");
    await expect(page.getByRole("button", { name: "Respecter la casse (Alt C)" })).toHaveAttribute("aria-pressed", "true");
    await expect(page.getByText("1 sur 2")).toBeVisible();
    await expect(page.locator(".page .m.cur")).toHaveCount(1);
    await box.press("Escape");
    await expect(box).toHaveValue("");
    await expect(page.getByRole("tab", { name: "Miniatures" })).toBeVisible();
  });
});

test.describe("Document protégé", () => {
  test("mot de passe incorrect puis correct", async ({ page }) => {
    await start(page, ["/docs/protege.pdf"]);
    const dialog = page.getByRole("dialog", { name: "Ce document est protégé" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByText("Saisissez le mot de passe pour ouvrir « protege.pdf ».")).toBeVisible();
    const pwd = dialog.getByLabel("Mot de passe", { exact: true });
    await pwd.fill("mauvais");
    await pwd.press("Enter");
    await expect(dialog.getByRole("alert")).toHaveText(/Mot de passe incorrect/);
    await dialog.getByRole("button", { name: "Afficher le mot de passe" }).click();
    await expect(pwd).toHaveAttribute("type", "text");
    await pwd.fill("feuillet");
    await dialog.getByRole("button", { name: "Déverrouiller" }).click();
    await expect(page.getByRole("toolbar").getByText("sur 3", { exact: true })).toBeVisible();
  });

  test("formulaire XFA : message clair", async ({ page }) => {
    await start(page, ["/docs/xfa.pdf"]);
    await expect(page.getByText(/formulaire XFA dynamique, qui n’est pas pris en charge/)).toBeVisible();
  });
});

test.describe("Préférences", () => {
  test("thème, langue et boutons de fenêtre", async ({ page }) => {
    await start(page);
    await page.getByRole("button", { name: "Menu principal" }).click();
    await page.getByRole("menuitem", { name: /Préférences/ }).click();
    const dlg = page.getByRole("dialog", { name: "Préférences" });
    await dlg.getByRole("radio", { name: "Sombre" }).click();
    await expect(page.locator("html")).toHaveClass(/dark/);
    await dlg.getByRole("checkbox").uncheck({ force: true });
    await expect(page.getByRole("button", { name: "Réduire" })).toHaveCount(0);
    await dlg.getByLabel("Langue").selectOption("en");
    await expect(page.getByRole("dialog", { name: "Preferences" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("heading", { name: "Drop a PDF here" })).toBeVisible();
  });
});

test.describe("Accessibilité", () => {
  test("tous les boutons ont un nom accessible", async ({ page }) => {
    await start(page, ["/docs/contrat.pdf"]);
    await expect(page.getByRole("toolbar").getByText("sur 12", { exact: true })).toBeVisible();
    const unnamed = await page.$$eval("button", (els) =>
      els.filter((b) => !(b.getAttribute("aria-label") || b.textContent?.trim())).map((b) => b.outerHTML.slice(0, 80)),
    );
    expect(unnamed).toEqual([]);
  });

  test("focus visible au clavier", async ({ page }) => {
    await start(page);
    await page.keyboard.press("Tab");
    const shadow = await page.evaluate(() => getComputedStyle(document.activeElement!).boxShadow);
    expect(shadow).not.toBe("none");
  });
});
