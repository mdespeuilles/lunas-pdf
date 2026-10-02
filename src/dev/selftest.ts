// Scénario d'autotest visuel (développement uniquement : VITE_SELFTEST=1 pnpm tauri dev).
// Pilote les stores à intervalles fixes pour des captures d'écran externes (grim),
// sans simuler de touches au niveau du système.
import { useSettings } from "../stores/settings";
import { useTabs } from "../stores/tabs";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Phase 2 : annotations sur une copie temporaire (VITE_SELFTEST=2). */
export async function runSelftestAnnot(file: string) {
  const tabs = useTabs();
  const tools = (await import("../stores/annotTools")).useAnnotTools();
  const step = async (name: string, f: () => unknown) => {
    await f();
    console.info(`[selftest] ${name}`);
    await sleep(4000);
  };
  await sleep(4000);
  await step("1-ouverture", () => tabs.openPaths([file]));
  const tab = () => tabs.active!;
  const base = { opacity: 1, width: 2, contents: null, author: null, modified: null, excerpt: null, hidden: false };
  await step("2-annotations", async () => {
    tabs.toggleAnnotating(tab(), true);
    tab().sidebarTab = "annots";
    await sleep(500);
    const { commands } = await import("../lib/api");
    const doc = tab().info!.id;
    const l1 = await commands.textRange(doc, 0, { x: 73, y: 125 }, { x: 300, y: 125 });
    const l2 = await commands.textRange(doc, 0, { x: 73, y: 143 }, { x: 260, y: 143 });
    const l3 = await commands.textRange(doc, 0, { x: 73, y: 161 }, { x: 220, y: 161 });
    const q = (r: typeof l1) => (r.status === "ok" ? r.data.rects : []);
    const add = (a: Record<string, unknown>) => tabs.addAnnot(tab(), { id: crypto.randomUUID(), page: 0, ...base, ...a } as never);
    await add({ rect: q(l1)[0], color: "#ffd43b", body: { type: "highlight", quads: q(l1) } });
    await add({ rect: q(l2)[0], color: "#1f6fe0", width: 1, body: { type: "underline", quads: q(l2) } });
    await add({ rect: q(l3)[0], color: "#e03131", width: 1, body: { type: "strikeOut", quads: q(l3) } });
    await add({ rect: { x: 330, y: 190, w: 220, h: 40 }, color: "#e03131", body: { type: "freeText", text: "Montant à confirmer avec la direction financière.", font: "sans", size: 12 } });
    await add({ rect: { x: 340, y: 300, w: 160, h: 80 }, color: "#e03131", body: { type: "square" } });
    await add({ rect: { x: 340, y: 400, w: 160, h: 80 }, color: "#2f9e44", body: { type: "circle" } });
    await add({ rect: { x: 0, y: 0, w: 0, h: 0 }, color: "#1b1b20", body: { type: "line", from: { x: 350, y: 520 }, to: { x: 500, y: 560 }, arrow: true } });
    for (const [i, style] of (["check", "cross", "dot"] as const).entries())
      await add({ rect: { x: 350 + i * 30, y: 600, w: 16, h: 16 }, color: "#1d4ed8", width: 2.6, body: { type: "check", style } });
    await add({ rect: { x: 520, y: 60, w: 22, h: 22 }, color: "#ffd43b", contents: "Dix jours ouvrés : proposer quinze ?", body: { type: "note" } });
    await add({ rect: { x: 72, y: 230, w: 200, h: 14 }, color: "#e03131", body: { type: "redact", quads: [{ x: 72, y: 230, w: 200, h: 14 }] } });
    tools.tool = "select";
    tab().selected = tab().edit!.annots.find((a) => a.body.type === "freeText")!.id;
  });
  await step("3-annuler", () => tabs.undo(tab()));
  await step("4-retablir-sombre", async () => {
    await tabs.redo(tab());
    const { useSettings } = await import("../stores/settings");
    void useSettings().update({ theme: "dark" });
    tab().selected = tab().edit!.annots.find((a) => a.body.type === "square")!.id;
  });
  await step("5-enregistrer", async () => {
    const { useSettings } = await import("../stores/settings");
    void useSettings().update({ theme: "light" });
    await tabs.save(tab());
  });
  await step("6-fin", () => undefined);
}

export async function runSelftest(fixtures: string) {
  const tabs = useTabs();
  const settings = useSettings();
  const step = async (name: string, f: () => unknown) => {
    await f();
    console.info(`[selftest] ${name}`);
    await sleep(4000);
  };
  await sleep(4000);
  await step("1-ouverture", () => tabs.openPaths([`${fixtures}/long-320-pages.pdf`]));
  const tab = () => tabs.active!;
  await step("2-double-page", () => tabs.setMode(tab(), "double"));
  await step("3-zoom-400", () => {
    tabs.setMode(tab(), "continuous");
    tabs.setZoom(tab(), 4);
  });
  await step("4-sombre-sommaire", () => {
    tabs.setFit(tab(), "page");
    void settings.update({ theme: "dark" });
    tab().sidebarTab = "outline";
    void tabs.loadOutline(tab());
  });
  await step("5-scan", () => tabs.openPaths([`${fixtures}/scan-lourd.pdf`]));
  await step("6-chiffre", () => tabs.openPaths([`${fixtures}/chiffre-aes256.pdf`]));
  await step("7-mauvais-mdp", () => tabs.unlock(tab(), "mauvais", false));
  await step("8-accueil-clair", () => {
    void settings.update({ theme: "light", language: "en" });
    for (const t of [...tabs.tabs]) tabs.close(t.key);
  });
  await step("9-fin", () => settings.update({ language: "system" }));
}
