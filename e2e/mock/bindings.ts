// Backend simulé pour les tests de parcours (vite --mode e2e) : remplace src/bindings.ts.
// Mêmes signatures que les commandes générées par tauri-specta, données déterministes.
import type {
  Annot, AnnotOp, DocInfo, EditState, ImageInfo, LinkInfo, OpenFilesEvent, OutlineItem, PageText, PdfError, Point, RecentDoc, Rect, SearchEvent, SearchHit,
  Settings, TextRun, TextSelection,
} from "../../src/bindings";

export type * from "../../src/bindings";

// --- Pont IPC minimal (Channel de @tauri-apps/api) ---------------------------------------------
const callbacks = new Map<number, (m: unknown) => void>();
let nextCb = 1;
const w = window as unknown as Record<string, unknown>;
w.__FEUILLET_E2E__ = w.__FEUILLET_E2E__ ?? {};
w.__TAURI_INTERNALS__ = {
  transformCallback: (cb: (m: unknown) => void) => {
    const id = nextCb++;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback: (id: number) => callbacks.delete(id),
};
const e2e = w.__FEUILLET_E2E__ as { pending?: string[]; calls?: string[]; settings?: Partial<Settings>; saved?: Record<string, Annot[]> };
e2e.calls = [];

// --- Documents simulés ---------------------------------------------------------------------------
const A4 = { width: 595.28, height: 841.89, label: null };
const WORDS = "Le prix forfaitaire est payable selon les conditions prévues au présent contrat".split(" ");

interface MockDoc {
  pages: number;
  password?: string;
  form?: DocInfo["form"];
  signed?: boolean;
}
const DOCS: Record<string, MockDoc> = {
  "/docs/contrat.pdf": { pages: 12 },
  "/docs/long.pdf": { pages: 320 },
  "/docs/protege.pdf": { pages: 3, password: "feuillet" },
  "/docs/xfa.pdf": { pages: 2, form: "xfa" },
};

let nextId = 1;
const open = new Map<number, string>();

const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v));

// --- Modèle d'annotations simulé (même sémantique que l'écrivain Rust) ---------------------------
interface Ed {
  annots: Annot[];
  saved: Annot[];
  undo: AnnotOp[][];
  redo: AnnotOp[][];
}
const editors = new Map<number, Ed>();
function editor(doc: number): Ed {
  let ed = editors.get(doc);
  if (!ed) {
    const initial: Annot[] = [
      { id: "ex1", page: 1, rect: { x: 72, y: 98, w: 200, h: 12 }, color: "#ffd43b", opacity: 1, width: 1, contents: "À vérifier", author: "Claire", modified: "D:20261002083800Z", excerpt: "Le prix forfaitaire", body: { type: "highlight", quads: [{ x: 72, y: 98, w: 200, h: 12 }] }, hidden: false },
      { id: "ex2", page: 5, rect: { x: 500, y: 60, w: 22, h: 22 }, color: "#ffd43b", opacity: 1, width: 1, contents: "Note de relecture", author: null, modified: null, excerpt: null, body: { type: "note" }, hidden: false },
    ];
    ed = { annots: clone(initial), saved: clone(initial), undo: [], redo: [] };
    editors.set(doc, ed);
  }
  return ed;
}
function applyOps(list: Annot[], ops: AnnotOp[]): AnnotOp[] {
  const inv: AnnotOp[] = [];
  for (const op of clone(ops)) {
    if (op.op === "add") {
      const a = { ...op.annot, author: op.annot.author ?? "Moi", modified: "D:20261002104200Z" };
      list.splice(op.index ?? list.length, 0, a);
      inv.push({ op: "remove", id: a.id });
    } else if (op.op === "update") {
      const i = list.findIndex((x) => x.id === op.annot.id);
      if (i < 0) continue;
      inv.push({ op: "update", annot: { ...list[i], hidden: false } });
      list[i] = { ...op.annot, modified: "D:20261002104300Z" };
    } else {
      const i = list.findIndex((x) => x.id === op.id);
      if (i < 0) continue;
      inv.push({ op: "add", annot: { ...list[i], hidden: false }, index: i });
      list.splice(i, 1);
    }
  }
  return inv.reverse();
}
function pagesOf(ops: AnnotOp[], ed: Ed): number[] {
  return [...new Set(ops.map((o) => (o.op === "remove" ? (ed.annots.find((a) => a.id === o.id)?.page ?? 0) : o.annot.page)))];
}
function state(doc: number, changedPages: number[]): EditState {
  const ed = editor(doc);
  const key = (a: Annot) => JSON.stringify({ ...a, hidden: false });
  const unsaved = ed.annots.filter((a) => !ed.saved.some((s) => key(s) === key(a))).length + ed.saved.filter((s) => !ed.annots.some((a) => a.id === s.id)).length;
  (e2e.saved ??= {})[open.get(doc) ?? ""] = ed.annots;
  return {
    annots: clone(ed.annots),
    changedPages,
    canUndo: ed.undo.length > 0,
    canRedo: ed.redo.length > 0,
    dirty: unsaved > 0,
    pendingRedactions: ed.annots.some((a) => a.body.type === "redact"),
    unsavedCount: unsaved,
    author: "Moi",
  };
}

