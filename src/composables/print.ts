// Impression native du document (dialogue d'impression du système).
import { i18n } from "../i18n";
import { BackendError, commands, unwrap } from "../lib/api";
import type { DocTab } from "../stores/tabs";
import { useUi } from "../stores/ui";

export async function printTab(tab: DocTab) {
  if (!tab.info || tab.status !== "ready") return;
  const ui = useUi();
  const t = i18n.global.t;
  try {
    const pages = tab.info.pages.map((g) => [g.width, g.height] as [number, number]);
    const started = await unwrap(commands.printDocument(tab.info.id, tab.name, pages));
    if (started) ui.notify(t("export.printed", { name: tab.name }));
  } catch (e) {
    const msg = e instanceof BackendError && "message" in e.error ? e.error.message : String(e);
    ui.notify(t("export.printFailed", { msg }));
  }
}
