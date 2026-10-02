// Raccourcis clavier globaux (repris des aria-label des planches).
import { onBeforeUnmount, onMounted } from "vue";
import { moved, canMove } from "../lib/annot-geom";
import { TOOL_KEYS, useAnnotTools } from "../stores/annotTools";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";
import { useAnnotClipboard } from "./clipboard";
import { useOrganize } from "./organize";
import { usePageClipboard } from "./page-clipboard";
import { printTab } from "./print";
import { stops, useForm } from "./form";
import { markupFromSelection } from "./markup";
import { openFromDialog } from "./open";
import { requestClose, saveTab } from "./save";
import { toggleAnnotate } from "./signed";

export interface ReaderHandle {
  focusSearch(): void;
  toggleCase(): void;
  focusPage(): void;
}

function isTyping(e: KeyboardEvent) {
  const el = e.target as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable);
}

export function useShortcuts(reader: () => ReaderHandle | undefined) {
  const tabs = useTabs();
  const ui = useUi();
  const tools = useAnnotTools();
  const clipboard = useAnnotClipboard();
  const form = useForm();
  const organize = useOrganize();
  const pageClip = usePageClipboard();

  function onKey(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    const modal = ui.prefsOpen || ui.aboutOpen || !!ui.ask;

    // Application
    if (ctrl && !e.shiftKey && key === "o") return run(e, () => openFromDialog());
    if (ctrl && key === ",") return run(e, () => (ui.prefsOpen = true));
    if (modal) return;
    if (ctrl && key === "w") return run(e, () => tabs.active && requestClose(tabs.active));
    if (ctrl && (e.key === "Tab" || e.key === "PageDown" || e.key === "PageUp"))
      return run(e, () => tabs.cycle(e.key === "PageUp" || (e.key === "Tab" && e.shiftKey) ? -1 : 1));

    const tab = tabs.active;
    if (!tab || tab.status !== "ready") return;
    const typing = isTyping(e);

    // Enregistrement et annotation
    if (ctrl && key === "s") return run(e, () => saveTab(tab, e.shiftKey));
    if (ctrl && e.shiftKey && key === "o") return run(e, () => tabs.toggleOrganizing(tab));
    if (ctrl && e.shiftKey && key === "e") return run(e, () => (ui.exportTab = tab.key));
    if (ctrl && !e.shiftKey && key === "p") return run(e, () => printTab(tab));
    if (tab.organizing) return organizeKeys(e, tab, typing, ctrl, key);
    if (ctrl && e.shiftKey && key === "a") return run(e, () => toggleAnnotate(tab));
    if (!typing && ctrl && (key === "y" || (key === "z" && e.shiftKey))) return run(e, () => tabs.redo(tab));
    if (!typing && ctrl && key === "z") return run(e, () => tabs.undo(tab));
    // Miniatures de la barre latérale : opérations sur la page qui a le focus.
    const thumb = (e.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-thumbs] [data-index]");
    if (thumb && !typing) {
      const i = Number(thumb.dataset.index);
      if (ctrl && !e.shiftKey && (key === "c" || key === "x")) return run(e, () => pageClip.copy(tab, [i], key === "x"));
      if (ctrl && !e.shiftKey && key === "v") return run(e, () => pageClip.paste(tab, i + 1));
      if (ctrl && !e.shiftKey && key === "d") return run(e, () => ((tab.orgSel = [i]), organize.duplicate(tab)));
      if (ctrl && key === "r") return run(e, () => organize.rotatePages(tab, [i], e.shiftKey ? -90 : 90));
      if (!ctrl && (e.key === "Delete" || e.key === "Backspace")) return run(e, () => ((tab.orgSel = [i]), organize.remove(tab)));
    }
    // Presse-papiers des annotations ; sans annotation sélectionnée, Ctrl C copie le texte.
    if (!typing && ctrl && !e.shiftKey && (key === "c" || key === "x")) {
      if (clipboard.copy(tab, key === "x")) return run(e, () => {});
      clipboard.forget();
    }
    if (!typing && ctrl && !e.shiftKey && key === "v" && clipboard.canPaste()) return run(e, () => clipboard.paste(tab));
    const sel = tab.selected ? tab.edit?.annots.find((a) => a.id === tab.selected) : undefined;
    if (!typing && sel) {
      if (e.key === "Delete" || e.key === "Backspace") return run(e, () => tabs.removeAnnot(tab, sel.id));
      if (ctrl && key === "d") return run(e, () => clipboard.duplicate(tab));
      const arrows: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
      if (arrows[e.key] && canMove(sel)) {
        const step = e.shiftKey ? 10 : 1;
        return run(e, () => tabs.updateAnnot(tab, moved(sel, arrows[e.key][0] * step, arrows[e.key][1] * step)));
      }
      if (e.key === "Escape") return run(e, () => (tab.selected = null));
    }
    if (e.key === "Escape" && tools.placing) return run(e, () => tools.cancelPlacing());
    if (!typing && !ctrl && !e.altKey && tab.annotating) {
      if (e.key === "Escape" && tools.tool !== "select") return run(e, () => (tools.tool = "select"));
      const tool = TOOL_KEYS[key];
      if (tool) {
        return run(e, async () => {
          // H / U / K sur une sélection de texte : marquage immédiat.
          if (["highlight", "underline", "strike"].includes(tool) && (await markupFromSelection(tab, tool))) return;
          if (tool === "signature" || tool === "image") {
            tools.request = { tool, seq: Date.now() };
            return;
          }
          tools.tool = tool;
          tab.selected = null;
        });
      }
    }

    // Formulaire : Tab depuis le document entre dans les champs (les contrôles gèrent la suite).
    if (e.key === "Tab" && !ctrl && !e.altKey && !tab.annotating && stops(tab.edit?.fields ?? []).length) {
      const el = document.activeElement;
      if (!el || el === document.body || el.matches(".scroller")) return run(e, () => form.step(tab, e.shiftKey ? -1 : 1));
    }
    if (e.key === "F9") return run(e, () => tabs.toggleSidebar(tab));
    if (ctrl && !e.shiftKey && key === "f") return run(e, () => reader()?.focusSearch());
    if (e.altKey && !ctrl && (key === "c" || e.code === "KeyC")) return run(e, () => reader()?.toggleCase());
    if (ctrl && key === "g") return run(e, () => reader()?.focusPage());
    if (ctrl && (e.key === "+" || e.key === "=" || e.code === "NumpadAdd")) return run(e, () => tabs.zoomBy(tab, 1));
    if (ctrl && (e.key === "-" || e.key === "−" || e.code === "NumpadSubtract")) return run(e, () => tabs.zoomBy(tab, -1));
    if (ctrl && (e.code === "Digit0" || e.code === "Numpad0")) return run(e, () => tabs.setZoom(tab, 1));
    if (ctrl && !e.shiftKey && (e.code === "Digit1" || e.code === "Numpad1")) return run(e, () => tabs.setMode(tab, "single"));
    if (ctrl && !e.shiftKey && (e.code === "Digit2" || e.code === "Numpad2")) return run(e, () => tabs.setMode(tab, "continuous"));
    if (ctrl && !e.shiftKey && (e.code === "Digit3" || e.code === "Numpad3")) return run(e, () => tabs.setMode(tab, "double"));

    // Les contrôles du formulaire (cases, radios) gardent leurs touches simples.
    if (typing || ctrl || e.altKey || (e.target as HTMLElement | null)?.closest?.(".flayer")) return;
    if (key === "w") return run(e, () => tabs.setFit(tab, "width"));
    if (key === "f") return run(e, () => tabs.setFit(tab, "page"));
    if (e.key === "ArrowLeft") return run(e, () => tabs.step(tab, -1));
    if (e.key === "ArrowRight") return run(e, () => tabs.step(tab, 1));
    if (e.key === "Home") return run(e, () => tabs.goto(tab, 0));
    if (e.key === "End") return run(e, () => tabs.goto(tab, tabs.pageCount(tab) - 1));
  }

  /** Mode « Organiser les pages » : actions sur la sélection du volet actif. */
  function organizeKeys(e: KeyboardEvent, tab: DocTab, typing: boolean, ctrl: boolean, key: string) {
    if (typing) return;
    const pane = document.activeElement?.closest<HTMLElement>("[data-pane]")?.dataset.pane;
    const target = tabs.tabs.find((t) => t.key === pane && t.status === "ready") ?? tab;
    if (ctrl && (key === "y" || (key === "z" && e.shiftKey))) return run(e, () => tabs.redo(target));
    if (ctrl && key === "z") return run(e, () => tabs.undo(target));
    if (ctrl && key === "a") return run(e, () => (target.orgSel = Array.from({ length: tabs.pageCount(target) }, (_, i) => i)));
    if (ctrl && key === "r") return run(e, () => organize.rotate(target, e.shiftKey ? -90 : 90));
    if (ctrl && !e.shiftKey && key === "d") return run(e, () => organize.duplicate(target));
    if (ctrl && !e.shiftKey && (key === "c" || key === "x")) return run(e, () => pageClip.copy(target, target.orgSel, key === "x"));
    if (ctrl && !e.shiftKey && key === "v") {
      const at = target.orgSel.length ? Math.max(...target.orgSel) + 1 : tabs.pageCount(target);
      return run(e, () => pageClip.paste(target, at));
    }
    if (ctrl && e.shiftKey && key === "n") return run(e, () => organize.insertBlank(target));
    if (!ctrl && (e.key === "Delete" || e.key === "Backspace")) return run(e, () => organize.remove(target));
    if (e.key === "Escape") return run(e, () => (target.orgSel.length ? (target.orgSel = []) : tabs.toggleOrganizing(tab, false)));
  }

  function run(e: KeyboardEvent, f: () => unknown) {
    e.preventDefault();
    f();
  }

  onMounted(() => window.addEventListener("keydown", onKey));
  onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
}