function lines(page: number): string[] {
  // « échéance » sur les pages 3 (×2), 4 et 7.
  const out = [`Article ${page + 1}`];
  for (let i = 0; i < 12; i++) out.push(WORDS.slice(0, 6 + ((page + i) % 5)).join(" "));
  if (page === 2) out.push("Chaque échéance donne lieu à une facture.", "Les factures suivent l’échéance.");
  if (page === 3 || page === 6) out.push("Le report d’une Échéance ne peut excéder trente jours.");
  return out;
}

function runs(page: number): TextRun[] {
  const out: TextRun[] = [];
  lines(page).forEach((line, li) => {
    let x = 72;
    const words = line.split(" ");
    words.forEach((word, wi) => {
      const w = word.length * 5.6;
      out.push({ text: wi < words.length - 1 ? word + " " : word, rect: { x, y: 80 + li * 18, w, h: 11 }, eol: wi === words.length - 1 });
      x += w + 3;
    });
  });
  return out;
}

type Res<T> = Promise<{ status: "ok"; data: T } | { status: "error"; error: PdfError }>;
const ok = async <T,>(data: T): Res<T> => ({ status: "ok", data });
const err = async <T,>(error: PdfError): Res<T> => ({ status: "error", error });
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

let recents: RecentDoc[] = [
  { path: "/docs/contrat.pdf", name: "contrat.pdf", openedAt: Date.now() - 2 * 3600e3, pageCount: 12, locked: false, signed: true, form: false, thumb: null },
  { path: "/docs/protege.pdf", name: "protege.pdf", openedAt: Date.now() - 26 * 3600e3, pageCount: 0, locked: true, signed: false, form: false, thumb: null },
];
let settings: Settings = { theme: "light", accent: "#3466f6", language: "fr", windowControls: true, recentsView: "grid", sidebarOpen: true, ...e2e.settings };

