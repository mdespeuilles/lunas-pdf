// Copier, couper, coller et dupliquer des annotations. Presse-papiers interne, commun aux
// onglets : une annotation se colle aussi dans un autre document.
import { ref } from "vue";
import type { Annot } from "../bindings";
import { commands, unwrap } from "../lib/api";
import { canCopy, pasted } from "../lib/annot-geom";
import { useAnnotTools } from "../stores/annotTools";
import { type DocTab, useTabs } from "../stores/tabs";

const STEP = 12;

interface Clip {
  annot: Annot;
  doc: number;
  /** Coupée : le premier collage reprend sa place exacte. */
  cut: boolean;
  /** Dernier collage, pour décaler les suivants en cascade. */
  last: { doc: number; page: number; n: number } | null;
}

const clip = ref<Clip | null>(null);

function selectedOf(tab: DocTab) {
  const a = tab.selected ? tab.edit?.annots.find((x) => x.id === tab.selected) : undefined;
  return a && canCopy(a) ? a : undefined;
}

export function useAnnotClipboard() {
  const tabs = useTabs();
  const tools = useAnnotTools();

  /** Copie l'annotation sélectionnée ; faux s'il n'y en a pas (le texte suit alors son cours). */
  function copy(tab: DocTab, cut = false): boolean {
    const a = selectedOf(tab);
    if (!a || !tab.info) return false;
    clip.value = { annot: JSON.parse(JSON.stringify(a)) as Annot, doc: tab.info.id, cut, last: null };
    if (cut) void tabs.removeAnnot(tab, a.id);
    return true;
  }

  /** Oublie l'annotation copiée (une copie de texte la remplace). */
  function forget() {
    clip.value = null;
  }

  const canPaste = () => !!clip.value;

  /** Colle sur la page courante, à la même position (décalée si elle recouvrirait l'original). */
  async function paste(tab: DocTab) {
    const c = clip.value;
    if (!c || !tab.info) return;
    const doc = tab.info.id;
    const page = tab.page;
    const n = c.last && c.last.doc === doc && c.last.page === page ? c.last.n + 1 : !c.cut && doc === c.doc && page === c.annot.page ? 1 : 0;
    c.last = { doc, page, n };
    const geom = tab.info.pages[page];
    const copy = pasted(c.annot, page, geom.width, geom.height, n * STEP);
    try {
      if (copy.body.type === "image" && doc !== c.doc) await unwrap(commands.copyImage(c.doc, doc, copy.body.image));
      if (!tab.edit) await tabs.loadAnnotations(tab);
      tabs.toggleAnnotating(tab, true);
      tools.tool = "select";
      await tabs.addAnnot(tab, copy);
      tab.selected = copy.id;
    } catch (e) {
      console.error(e);
    }
  }

  async function duplicate(tab: DocTab) {
    const a = selectedOf(tab);
    const geom = a && tab.info?.pages[a.page];
    if (!a || !geom) return;
    const copy = pasted(a, a.page, geom.width, geom.height, STEP);
    await tabs.addAnnot(tab, copy);
    tab.selected = copy.id;
  }

  return { copy, forget, canPaste, paste, duplicate };
}
