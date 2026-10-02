import { i18n } from "../i18n";
import { isPdfPath, pickPdfFiles } from "../lib/window";
import { useTabs } from "../stores/tabs";
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
