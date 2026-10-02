// Fichiers PDF déposés sur les miniatures : leurs pages sont insérées dans le document.
import { i18n } from "../i18n";
import { BackendError, commands, unwrap } from "../lib/api";
import { isPdfPath } from "../lib/window";
import type { DocTab } from "../stores/tabs";
import { useUi } from "../stores/ui";
import { useOrganize } from "./organize";

export async function insertFiles(tab: DocTab, paths: string[], at: number) {
  const ui = useUi();
  const org = useOrganize();
  const t = i18n.global.t;
  let pos = at;
  for (const path of paths.filter(isPdfPath)) {
    let id: number | null = null;
    try {
      const info = await unwrap(commands.openPageSource(path));
      id = info.id;
      const pages = info.pages.map((_, i) => i);
      if (await org.importFrom(tab, info.id, pages, pos)) pos += pages.length;
    } catch (e) {
      const err = e instanceof BackendError ? e.error : null;
      const msg = err?.kind === "passwordRequired" ? t("pageDrag.locked") : err && "message" in err ? err.message : String(e);
      ui.notify(t("pageDrag.insertFailed", { name: path.split(/[\\/]/).pop() ?? path, msg }));
    } finally {
      if (id !== null) void commands.closeDocument(id);
    }
  }
  if (paths.some((p) => !isPdfPath(p))) ui.notify(t("errors.notPdf"));
}
