import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import { Channel } from "@tauri-apps/api/core";
import type { Annot, AnnotOp, DocInfo, EditState, OutlineItem, PageOp, PdfError, Rect, SearchEvent, SearchHit, SignatureInfo } from "../bindings";
import { BackendError, commands, unwrap } from "../lib/api";
import { bitmaps } from "../lib/bitmap-cache";
import { dropDocData } from "../lib/page-data";
import { clampZoom, type FitMode, type ViewMode, zoomStep } from "../lib/layout";
import { useSettings } from "./settings";

export type SidebarTab = "thumbs" | "outline" | "annots" | "search";

export interface SearchState {
  query: string;
  caseSensitive: boolean;
  id: number;
  hits: SearchHit[];
  current: number;
  running: boolean;
}

export interface NavRequest {
  page: number;
  /** Position verticale dans la page, en points (sinon : haut de page). */
  yPt?: number;
  seq: number;
}

export interface DocTab {
  key: string;
  path: string;
  name: string;
  status: "loading" | "locked" | "ready" | "error";
  wrongPassword: boolean;
  error: PdfError | null;
  info: DocInfo | null;
  mode: ViewMode;
  zoom: number;
  fit: FitMode;
  page: number;
  sidebarOpen: boolean;
  sidebarTab: SidebarTab;
  search: SearchState;
  outline: OutlineItem[] | null;
  /** Modèle d'annotations (chargé à la demande) et état d'annulation. */
  edit: EditState | null;
  /** Mode annotation (barre d'outils secondaire ouverte). */
  annotating: boolean;
  /** Annotation sélectionnée. */
  selected: string | null;
  /** Révision du rendu de chaque page (invalide le cache des bitmaps). */
  pageRev: Record<number, number>;
  dirty: boolean;
  nav: NavRequest | null;
  epoch: number;
  /** Annotation à faire clignoter après navigation depuis la liste. */
  flash: { page: number; rect: Rect; seq: number } | null;
  /** Champ de formulaire ayant le focus, et demande de focus (navigation Tab). */
  focusedField: string | null;
  formFocus: { id: string; seq: number } | null;
  /** Bandeau « Ce document contient un formulaire » masqué. */
  formBannerHidden: boolean;
  /** Signatures numériques vérifiées (null : pas encore lues). */
  signatures: SignatureInfo[] | null;
  /** Panneau « Signature numérique » ouvert, signature mise en avant, bandeau masqué. */
  sigPanel: boolean;
  sigSelected: number;
  sigBannerHidden: boolean;
  /** Mode « Organiser les pages » (planche 08), pages sélectionnées dans la grille. */
  organizing: boolean;
  orgSel: number[];
  /** Document affiché côte à côte (clé d'onglet). */
  orgSide: string | null;
  /** Position de défilement mémorisée (changement d'onglet). */
  scroll?: { top: number; left: number };
}

let nextKey = 1;
let nextSearchId = 1;
let navSeq = 1;

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

