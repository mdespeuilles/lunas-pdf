// Backend simulé pour les tests de parcours (vite --mode e2e) : remplace src/bindings.ts.
// Mêmes signatures que les commandes générées par tauri-specta, données déterministes.
import type {
  Annot, AnnotOp, DocInfo, EditState, FormField, SavedSignature, SignatureInfo, TrustedRoot, ImageInfo, LinkInfo, OpenFilesEvent, OutlineItem, PageText, PdfError, Point, RecentDoc, Rect, SearchEvent, SearchHit,
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
const e2e = w.__FEUILLET_E2E__ as { pending?: string[]; calls?: string[]; settings?: Partial<Settings>; saved?: Record<string, Annot[]>; fields?: Record<string, FormField[]> };
e2e.calls = [];

// --- Documents simulés ---------------------------------------------------------------------------
const A4 = { width: 595.28, height: 841.89, label: null };
const WORDS = "Le prix forfaitaire est payable selon les conditions prévues au présent contrat".split(" ");

interface MockDoc {
  pages: number;
  password?: string;
  form?: DocInfo["form"];
  signed?: boolean;
  sig?: SignatureInfo["status"];
}
const DOCS: Record<string, MockDoc> = {
  "/docs/contrat.pdf": { pages: 12 },
  "/docs/long.pdf": { pages: 320 },
  "/docs/protege.pdf": { pages: 3, password: "feuillet" },
  "/docs/xfa.pdf": { pages: 2, form: "xfa" },
  "/docs/formulaire.pdf": { pages: 2, form: "acroForm" },
  "/docs/signe.pdf": { pages: 2, signed: true, sig: "valid" },
  "/docs/signe-altere.pdf": { pages: 2, signed: true, sig: "invalid" },
  "/docs/signe-inconnu.pdf": { pages: 2, signed: true, sig: "unknown" },
};

function signatureOf(status: SignatureInfo["status"]): SignatureInfo {
  return {
    field: "Signature1", page: 0, rect: { x: 72, y: 600, w: 220, h: 60 }, status,
    signer: "Claire Martin", organization: "Atelier Vauban SARL", signedAt: 1790598720, utcOffset: 120,
    reason: "Bon pour accord", location: "Lyon", intact: status !== "invalid", coversWhole: true, trusted: status === "valid", trustSource: status === "valid" ? "eu:IT" : null,
    certValidAtSigning: true, timestamp: null, timestampVerified: false, timestampTrusted: false, subFilter: "ETSI.CAdES.detached",
    problem: status === "valid" ? null : status === "invalid" ? "modified" : "untrusted",
    certificate: {
      subject: "CN=Claire Martin,O=Atelier Vauban SARL,C=FR", commonName: "Claire Martin", organization: "Atelier Vauban SARL",
      issuer: "CN=Autorité de test,C=FR", issuerName: "Autorité de test", notBefore: 1768176000, notAfter: 1831248000,
      serial: "01:23", sha256: "4F:2A:91:C3:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:7C:0E",
      chain: status === "valid" ? ["Claire Martin", "Autorité de test"] : ["Claire Martin", "Autorité de test"],
      rootName: "Autorité de test", rootSha256: "AB:CD:EF",
    },
  };
}

const PNG_1PX = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
let savedSignatures: SavedSignature[] = [];
let trustedRoots: TrustedRoot[] = [];

// Champs du formulaire simulé (mêmes types que le fixture formulaire-acroform.pdf).
function formFields(): FormField[] {
  const base = { label: null, readOnly: false, required: false, fontSize: 12, align: 0, font: "sans" as const, format: null, range: null, calc: null, customScript: false };
  const w = (x: number, y: number, wd: number, h: number, onState: string | null = null, page = 0) => ({ page, rect: { x, y, w: wd, h }, onState });
  const text = { type: "text" as const, multiline: false, password: false, comb: false, maxLen: null };
  const opts = (l: string[]) => l.map((v) => ({ value: v, label: v }));
  return [
    { ...base, id: "nom", label: "Nom", kind: text, value: [""], defaultValue: [""], widgets: [w(180, 100, 300, 20)] },
    { ...base, id: "prenom", kind: text, value: [""], defaultValue: [""], widgets: [w(180, 136, 300, 20)] },
    { ...base, id: "commentaire", kind: { ...text, multiline: true }, value: [""], defaultValue: [""], widgets: [w(180, 172, 300, 60)] },
    { ...base, id: "accepte", kind: { type: "checkbox" }, value: [], defaultValue: [], widgets: [w(250, 250, 16, 16, "Yes")] },
    { ...base, id: "formule", kind: { type: "radio" }, value: ["mensuelle"], defaultValue: ["mensuelle"], widgets: [w(180, 290, 16, 16, "mensuelle"), w(290, 290, 16, 16, "annuelle"), w(400, 290, 16, 16, "a_vie")] },
    { ...base, id: "pays", kind: { type: "combo", options: opts(["France", "Belgique", "Suisse"]), editable: false }, value: ["France"], defaultValue: ["France"], widgets: [w(180, 330, 200, 20)] },
    { ...base, id: "fixe", readOnly: true, kind: text, value: ["Lecture seule"], defaultValue: ["Lecture seule"], widgets: [w(180, 370, 200, 20)] },
    { ...base, id: "prix", kind: text, value: [""], defaultValue: [""], widgets: [w(180, 410, 200, 20)], format: { type: "number" as const, decimals: 2, sepStyle: 2, negStyle: 0, currency: " €", prepend: false }, range: { min: 0, max: 10000 } },
    { ...base, id: "date", kind: text, value: [""], defaultValue: [""], widgets: [w(180, 450, 200, 20)], format: { type: "date" as const, format: "dd/mm/yyyy" }, customScript: true },
    { ...base, id: "signature", kind: { type: "signature" as const }, value: [], defaultValue: [], widgets: [w(180, 500, 200, 50)] },
    { ...base, id: "ville", kind: text, value: [""], defaultValue: [""], widgets: [w(180, 120, 300, 20, null, 1)] },
  ];
}

let nextId = 1;
const open = new Map<number, string>();

const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v));

