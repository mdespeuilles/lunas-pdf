import { i18n } from "../i18n";
import { isPdfPath, pickPdfFiles } from "../lib/window";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";

/** Ouvre des chemins (glisser-déposer, ligne de commande, association de fichiers). */
export function openPaths(paths: string[]) {
  const pdfs = paths.filter(isPdfPath);
  if (pdfs.length < paths.length) useUi().notify(i18n.global.t("errors.notPdf"));
  if (pdfs.length) void useTabs().openPaths(pdfs);
}

export async function openFromDialog() {
  const paths = await pickPdfFiles(i18n.global.t("menu.open"));
  if (paths.length) void useTabs().openPaths(paths);
}

/** Ouvre le premier PDF à côté de `tab` (mode Organiser), sans quitter son onglet. */
export async function openBeside(tab: DocTab, paths: string[]) {
  const pdf = paths.find(isPdfPath);
  if (!pdf) {
    useUi().notify(i18n.global.t("errors.notPdf"));
    return;
  }
  const opened = await useTabs().openPaths([pdf], false);
  if (opened && opened.key !== tab.key) tab.orgSide = opened.key;
}