export const commands = {
  async openDocument(path: string, password: string | null, _remember: boolean): Res<DocInfo> {
    e2e.calls!.push(`open:${path}`);
    await delay(30);
    const d = DOCS[path];
    if (!d) return err({ kind: "notFound", message: path });
    if (d.password && password !== d.password) return err({ kind: password ? "wrongPassword" : "passwordRequired" });
    const id = nextId++;
    open.set(id, path);
    return ok({
      id,
      path,
      name: path.split("/").pop()!,
      title: null,
      pages: Array.from({ length: d.pages }, () => ({ ...A4 })),
      encrypted: !!d.password,
      form: d.form ?? "none",
      signatureCount: d.signed ? 1 : 0,
      canCopy: true,
      canPrint: true,
    });
  },
  async closeDocument(doc: number) {
    open.delete(doc);
  },
  async setRenderEpoch(_doc: number, _epoch: number) {},
  async getOutline(_doc: number): Res<OutlineItem[]> {
    return ok([
      { title: "Conditions générales", page: 0, children: [{ title: "Objet", page: 1, children: [] }] },
      { title: "Prix et paiement", page: 2, children: [] },
      { title: "Annexes", page: 8, children: [] },
    ]);
  },
  async loadAnnotations(doc: number): Res<EditState> {
    return ok(state(doc, []));
  },
  async applyAnnotations(doc: number, ops: AnnotOp[]): Res<EditState> {
    const ed = editor(doc);
    const inverse = applyOps(ed.annots, ops);
    ed.undo.push(inverse);
    ed.redo = [];
    return ok(state(doc, pagesOf(ops, ed)));
  },
  async undo(doc: number): Res<EditState> {
    const ed = editor(doc);
    const ops = ed.undo.pop();
    if (!ops) return ok(state(doc, []));
    ed.redo.push(applyOps(ed.annots, ops));
    return ok(state(doc, pagesOf(ops, ed)));
  },
  async redo(doc: number): Res<EditState> {
    const ed = editor(doc);
    const ops = ed.redo.pop();
    if (!ops) return ok(state(doc, []));
    ed.undo.push(applyOps(ed.annots, ops));
    return ok(state(doc, pagesOf(ops, ed)));
  },
  async setAnnotationHidden(doc: number, id: string, hidden: boolean): Res<EditState> {
    const a = editor(doc).annots.find((x) => x.id === id);
    if (a) a.hidden = hidden;
    return ok(state(doc, a ? [a.page] : []));
  },
  async textRange(_doc: number, page: number, from: Point, to: Point): Res<TextSelection> {
    const ls = lines(page);
    const li = (y: number) => Math.min(ls.length - 1, Math.max(0, Math.floor((y - 80 + 3) / 18)));
    let [a, b] = [li(from.y), li(to.y)];
    let [fx, tx] = [from.x, to.x];
    if (a > b || (a === b && fx > tx)) [a, b, fx, tx] = [b, a, tx, fx];
    const rects: Rect[] = [];
    for (let l = a; l <= b; l++) {
      const end = 72 + ls[l].length * 8.6;
      const x0 = l === a ? Math.max(72, fx) : 72;
      const x1 = l === b ? Math.min(end, tx) : end;
      if (x1 > x0) rects.push({ x: x0, y: 80 + l * 18, w: x1 - x0, h: 11 });
    }
    return ok({ rects, text: ls.slice(a, b + 1).join(" ") });
  },
  async importImage(_doc: number, path: string): Res<ImageInfo> {
    return ok({ key: `img:${path}`, width: 200, height: 100 });
  },
  async copyImage(_from: number, _to: number, _key: string): Res<null> {
    return ok(null);
  },
  async saveDocument(doc: number, path: string | null): Res<DocInfo> {
    e2e.calls!.push(`save:${path ?? open.get(doc)}`);
    const ed = editor(doc);
    ed.annots = ed.annots.filter((a) => a.body.type !== "redact");
    ed.saved = clone(ed.annots);
    ed.undo = [];
    ed.redo = [];
    const p = path ?? open.get(doc)!;
    return ok({ ...(await commands.openDocument(open.get(doc)!, "feuillet", false).then((r) => (r.status === "ok" ? r.data : null)))!, id: doc, path: p, name: p.split("/").pop()! });
  },
  async getLinks(_doc: number, page: number): Res<LinkInfo[]> {
    return ok(page === 0 ? [{ rect: { x: 72, y: 700, w: 200, h: 14 }, target: { type: "page", page: 8 } }] : []);
  },
  async getPageText(_doc: number, page: number): Res<PageText> {
    return ok({ runs: runs(page) });
  },
  async search(doc: number, searchId: number, query: string, caseSensitive: boolean, onEvent: { id: number }) {
    const pages = DOCS[open.get(doc)!]?.pages ?? 0;
    const send = (index: number, message: SearchEvent) => callbacks.get(onEvent.id)?.({ index, message });
    const norm = (s: string) => (caseSensitive ? s : s.toLowerCase());
    let total = 0;
    let i = 0;
    for (let p = 0; p < pages; p++) {
      const hits: SearchHit[] = [];
      lines(p).forEach((line, li) => {
        let at = norm(line).indexOf(norm(query));
        while (at >= 0) {
          hits.push({ page: p, rects: [{ x: 72 + at * 8.6, y: 80 + li * 18, w: query.length * 8.6, h: 11 }], before: line.slice(0, at), matched: line.slice(at, at + query.length), after: line.slice(at + query.length) });
          at = norm(line).indexOf(norm(query), at + 1);
        }
      });
      total += hits.length;
      send(i++, { type: "page", searchId, page: p, hits });
      if (p % 50 === 49) await delay(1);
    }
    send(i++, { type: "done", searchId, total });
  },
  async cancelSearch(_doc: number) {},
  async getSettings(): Promise<Settings> {
    return settings;
  },
  async setSettings(s: Settings): Res<null> {
    settings = s;
    return ok(null);
  },
  async getRecents(): Promise<RecentDoc[]> {
    return recents;
  },
  async removeRecent(path: string): Res<null> {
    recents = recents.filter((r) => r.path !== path);
    return ok(null);
  },
  async clearRecents(): Res<null> {
    recents = [];
    return ok(null);
  },
  async systemLocale(): Promise<string> {
    return "fr-FR";
  },
  async logFrontendError(message: string) {
    console.error(message);
  },
  async takePendingFiles(): Promise<string[]> {
    const p = e2e.pending ?? [];
    e2e.pending = [];
    return p;
  },
};

export const events = {
  openFilesEvent: {
    listen: async (cb: (e: { payload: OpenFilesEvent }) => void) => {
      (w.__FEUILLET_E2E__ as Record<string, unknown>).emitOpen = (paths: string[]) => cb({ payload: { paths } });
      return () => {};
    },
  },
};
