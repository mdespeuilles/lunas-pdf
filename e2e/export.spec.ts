import { expect, type Page, test } from "@playwright/test";

async function open(page: Page, extra: Record<string, unknown> = {}) {
  await page.addInitScript((x) => {
    (window as unknown as Record<string, unknown>).__FEUILLET_E2E__ = { pending: ["/docs/contrat.pdf"], exportPath: "/docs/sortie.pdf", ...x };
  }, extra);
  await page.goto("/");
  await expect(page.getByRole("toolbar", { name: "contrat.pdf" }).getByText(/^sur \d+$/)).toBeVisible();
}

const calls = (page: Page) => page.evaluate(() => (window as any).__FEUILLET_E2E__.calls as string[]);
const dialog = (page: Page) => page.getByRole("dialog", { name: "Exporter « contrat »" });

test("PDF aplati, plage de pages, estimation de taille", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Partager et exporter (Ctrl Maj E)" }).click();
  const d = dialog(page);
  await expect(d).toBeVisible();
  // 12 pages × 350 Ko × 0,7 (équilibrée) ≈ 2,8 Mo ; actuel 4 Mo.
  await expect(d.getByText("≈ 2,8 Mo · actuel 4 Mo")).toBeVisible();
  await expect(d.getByText("contrat.pdf", { exact: true })).toBeVisible();

  await d.getByRole("radio", { name: /PDF aplati/ }).click();
  await expect(d.getByText("contrat (aplati).pdf")).toBeVisible();
  await d.getByRole("slider", { name: "Compression" }).fill("0");
  await d.getByRole("radio", { name: "Plage…" }).click();
  const range = d.getByRole("textbox", { name: "Pages à exporter" });
  await range.fill("0");
  await expect(d.getByRole("button", { name: "Exporter…" })).toBeDisabled();
  await range.fill("2-4");
  await expect(d.getByText("≈ 410 Ko · actuel 4 Mo")).toBeVisible();
  await d.getByRole("button", { name: "Exporter…" }).click();
  await expect.poll(() => calls(page)).toContain("export:/docs/sortie.pdf:flattened:light:1,2,3:150");
  await expect(page.getByText("Exporté : « sortie.pdf »")).toBeVisible();
  await expect(d).toHaveCount(0);
});

test("mot de passe : confirmation, autorisations", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+Shift+E");
  const d = dialog(page);
  await d.getByRole("switch", { name: "Protéger par mot de passe" }).click();
  const exp = d.getByRole("button", { name: "Exporter…" });
  await expect(exp).toBeDisabled();
  await d.getByLabel("Mot de passe", { exact: true }).fill("Feuillet-2026!");
  await d.getByLabel("Confirmation").fill("Feuillet");
  await expect(d.getByText("Les mots de passe ne correspondent pas")).toBeVisible();
  await expect(exp).toBeDisabled();
  await d.getByLabel("Confirmation").fill("Feuillet-2026!");
  await expect(d.getByText("Mot de passe très solide")).toBeVisible();
  await d.getByRole("checkbox", { name: "Autoriser la copie du texte" }).click();
  await d.getByRole("radio", { name: "Page actuelle" }).click();
  await exp.click();
  await expect.poll(() => calls(page)).toContain("export:/docs/sortie.pdf:pdf:balanced:0:150:pw=Feuillet-2026!:print=true:copy=true");
});

test("images JPG, impression", async ({ page }) => {
  await open(page, { exportPath: "/docs/pages.jpg" });
  await page.keyboard.press("Control+Shift+E");
  const d = dialog(page);
  await d.getByRole("radio", { name: /Images/ }).click();
  await d.getByRole("button", { name: "JPG" }).click();
  await d.getByRole("combobox", { name: "Résolution des images" }).selectOption("300");
  await expect(d.getByText("contrat.jpg")).toBeVisible();
  await expect(d.getByRole("switch", { name: "Protéger par mot de passe" })).toHaveCount(0);
  await d.getByRole("button", { name: "Exporter…" }).click();
  await expect.poll(() => calls(page)).toContain("export:/docs/pages.jpg:jpg:balanced:all:300");
  await expect(page.getByText("12 images exportées")).toBeVisible();

  await page.keyboard.press("Control+p");
  await expect.poll(() => calls(page)).toContain("print:contrat.pdf:12");
  await expect(page.getByText("« contrat.pdf » envoyé à l’impression")).toBeVisible();
});
