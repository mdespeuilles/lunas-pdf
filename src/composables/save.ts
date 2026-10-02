// Enregistrement et fermeture des documents modifiés.
import { i18n } from "../i18n";
import { BackendError, inTauri } from "../lib/api";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";

async function pickSavePath(tab: DocTab): Promise<string | null> {
  if (!inTauri) return (window as unknown as { __FEUILLET_E2E__?: { savePath?: string } }).__FEUILLET_E2E__?.savePath ?? null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save({ defaultPath: tab.path, filters: [{ name: "PDF", extensions: ["pdf"] }] });
}

/** Enregistre ; `saveAs` demande un nouveau chemin. Renvoie vrai si le document est enregistré. */
export async function saveTab(tab: DocTab, saveAs = false): Promise<boolean> {
  const t = i18n.global.t;
  const ui = useUi();
  if (!tab.info || tab.status !== "ready") return false;
  let path: string | null = null;
  if (saveAs) {
    path = await pickSavePath(tab);
    if (!path) return false;
  } else if (!tab.dirty) {
    return true;
  }
  if (tab.edit?.pendingRedactions && (await ui.askUser("redact", tab.key)) !== "confirm") return false;
  try {
    await useTabs().save(tab, path);
    ui.notify(t("save.saved"));
    return true;
  } catch (e) {
    const msg = e instanceof BackendError && "message" in e.error ? e.error.message : String(e);
    ui.notify(t("save.failed", { msg }));
    return false;
  }
}

/** Ferme un onglet, en proposant d'enregistrer s'il est modifié (planche 12). */
export async function requestClose(tab: DocTab): Promise<boolean> {
  const tabs = useTabs();
  if (!tab.dirty) {
    tabs.close(tab.key);
    return true;
  }
  tabs.activeKey = tab.key;
  const answer = await useUi().askUser("unsaved", tab.key);
  if (answer === "cancel") return false;
  if (answer === "save" && !(await saveTab(tab))) return false;
  tabs.close(tab.key);
  return true;
}

/** Avant de quitter : une demande par document modifié ; faux si l'utilisateur annule. */
export async function confirmQuit(): Promise<boolean> {
  for (const tab of [...useTabs().tabs].filter((t) => t.dirty)) {
    if (!(await requestClose(tab))) return false;
  }
  return true;
}
