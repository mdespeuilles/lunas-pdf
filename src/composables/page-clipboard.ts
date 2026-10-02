// Presse-papiers de pages : copie figée des pages (document en mémoire côté moteur), à coller
// dans n'importe quel onglet. Commun à la barre latérale et au mode « Organiser ».
import { ref } from "vue";
import { i18n } from "../i18n";
import { BackendError, commands, unwrap } from "../lib/api";
import type { DocTab } from "../stores/tabs";
import { useUi } from "../stores/ui";
import { useOrganize } from "./organize";

const clip = ref<{ doc: number; count: number } | null>(null);

function message(e: unknown) {
  return e instanceof BackendError && "message" in e.error ? e.error.message : String(e);
}

export function usePageClipboard() {
  const ui = useUi();
  const org = useOrganize();
  const t = i18n.global.t;

  async function copy(tab: DocTab, pages: number[], cut = false): Promise<boolean> {
    if (!tab.info || !pages.length) return false;
    const sel = [...new Set(pages)].sort((a, b) => a - b);
    try {
      const info = await unwrap(commands.clipPages(tab.info.id, sel));
      if (clip.value) void commands.closeDocument(clip.value.doc);
      clip.value = { doc: info.id, count: sel.length };
    } catch (e) {
      ui.notify(t("organize.failed", { msg: message(e) }));
      return false;
    }
    if (cut) {
      if (sel.length >= (tab.info.pages.length ?? 0)) {
        ui.notify(t("organize.keepOne"));
        return false;
      }
      tab.orgSel = sel;
      await org.remove(tab);
    }
    ui.notify(t(cut ? "pageClip.cut" : "pageClip.copied", { n: sel.length }, sel.length));
    return true;
  }

  /** Colle avant l'indice `at` ; les pages collées deviennent la sélection. */
  async function paste(tab: DocTab, at: number): Promise<boolean> {
    const c = clip.value;
    if (!c || !tab.info) return false;
    const pages = Array.from({ length: c.count }, (_, i) => i);
    const ok = await org.importFrom(tab, c.doc, pages, at);
    if (ok) ui.notify(t("pageClip.pasted", { n: c.count }, c.count));
    return ok;
  }

  const hasPages = () => !!clip.value;

  return { copy, paste, hasPages };
}
