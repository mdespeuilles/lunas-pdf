// Organisation des pages (planche 08) : actions de la barre, déplacement, copie entre
// documents, extraction. Chaque action est un pas d'annulation.
import type { PageOp } from "../bindings";
import { i18n } from "../i18n";
import { BackendError, commands, inTauri, unwrap } from "../lib/api";
import { movedIndices } from "../lib/page-order";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";
import { guardSigned } from "./signed";

const sorted = (v: number[]) => [...new Set(v)].sort((a, b) => a - b);

function message(e: unknown) {
  return e instanceof BackendError && "message" in e.error ? e.error.message : String(e);
}

export function useOrganize() {
  const tabs = useTabs();
  const ui = useUi();
  const t = i18n.global.t;

  /** Applique `op` ; `sel` : sélection après l'opération. Faux si refusée ou en échec. */
  async function run(tab: DocTab, op: PageOp, sel?: number[]): Promise<boolean> {
    if (!(await guardSigned(tab, "organize"))) return false;
    try {
      await tabs.applyPages(tab, op);
      if (sel) tab.orgSel = sel.filter((i) => i < tabs.pageCount(tab));
      return true;
    } catch (e) {
      ui.notify(t("organize.failed", { msg: message(e) }));
      return false;
    }
  }

  const count = (tab: DocTab) => tabs.pageCount(tab);

  function rotate(tab: DocTab, delta: 90 | -90) {
    const sel = sorted(tab.orgSel);
    if (sel.length) return run(tab, { op: "rotate", pages: sel, delta }, sel);
  }

  /** Rotation de pages données (miniatures de la barre latérale). */
  function rotatePages(tab: DocTab, pages: number[], delta: 90 | -90) {
    return run(tab, { op: "rotate", pages: sorted(pages), delta });
  }

  function remove(tab: DocTab) {
    const sel = sorted(tab.orgSel);
    if (!sel.length) return;
    if (sel.length >= count(tab)) {
      ui.notify(t("organize.keepOne"));
      return;
    }
    return run(tab, { op: "delete", pages: sel }, [Math.min(sel[0], count(tab) - sel.length - 1)]);
  }

  function duplicate(tab: DocTab) {
    const sel = sorted(tab.orgSel);
    // Chaque copie suit son original : les originaux restent sélectionnés.
    if (sel.length) return run(tab, { op: "duplicate", pages: sel }, sel.map((p, k) => p + k));
  }

  function insertBlank(tab: DocTab) {
    const sel = sorted(tab.orgSel);
    const at = sel.length ? sel[sel.length - 1] + 1 : count(tab);
    return run(tab, { op: "insertBlank", at }, [at]);
  }

  function move(tab: DocTab, pages: number[], to: number) {
    const sel = sorted(pages);
    return run(tab, { op: "move", pages: sel, to }, movedIndices(count(tab), sel, to));
  }

  /** Copie de pages de `src` dans `dst`, avant l'indice `at`. */
  function copyPages(dst: DocTab, src: DocTab, pages: number[], at: number) {
    if (!src.info) return;
    return importFrom(dst, src.info.id, pages, at);
  }

  /** Copie des pages d'un document du moteur (onglet, presse-papiers, fichier déposé). */
  function importFrom(dst: DocTab, from: number, pages: number[], at: number) {
    const sel = sorted(pages);
    return run(dst, { op: "import", from, pages: sel, at }, sel.map((_, k) => at + k));
  }

  async function extract(tab: DocTab) {
    const sel = sorted(tab.orgSel);
    if (!sel.length || !tab.info) return;
    const suffix = t("organize.extractSuffix");
    const defaultPath = /\.pdf$/i.test(tab.path) ? tab.path.replace(/\.pdf$/i, ` (${suffix}).pdf`) : `${tab.path} (${suffix})`;
    let path: string | null;
    if (inTauri) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      path = await save({ defaultPath, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    } else {
      path = (window as unknown as { __LUNAS_PDF_E2E__?: { extractPath?: string } }).__LUNAS_PDF_E2E__?.extractPath ?? null;
    }
    if (!path) return;
    try {
      await unwrap(commands.extractPages(tab.info.id, sel, path));
      ui.notify(t("organize.extracted", { n: sel.length, name: path.split(/[\\/]/).pop() ?? path }, sel.length));
    } catch (e) {
      ui.notify(t("organize.failed", { msg: message(e) }));
    }
  }

  return { rotate, rotatePages, remove, duplicate, insertBlank, move, copyPages, importFrom, extract };
}