// --- Modèle d'annotations simulé (même sémantique que l'écrivain Rust) ---------------------------
interface Ed {
  annots: Annot[];
  saved: Annot[];
  fields: FormField[];
  savedFields: FormField[];
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
    const form = DOCS[open.get(doc) ?? ""]?.form === "acroForm";
    const annots = form ? [] : initial;
    const fields = form ? formFields() : [];
    ed = { annots: clone(annots), saved: clone(annots), fields: clone(fields), savedFields: clone(fields), undo: [], redo: [] };
    editors.set(doc, ed);
  }
  return ed;
}
function applyOps(ed: Ed, ops: AnnotOp[]): AnnotOp[] {
  const list = ed.annots;
  const inv: AnnotOp[] = [];
  for (const op of clone(ops)) {
    if (op.op === "setField") {
      const f = ed.fields.find((x) => x.id === op.id);
      if (!f || f.readOnly || JSON.stringify(f.value) === JSON.stringify(op.value)) continue;
      inv.push({ op: "setField", id: f.id, value: f.value });
      f.value = op.value;
    } else if (op.op === "add") {
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
  return [...new Set(ops.flatMap((o) => (o.op === "setField" ? (ed.fields.find((f) => f.id === o.id)?.widgets.map((w) => w.page) ?? []) : o.op === "remove" ? [ed.annots.find((a) => a.id === o.id)?.page ?? 0] : [o.annot.page])))];
}
function state(doc: number, changedPages: number[]): EditState {
  const ed = editor(doc);
  const key = (a: Annot) => JSON.stringify({ ...a, hidden: false });
  const changedFields = ed.fields.filter((f, i) => JSON.stringify(f.value) !== JSON.stringify(ed.savedFields[i]?.value)).length;
  const unsaved = ed.annots.filter((a) => !ed.saved.some((s) => key(s) === key(a))).length + ed.saved.filter((s) => !ed.annots.some((a) => a.id === s.id)).length + changedFields;
  (e2e.saved ??= {})[open.get(doc) ?? ""] = ed.annots;
  (e2e.fields ??= {})[open.get(doc) ?? ""] = ed.fields;
  return {
    annots: clone(ed.annots),
    fields: clone(ed.fields),
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
    const inverse = applyOps(ed, ops);
    ed.undo.push(inverse);
    ed.redo = [];
    return ok(state(doc, pagesOf(ops, ed)));
  },
  async undo(doc: number): Res<EditState> {
    const ed = editor(doc);
    const ops = ed.undo.pop();
    if (!ops) return ok(state(doc, []));
    ed.redo.push(applyOps(ed, ops));
    return ok(state(doc, pagesOf(ops, ed)));
  },
  async redo(doc: number): Res<EditState> {
    const ed = editor(doc);
    const ops = ed.redo.pop();
    if (!ops) return ok(state(doc, []));
    ed.undo.push(applyOps(ed, ops));
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
  async getSignatures(doc: number): Res<SignatureInfo[]> {
    const d = DOCS[open.get(doc) ?? ""];
    // Une autorité approuvée rend valide la signature intacte « non vérifiable ».
    const status = d?.sig === "unknown" && trustedRoots.length ? "valid" : d?.sig;
    return ok(status ? [signatureOf(status)] : []);
  },
  async trustSignatureRoot(_doc: number, _field: string): Res<TrustedRoot> {
    const r = { id: "AB:CD:EF", name: "Autorité de test", added: Date.now() };
    trustedRoots = [r];
    return ok(r);
  },
  trustListsInfo(): Promise<{ generated: number; eu: number; microsoft: number }> {
    return Promise.resolve({ generated: Date.UTC(2026, 9, 2), eu: 3414, microsoft: 283 });
  },
  async listTrustedRoots(): Promise<TrustedRoot[]> {
    return clone(trustedRoots);
  },
  async removeTrustedRoot(id: string): Res<null> {
    trustedRoots = trustedRoots.filter((r) => r.id !== id);
    return ok(null);
  },
  async importImageBytes(_doc: number, bytes: number[]): Res<ImageInfo> {
    e2e.calls!.push(`importImageBytes:${bytes.length}`);
    return ok({ key: `img:bytes${bytes.length}`, width: 300, height: 100 });
  },
  async readImageFile(path: string): Res<number[]> {
    e2e.calls!.push(`readImageFile:${path}`);
    return ok(Array.from(atob(PNG_1PX), (c) => c.charCodeAt(0)));
  },
  async listSignatures(): Promise<SavedSignature[]> {
    return clone(savedSignatures);
  },
  async saveSignature(png: number[], width: number, height: number): Res<SavedSignature> {
    const s = { id: `sig${savedSignatures.length + 1}`, created: Date.UTC(2026, 8, 14), width, height, data: `data:image/png;base64,${PNG_1PX}` };
    savedSignatures = [s, ...savedSignatures];
    e2e.calls!.push(`saveSignature:${png.length > 0}`);
    return ok(s);
  },
  async deleteSignature(id: string): Res<null> {
    savedSignatures = savedSignatures.filter((s) => s.id !== id);
    return ok(null);
  },
  async copyImage(_from: number, _to: number, _key: string): Res<null> {
    return ok(null);
  },
  async saveDocument(doc: number, path: string | null): Res<DocInfo> {
    e2e.calls!.push(`save:${path ?? open.get(doc)}`);
    const ed = editor(doc);
    ed.annots = ed.annots.filter((a) => a.body.type !== "redact");
    ed.saved = clone(ed.annots);
    ed.savedFields = clone(ed.fields);
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
  trustListsUpdatedEvent: {
    listen: async (_cb: unknown) => () => {},
  },
};