export const useTabs = defineStore("tabs", () => {
  const tabs = ref<DocTab[]>([]);
  const activeKey = ref<string | null>(null);
  const active = computed(() => tabs.value.find((t) => t.key === activeKey.value) ?? null);
  const settings = useSettings();

  function newTab(path: string): DocTab {
    return reactive<DocTab>({
      key: `t${nextKey++}`,
      path,
      name: fileName(path),
      status: "loading",
      wrongPassword: false,
      error: null,
      info: null,
      mode: "continuous",
      zoom: 1,
      fit: "width",
      page: 0,
      sidebarOpen: settings.settings.sidebarOpen,
      sidebarTab: "thumbs",
      search: { query: "", caseSensitive: false, id: 0, hits: [], current: -1, running: false },
      outline: null,
      edit: null,
      annotating: false,
      selected: null,
      pageRev: {},
      dirty: false,
      nav: null,
      epoch: 0,
      flash: null,
      focusedField: null,
      formFocus: null,
      formBannerHidden: false,
      signatures: null,
      sigPanel: false,
      sigSelected: 0,
      sigBannerHidden: false,
      organizing: false,
      orgSel: [],
      orgSide: null,
    }) as DocTab;
  }

  async function load(tab: DocTab, password: string | null = null, remember = false) {
    tab.status = "loading";
    try {
      tab.info = await unwrap(commands.openDocument(tab.path, password, remember));
      tab.status = "ready";
      tab.wrongPassword = false;
      tab.name = tab.info.name;
      // Formulaire : champs chargés d'emblée pour la saisie.
      if (tab.info.form === "acroForm") void loadAnnotations(tab);
      if (tab.info.signatureCount > 0) void loadSignatures(tab);
    } catch (e) {
      const err = e instanceof BackendError ? e.error : ({ kind: "engine", message: String(e) } as PdfError);
      if (err.kind === "passwordRequired" || err.kind === "wrongPassword") {
        tab.status = "locked";
        tab.wrongPassword = err.kind === "wrongPassword";
      } else {
        tab.status = "error";
        tab.error = err;
      }
    }
  }

  async function loadSignatures(tab: DocTab) {
    if (!tab.info) return;
    try {
      tab.signatures = await unwrap(commands.getSignatures(tab.info.id));
    } catch (e) {
      console.error(e);
      tab.signatures = [];
    }
  }

  /** Revérifie les signatures de tous les onglets (autorités approuvées modifiées). */
  async function reloadAllSignatures() {
    await Promise.all(tabs.value.filter((t) => t.info?.signatureCount).map((t) => loadSignatures(t)));
  }

  /** Ouvre des documents (un onglet chacun) ; `activate` : affiche le dernier. */
  async function openPaths(paths: string[], activate = true): Promise<DocTab | null> {
    let last: DocTab | null = null;
    for (const path of paths) {
      const existing = tabs.value.find((t) => t.path === path);
      if (existing) {
        last = existing;
        continue;
      }
      const tab = newTab(path);
      tabs.value.push(tab);
      last = tab;
      void load(tabs.value.at(-1)!);
    }
    if (last && activate) activeKey.value = last.key;
    return last;
  }

  async function unlock(tab: DocTab, password: string, remember: boolean) {
    await load(tab, password, remember);
  }

  function close(key: string) {
    const i = tabs.value.findIndex((t) => t.key === key);
    if (i < 0) return;
    const [tab] = tabs.value.splice(i, 1);
    if (tab.info) {
      void commands.cancelSearch(tab.info.id);
      void commands.closeDocument(tab.info.id);
      bitmaps.dropDoc(tab.info.id);
      dropDocData(tab.info.id);
    }
    if (activeKey.value === key) activeKey.value = tabs.value[Math.min(i, tabs.value.length - 1)]?.key ?? null;
  }

  function cycle(dir: 1 | -1) {
    if (!tabs.value.length) return;
    const i = tabs.value.findIndex((t) => t.key === activeKey.value);
    activeKey.value = tabs.value[(i + dir + tabs.value.length) % tabs.value.length].key;
  }

  // --- Navigation et vue -------------------------------------------------------------------

  function pageCount(tab: DocTab) {
    return tab.info?.pages.length ?? 0;
  }

  function goto(tab: DocTab, page: number, yPt?: number) {
    const n = pageCount(tab);
    if (!n) return;
    const p = Math.min(Math.max(0, page), n - 1);
    tab.page = p;
    tab.nav = { page: p, yPt, seq: navSeq++ };
  }

  function step(tab: DocTab, dir: 1 | -1) {
    const by = tab.mode === "double" ? 2 : 1;
    const base = tab.mode === "double" ? tab.page - (tab.page % 2) : tab.page;
    goto(tab, base + dir * by);
  }

  function setZoom(tab: DocTab, zoom: number) {
    tab.fit = null;
    tab.zoom = clampZoom(zoom);
  }

  function zoomBy(tab: DocTab, dir: 1 | -1) {
    setZoom(tab, zoomStep(tab.zoom, dir));
  }

  function setFit(tab: DocTab, fit: FitMode) {
    tab.fit = fit;
  }

  function setMode(tab: DocTab, mode: ViewMode) {
    tab.mode = mode;
    tab.nav = { page: tab.page, seq: navSeq++ };
  }

  function toggleSidebar(tab: DocTab) {
    tab.sidebarOpen = !tab.sidebarOpen;
    void settings.update({ sidebarOpen: tab.sidebarOpen });
  }

  async function loadOutline(tab: DocTab) {
    if (tab.outline || !tab.info) return;
    tab.outline = await unwrap(commands.getOutline(tab.info.id)).catch(() => []);
  }

  // --- Annotations -------------------------------------------------------------------------

  function applyState(tab: DocTab, st: EditState) {
    if (st.pages && tab.info) {
      // Pages ajoutées, retirées, déplacées ou pivotées : tout est à redessiner.
      tab.info = { ...tab.info, pages: st.pages };
      bitmaps.dropDoc(tab.info.id);
      dropDocData(tab.info.id);
      for (let p = 0; p < st.pages.length; p++) tab.pageRev[p] = (tab.pageRev[p] ?? 0) + 1;
      tab.outline = null;
      if (tab.sidebarTab === "outline") void loadOutline(tab);
      tab.page = Math.min(tab.page, st.pages.length - 1);
      tab.orgSel = tab.orgSel.filter((i) => i < st.pages!.length);
      tab.search.hits = [];
    }
    for (const p of st.changedPages) tab.pageRev[p] = (tab.pageRev[p] ?? 0) + 1;
    tab.edit = st;
    tab.dirty = st.dirty;
    if (tab.selected && !st.annots.some((a) => a.id === tab.selected)) tab.selected = null;
  }

  async function loadAnnotations(tab: DocTab) {
    if (tab.edit || !tab.info) return;
    try {
      applyState(tab, await unwrap(commands.loadAnnotations(tab.info.id)));
    } catch (e) {
      console.error(e);
    }
  }

  /** File d'attente : les opérations d'un document s'appliquent dans l'ordre. */
  const queues = new Map<string, Promise<unknown>>();
  function enqueue<T>(tab: DocTab, f: () => Promise<T>): Promise<T> {
    const prev = queues.get(tab.key) ?? Promise.resolve();
    const next = prev.then(f, f);
    queues.set(tab.key, next.catch(() => {}));
    return next;
  }

  function editCall(tab: DocTab, call: (doc: number) => Promise<{ status: "ok"; data: EditState } | { status: "error"; error: PdfError }>) {
    return enqueue(tab, async () => {
      if (!tab.info) return;
      applyState(tab, await unwrap(call(tab.info.id)));
    });
  }

  function applyOps(tab: DocTab, ops: AnnotOp[]) {
    return editCall(tab, (doc) => commands.applyAnnotations(doc, ops));
  }
  const addAnnot = (tab: DocTab, annot: Annot) => applyOps(tab, [{ op: "add", annot, index: null }]);
  const updateAnnot = (tab: DocTab, annot: Annot) => applyOps(tab, [{ op: "update", annot }]);
  const removeAnnot = (tab: DocTab, id: string) => applyOps(tab, [{ op: "remove", id }]);
  const undo = (tab: DocTab) => editCall(tab, (doc) => commands.undo(doc));
  const redo = (tab: DocTab) => editCall(tab, (doc) => commands.redo(doc));
  const applyPages = (tab: DocTab, op: PageOp) => editCall(tab, (doc) => commands.applyPages(doc, op));
  const setHidden = (tab: DocTab, id: string, hidden: boolean) => editCall(tab, (doc) => commands.setAnnotationHidden(doc, id, hidden));

  function toggleOrganizing(tab: DocTab, on = !tab.organizing) {
    tab.organizing = on;
    if (on) {
      tab.annotating = false;
      tab.selected = null;
      tab.orgSel = [tab.page];
      void loadAnnotations(tab);
    } else {
      tab.orgSide = null;
      if (tab.orgSel.length) goto(tab, tab.orgSel[0]);
    }
  }

  function toggleAnnotating(tab: DocTab, on = !tab.annotating) {
    tab.annotating = on;
    if (on) void loadAnnotations(tab);
    else tab.selected = null;
  }

  /** Enregistre (sur place ou sous `path`). */
  async function save(tab: DocTab, path: string | null = null) {
    if (!tab.info) return;
    const redacted = !!tab.edit?.pendingRedactions;
    await enqueue(tab, async () => {
      const info = await unwrap(commands.saveDocument(tab.info!.id, path));
      tab.info = info;
      tab.name = info.name;
      tab.path = info.path;
      tab.edit = null;
      tab.dirty = false;
      if (redacted) {
        // Le contenu des pages a changé : rendus et textes à refaire.
        bitmaps.dropDoc(info.id);
        dropDocData(info.id);
        for (let p = 0; p < info.pages.length; p++) tab.pageRev[p] = (tab.pageRev[p] ?? 0) + 1;
      }
    });
    await loadAnnotations(tab);
    if (tab.info?.signatureCount) await loadSignatures(tab);
  }

  // --- Recherche ---------------------------------------------------------------------------

  function search(tab: DocTab, query: string, caseSensitive = tab.search.caseSensitive) {
    if (!tab.info) return;
    const s = tab.search;
    s.query = query;
    s.caseSensitive = caseSensitive;
    s.hits = [];
    s.current = -1;
    if (!query.trim()) {
      s.running = false;
      void commands.cancelSearch(tab.info.id);
      if (tab.sidebarTab === "search") tab.sidebarTab = "thumbs";
      return;
    }
    const id = nextSearchId++;
    s.id = id;
    s.running = true;
    tab.sidebarTab = "search";
    const channel = new Channel<SearchEvent>();
    channel.onmessage = (ev) => {
      if (ev.searchId !== s.id) return;
      if (ev.type === "page") {
        if (ev.hits.length) {
          s.hits.push(...ev.hits);
          if (s.current < 0) {
            // Premier résultat à partir de la page courante.
            if (ev.page >= tab.page || s.hits.length === ev.hits.length) selectHit(tab, s.hits.length - ev.hits.length);
          }
        }
      } else {
        s.running = false;
        if (s.current < 0 && s.hits.length) selectHit(tab, 0);
      }
    };
    void commands.search(tab.info.id, id, query, caseSensitive, channel);
  }

  function selectHit(tab: DocTab, i: number) {
    const s = tab.search;
    if (!s.hits.length) return;
    s.current = (i + s.hits.length) % s.hits.length;
    const hit = s.hits[s.current];
    goto(tab, hit.page, hit.rects[0]?.y);
  }

  function nextHit(tab: DocTab, dir: 1 | -1) {
    if (tab.search.hits.length) selectHit(tab, tab.search.current + dir);
  }

  function bumpEpoch(tab: DocTab) {
    if (!tab.info) return;
    tab.epoch++;
    void commands.setRenderEpoch(tab.info.id, tab.epoch);
  }

  return {
    tabs,
    activeKey,
    active,
    openPaths,
    unlock,
    close,
    cycle,
    goto,
    step,
    setZoom,
    zoomBy,
    setFit,
    setMode,
    toggleSidebar,
    loadOutline,
    loadAnnotations,
    loadSignatures,
    reloadAllSignatures,
    applyOps,
    addAnnot,
    updateAnnot,
    removeAnnot,
    undo,
    redo,
    setHidden,
    toggleAnnotating,
    toggleOrganizing,
    applyPages,
    save,
    search,
    selectHit,
    nextHit,
    bumpEpoch,
    pageCount,
  };
});
