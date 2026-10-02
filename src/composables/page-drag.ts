// Glisser-déposer de pages depuis les miniatures de la barre latérale : réordonner dans le
// document, ou copier dans un autre onglet (survoler son onglet l'affiche, comme dans un
// navigateur). L'état est global : l'onglet source peut être masqué pendant le glisser.
import { ref } from "vue";
import { isNoop } from "../lib/page-order";
import { type DocTab, useTabs } from "../stores/tabs";
import { useOrganize } from "./organize";

export interface DropTarget {
  tab: DocTab;
  at: number;
}

interface DragState {
  src: DocTab;
  pages: number[];
  x: number;
  y: number;
  target: DropTarget | null;
  /** Onglet survolé dans la barre de titre (affiché après un court délai). */
  spring: string | null;
}

export const pageDrag = ref<DragState | null>(null);
/** Fichiers PDF glissés depuis le gestionnaire de fichiers au-dessus des miniatures. */
export const fileDrop = ref<DropTarget | null>(null);

/** Point d'insertion dans les miniatures sous (x, y), s'il y en a. */
export function thumbsTarget(x: number, y: number): DropTarget | null {
  const tabs = useTabs();
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  const list = el?.closest<HTMLElement>("[data-thumbs]");
  if (!list) return null;
  const tab = tabs.tabs.find((t) => t.key === list.dataset.thumbs && t.status === "ready");
  if (!tab?.info) return null;
  const items = [...list.querySelectorAll<HTMLElement>("[data-index]")];
  for (const it of items) {
    const r = it.getBoundingClientRect();
    if (y < r.top + r.height / 2) return { tab, at: Number(it.dataset.index) };
    if (y < r.bottom) return { tab, at: Number(it.dataset.index) + 1 };
  }
  // Sous la dernière miniature affichée (liste virtualisée) : après elle.
  const last = items.at(-1);
  return { tab, at: last ? Number(last.dataset.index) + 1 : tab.info.pages.length };
}

interface Press {
  src: DocTab;
  index: number;
  x: number;
  y: number;
}
let press: Press | null = null;
let springTimer = 0;
let scrollTimer = 0;

export function usePageDrag() {
  const tabs = useTabs();
  const org = useOrganize();

  function press_(src: DocTab, index: number, e: PointerEvent) {
    if (e.button !== 0) return;
    press = { src, index, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
    window.addEventListener("keydown", onKey, true);
  }

  function spring(x: number, y: number) {
    const d = pageDrag.value!;
    const tabEl = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>("[data-tab-key]");
    const key = tabEl?.dataset.tabKey ?? null;
    if (key === d.spring) return;
    d.spring = key;
    window.clearTimeout(springTimer);
    if (key && key !== tabs.activeKey) springTimer = window.setTimeout(() => (tabs.activeKey = key), 600);
  }

  function autoScroll(x: number, y: number) {
    window.clearInterval(scrollTimer);
    const list = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>("[data-thumbs]");
    const scroller = list?.closest<HTMLElement>(".scroll");
    if (!scroller) return;
    const r = scroller.getBoundingClientRect();
    const dir = y < r.top + 40 ? -1 : y > r.bottom - 40 ? 1 : 0;
    if (dir) scrollTimer = window.setInterval(() => (scroller.scrollTop += dir * 12), 16);
  }

  function onMove(e: PointerEvent) {
    if (!press) return;
    if (!pageDrag.value) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
      pageDrag.value = { src: press.src, pages: [press.index], x: e.clientX, y: e.clientY, target: null, spring: null };
    }
    const d = pageDrag.value;
    d.x = e.clientX;
    d.y = e.clientY;
    d.target = thumbsTarget(e.clientX, e.clientY);
    spring(e.clientX, e.clientY);
    autoScroll(e.clientX, e.clientY);
  }

  function stop() {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("keydown", onKey, true);
    window.clearTimeout(springTimer);
    window.clearInterval(scrollTimer);
    press = null;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && pageDrag.value) {
      e.preventDefault();
      e.stopPropagation();
      pageDrag.value = null;
      stop();
    }
  }

  async function onUp() {
    const d = pageDrag.value;
    stop();
    pageDrag.value = null;
    if (!d?.target || !d.src.info) return;
    const { tab, at } = d.target;
    if (tab.key === d.src.key) {
      if (isNoop(d.src.info.pages.length, d.pages, at)) return;
      if (await org.move(d.src, d.pages, at)) tabs.goto(d.src, d.src.orgSel[0] ?? at);
    } else if (await org.copyPages(tab, d.src, d.pages, at)) {
      tabs.goto(tab, at);
    }
  }

  return { press: press_ };
}
