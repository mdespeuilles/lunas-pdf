<script setup lang="ts">
// Calque d'annotation d'une page (planche 03) : création, sélection, poignées, mini-barre,
// édition en place. Coordonnées en points d'affichage ; `k` = pixels CSS par point.
import { Copy, MessageSquare, Trash2 } from "lucide-vue-next";
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { Annot, AnnotBody, Point, Rect } from "../bindings";
import { commands, unwrap } from "../lib/api";
import { FREETEXT_LEADING, FREETEXT_PAD, fitTextBox, wrap } from "../lib/afm";
import AnnotPreview from "./AnnotPreview.vue";
import {
  BOX_HANDLES, type Handle, canMove, canResize, handlePos, hitRects, moved, rectFrom, resized, snap45,
} from "../lib/annot-geom";
import { PALETTE, type PaletteKey, SIZES, WIDTHS, colorFor, familyOf, paletteKeyOf, useAnnotTools } from "../stores/annotTools";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab; page: number; k: number; pageW: number; pageH: number }>();
const { t } = useI18n();
const tabs = useTabs();
const tools = useAnnotTools();
const layer = ref<HTMLElement>();

const doc = computed(() => props.tab.info!.id);
const annots = computed(() => (props.tab.edit?.annots ?? []).filter((a) => a.page === props.page));
const interactive = computed(() => props.tab.annotating && !!props.tab.edit);
const creating = computed(() => interactive.value && !["select", "signature", "image"].includes(tools.tool));
const hitsActive = computed(() => interactive.value && tools.tool === "select");
const selected = computed(() => annots.value.find((a) => a.id === props.tab.selected) ?? null);

const FONT_CSS = { sans: "Helvetica, Arial, 'Liberation Sans', sans-serif", serif: "'Times New Roman', Times, 'Liberation Serif', serif", mono: "'Courier New', Courier, 'Liberation Mono', monospace" } as const;

function css(r: Rect) {
  return { left: `${r.x * props.k}px`, top: `${r.y * props.k}px`, width: `${r.w * props.k}px`, height: `${r.h * props.k}px` };
}

function toPt(e: PointerEvent | MouseEvent): Point {
  const r = layer.value!.getBoundingClientRect();
  return {
    x: Math.min(Math.max(0, (e.clientX - r.left) / props.k), props.pageW),
    y: Math.min(Math.max(0, (e.clientY - r.top) / props.k), props.pageH),
  };
}

function newAnnot(rect: Rect, body: AnnotBody, extra: Partial<Annot> = {}): Annot {
  const fam = familyOf(tools.tool);
  return {
    id: crypto.randomUUID(),
    page: props.page,
    rect,
    color: colorFor(tools.colors[fam] ?? "red", tools.tool),
    opacity: 1,
    width: ["rect", "ellipse", "line", "arrow"].includes(tools.tool) ? tools.width : tools.tool === "check" ? 2.6 : 1,
    contents: null,
    author: null,
    modified: null,
    excerpt: null,
    body,
    hidden: false,
    ...extra,
  };
}

async function create(a: Annot) {
  await tabs.addAnnot(props.tab, a);
  props.tab.selected = a.id;
}

// --- Création par glisser --------------------------------------------------------------------
interface Draft {
  start: Point;
  cur: Point;
  rects: Rect[];
  moved: boolean;
}
const draft = ref<Draft | null>(null);
const isMarkup = (tool: string) => ["highlight", "underline", "strike"].includes(tool);
let rangeBusy = false;
let rangeAgain = false;

async function refreshRange() {
  if (!draft.value) return;
  if (rangeBusy) {
    rangeAgain = true;
    return;
  }
  rangeBusy = true;
  const { start, cur } = draft.value;
  try {
    const sel = await unwrap(commands.textRange(doc.value, props.page, start, cur));
    if (draft.value) draft.value.rects = sel.rects;
  } catch {
    /* page sans texte */
  }
  rangeBusy = false;
  if (rangeAgain) {
    rangeAgain = false;
    void refreshRange();
  }
}

function onLayerDown(e: PointerEvent) {
  if (!creating.value || e.button !== 0) return;
  e.preventDefault();
  if (editing.value || noteEdit.value) {
    void commitEdit();
    void commitNote();
    return;
  }
  const p = toPt(e);
  props.tab.selected = null;
  switch (tools.tool) {
    case "text": {
      // Le premier caractère commence au point cliqué (marge intérieure de 2 pt compensée),
      // centré verticalement sur la première ligne. Près du bord droit, la zone se rétrécit
      // au lieu de se décaler vers la gauche ; elle ne se décale qu'en deçà de 60 pt.
      const x0 = Math.max(0, p.x - FREETEXT_PAD);
      const { w, h } = fitTextBox("", tools.font, tools.size, props.pageW - x0, t("annot.textPlaceholder"));
      const rect = { x: Math.min(x0, Math.max(0, props.pageW - Math.max(w, 60))), y: Math.min(Math.max(0, p.y - h / 2), props.pageH - h), w, h };
      editing.value = { id: null, rect, auto: true, text: "", font: tools.font, size: tools.size, color: colorFor(tools.colors.text, "text") };
      void nextTick(() => editor.value?.focus());
      return;
    }
    case "note":
      noteEdit.value = { id: null, at: { x: Math.min(p.x, props.pageW - 22), y: Math.min(p.y, props.pageH - 22) }, text: "" };
      void nextTick(() => noteInput.value?.focus());
      return;
    case "check": {
      const s = 16;
      void create(newAnnot({ x: p.x - s / 2, y: p.y - s / 2, w: s, h: s }, { type: "check", style: tools.checkStyle }));
      return;
    }
  }
  draft.value = { start: p, cur: p, rects: [], moved: false };
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
}

function onLayerMove(e: PointerEvent) {
  const d = draft.value;
  if (!d) return;
  let p = toPt(e);
  if (e.shiftKey && (tools.tool === "line" || tools.tool === "arrow")) p = snap45(d.start, p);
  d.cur = p;
  d.moved ||= Math.hypot(p.x - d.start.x, p.y - d.start.y) > 2;
  if (isMarkup(tools.tool)) void refreshRange();
}

async function onLayerUp() {
  const d = draft.value;
  if (!d) return;
  draft.value = null;
  const tool = tools.tool;
  const r = rectFrom(d.start, d.cur);
  if (isMarkup(tool)) {
    let rects = d.rects;
    if (!rects.length) {
      try {
        rects = (await unwrap(commands.textRange(doc.value, props.page, d.start, d.cur))).rects;
      } catch {
        rects = [];
      }
    }
    // Page sans texte (scan) : surlignage de la zone glissée.
    if (!rects.length && tool === "highlight" && r.w > 4 && r.h > 4) rects = [r];
    if (!rects.length) return;
    const type = tool === "highlight" ? "highlight" : tool === "underline" ? "underline" : "strikeOut";
    await create(newAnnot(rects[0], { type, quads: rects } as AnnotBody));
    return;
  }
  if (tool === "line" || tool === "arrow") {
    if (!d.moved) return;
    await create(newAnnot(r, { type: "line", from: d.start, to: d.cur, arrow: tool === "arrow" }));
    return;
  }
  const box = d.moved && r.w > 3 && r.h > 3 ? r : tool === "redact" ? null : { x: d.start.x, y: d.start.y, w: 120, h: 72 };
  if (!box) return;
  if (tool === "rect") await create(newAnnot(box, { type: "square" }));
  else if (tool === "ellipse") await create(newAnnot(box, { type: "circle" }));
  else if (tool === "redact") await create(newAnnot(box, { type: "redact", quads: [box] }, { color: "#e03131" }));
}

const draftShape = computed(() => {
  const d = draft.value;
  if (!d) return null;
  return { r: rectFrom(d.start, d.cur), from: d.start, to: d.cur };
});

// --- Sélection, déplacement, redimensionnement ----------------------------------------------
interface Drag {
  handle: Handle | "move";
  start: Point;
  orig: Annot;
  preview: Annot;
  /** Original masqué : l'aperçu le remplace pendant le glisser. */
  hidden: boolean;
  /** Découpe du rendu de la page (images et annotations externes). */
  crop: string | null;
}
const drag = ref<Drag | null>(null);

function onHitDown(a: Annot, e: PointerEvent) {
  if (!hitsActive.value || e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  void commitEdit();
  props.tab.selected = a.id;
  if (!canMove(a)) return;
  drag.value = { handle: "move", start: toPt(e), orig: a, preview: a, hidden: false, crop: null };
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
}

function onHandleDown(h: Handle, e: PointerEvent) {
  const a = selected.value;
  if (!a) return;
  e.preventDefault();
  e.stopPropagation();
  drag.value = { handle: h, start: toPt(e), orig: a, preview: a, hidden: false, crop: null };
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
}

/** Image de l'annotation telle que rendue (pour celles dont l'aperçu ne se dessine pas). */
function cropOf(a: Annot): string | null {
  if (a.body.type !== "image" && a.body.type !== "other") return null;
  const c = layer.value?.parentElement?.querySelector<HTMLCanvasElement>("canvas.base");
  if (!c || !c.width) return null;
  const s = c.width / props.pageW;
  const out = document.createElement("canvas");
  out.width = Math.max(1, Math.round(a.rect.w * s));
  out.height = Math.max(1, Math.round(a.rect.h * s));
  out.getContext("2d")!.drawImage(c, a.rect.x * s, a.rect.y * s, out.width, out.height, 0, 0, out.width, out.height);
  return out.toDataURL();
}

function onDragMove(e: PointerEvent) {
  const d = drag.value;
  if (!d) return;
  const p = toPt(e);
  if (!d.hidden && Math.hypot(p.x - d.start.x, p.y - d.start.y) > 1) {
    // Premier déplacement réel : l'aperçu remplace l'original.
    d.hidden = true;
    d.crop = cropOf(d.orig);
    void tabs.setHidden(props.tab, d.orig.id, true);
  }
  if (d.handle === "move") {
    const r = d.orig.rect;
    const dx = Math.min(Math.max(p.x - d.start.x, -r.x), props.pageW - r.x - r.w);
    const dy = Math.min(Math.max(p.y - d.start.y, -r.y), props.pageH - r.y - r.h);
    d.preview = moved(d.orig, dx, dy);
  } else {
    d.preview = resized(d.orig, d.handle, d.handle === "to" && e.shiftKey && d.orig.body.type === "line" ? snap45(d.orig.body.from, p) : p, e.shiftKey || undefined);
  }
}

async function onDragUp() {
  const d = drag.value;
  if (!d) return;
  drag.value = null;
  const changed = JSON.stringify(d.preview) !== JSON.stringify(d.orig);
  if (changed) await tabs.updateAnnot(props.tab, { ...d.preview, hidden: false });
  else if (d.hidden) await tabs.setHidden(props.tab, d.orig.id, false);
}

const shown = computed(() => (drag.value ? drag.value.preview : selected.value));
const handles = computed<Handle[]>(() => {
  const a = shown.value;
  if (!a || !canResize(a)) return [];
  return a.body.type === "line" ? ["from", "to"] : BOX_HANDLES;
});
function handleAt(a: Annot, h: Handle): Point {
  if (a.body.type === "line") return h === "from" ? a.body.from : a.body.to;
  return handlePos(a.rect, h);
}

function onDblClick(a: Annot) {
  if (a.body.type === "freeText") startEdit(a);
  else if (a.body.type === "note") openNote(a);
}

function onHitClick(a: Annot) {
  if (a.body.type === "note" && props.tab.selected === a.id && !drag.value) openNote(a);
}

// --- Zone de texte : édition en place ---------------------------------------------------------
interface Editing {
  id: string | null;
  rect: Rect;
  /** Largeur ajustée au texte (sinon largeur fixée par l'utilisateur, le texte revient à la ligne). */
  auto: boolean;
  text: string;
  font: "sans" | "serif" | "mono";
  size: number;
  color: string;
}
const editing = ref<Editing | null>(null);
const editor = ref<HTMLTextAreaElement>();

function startEdit(a: Annot) {
  if (a.body.type !== "freeText") return;
  // Largeur « automatique » si la zone a exactement la largeur de son texte.
  const natural = fitTextBox(a.body.text, a.body.font, a.body.size, props.pageW - a.rect.x).w;
  editing.value = { id: a.id, rect: { ...a.rect }, auto: Math.abs(natural - a.rect.w) < 1.5, text: a.body.text, font: a.body.font, size: a.body.size, color: a.color };
  void tabs.setHidden(props.tab, a.id, true);
  void nextTick(() => {
    editor.value?.focus();
    editor.value?.select();
  });
}

/** Ajuste la zone à son texte avec les métriques du PDF (pas celles du navigateur). */
function refit() {
  const ed = editing.value;
  if (!ed) return;
  if (ed.auto) {
    const { w, h } = fitTextBox(ed.text, ed.font, ed.size, props.pageW - ed.rect.x, t("annot.textPlaceholder"));
    ed.rect = { ...ed.rect, w, h };
  } else {
    const lines = wrap(ed.text, ed.font, ed.size, ed.rect.w - 2 * FREETEXT_PAD).length;
    ed.rect = { ...ed.rect, h: lines * ed.size * FREETEXT_LEADING + 2 * FREETEXT_PAD };
  }
}

async function commitEdit(cancel = false) {
  const ed = editing.value;
  if (!ed) return;
  editing.value = null;
  const text = ed.text.replace(/\s+$/, "");
  // Dimensions finales : sans le texte indicatif ni les espaces de fin.
  if (ed.auto) {
    const { w, h } = fitTextBox(text, ed.font, ed.size, props.pageW - ed.rect.x);
    ed.rect = { ...ed.rect, w, h };
  }
  if (ed.id === null) {
    if (!cancel && text) {
      tools.tool = "select";
      await create(newAnnot(ed.rect, { type: "freeText", text, font: ed.font, size: ed.size }, { color: ed.color }));
    }
    return;
  }
  const a = (props.tab.edit?.annots ?? []).find((x) => x.id === ed.id);
  if (!a || a.body.type !== "freeText") return;
  if (cancel || (text === a.body.text && ed.rect.h === a.rect.h && ed.rect.w === a.rect.w)) {
    await tabs.setHidden(props.tab, a.id, false);
  } else if (!text) {
    await tabs.removeAnnot(props.tab, a.id);
  } else {
    await tabs.updateAnnot(props.tab, { ...a, rect: ed.rect, body: { ...a.body, text }, hidden: false });
  }
}

function onEditorKey(e: KeyboardEvent) {
  e.stopPropagation();
  if (e.key === "Escape") void commitEdit(true);
  else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void commitEdit();
}

// --- Notes -------------------------------------------------------------------------------------
const noteEdit = ref<{ id: string | null; at: Point; text: string } | null>(null);
const noteInput = ref<HTMLTextAreaElement>();

function openNote(a: Annot) {
  noteEdit.value = { id: a.id, at: { x: a.rect.x, y: a.rect.y }, text: a.contents ?? "" };
  void nextTick(() => noteInput.value?.focus());
}

async function commitNote() {
  const n = noteEdit.value;
  if (!n) return;
  noteEdit.value = null;
  const text = n.text.trim();
  if (n.id === null) {
    if (text) await create(newAnnot({ x: n.at.x, y: n.at.y, w: 22, h: 22 }, { type: "note" }, { contents: text, color: colorFor(tools.colors.note, "note") }));
    return;
  }
  const a = (props.tab.edit?.annots ?? []).find((x) => x.id === n.id);
  if (a && (a.contents ?? "") !== text) await tabs.updateAnnot(props.tab, { ...a, contents: text || null });
}

function onNoteKey(e: KeyboardEvent) {
  e.stopPropagation();
  if (e.key === "Escape") noteEdit.value = null;
  else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void commitNote();
}

// --- Mini-barre --------------------------------------------------------------------------------
/** Largeur réelle de la mini-barre (mesurée) pour la garder dans la page. */
const miniEl = ref<HTMLElement>();
const miniW = ref(0);
let miniObserver: ResizeObserver | undefined;
watch(miniEl, (el) => {
  miniObserver?.disconnect();
  if (!el) return;
  miniW.value = el.offsetWidth;
  miniObserver = new ResizeObserver(() => (miniW.value = el.offsetWidth));
  miniObserver.observe(el);
});
onBeforeUnmount(() => miniObserver?.disconnect());

const mini = computed(() => {
  const a = selected.value;
  if (!a || drag.value || editing.value || !interactive.value) return null;
  const pageW = props.pageW * props.k;
  const top = a.rect.y * props.k - 48;
  // Alignée sur l'annotation, mais ramenée dans la page près des bords.
  const left = Math.min(a.rect.x * props.k - 6, pageW - miniW.value - 4);
  return { left: Math.max(4, left), top: top < 4 ? (a.rect.y + a.rect.h) * props.k + 10 : top };
});
const selFamily = computed(() => (selected.value ? familyOf(selected.value.body.type) : ""));
const selKey = computed(() => (selected.value ? paletteKeyOf(selected.value.color) : null));
const recolorable = computed(() => selected.value && !["image", "other", "redact"].includes(selected.value.body.type));

function recolor(key: PaletteKey) {
  const a = selected.value;
  if (!a) return;
  const tool = a.body.type === "highlight" ? "highlight" : "rect";
  void tabs.updateAnnot(props.tab, { ...a, color: colorFor(key, tool) });
  tools.colors = { ...tools.colors, [selFamily.value]: key };
}

function setSize(size: number) {
  const a = selected.value;
  if (a?.body.type !== "freeText") return;
  const { text, font } = a.body;
  const maxW = props.pageW - a.rect.x;
  const auto = Math.abs(fitTextBox(text, font, a.body.size, maxW).w - a.rect.w) < 1.5;
  const lines = wrap(text, font, size, a.rect.w - 2 * FREETEXT_PAD).length;
  const dims = auto ? fitTextBox(text, font, size, maxW) : { w: a.rect.w, h: lines * size * FREETEXT_LEADING + 2 * FREETEXT_PAD };
  void tabs.updateAnnot(props.tab, { ...a, rect: { ...a.rect, ...dims }, body: { ...a.body, size } });
}

function setWidth(width: number) {
  const a = selected.value;
  if (a) void tabs.updateAnnot(props.tab, { ...a, width });
}

function duplicate() {
  const a = selected.value;
  if (!a) return;
  const copy = { ...moved(a, 12, 12), id: crypto.randomUUID(), author: null, modified: null };
  void create(copy);
}

function remove() {
  const a = selected.value;
  if (a) void tabs.removeAnnot(props.tab, a.id);
}

// Fin d'édition si l'onglet quitte le mode annotation.
watch(interactive, (on) => {
  if (!on) {
    void commitEdit();
    void commitNote();
  }
});
onBeforeUnmount(() => {
  void commitEdit();
  void commitNote();
});

defineExpose({ duplicate, remove, startEdit });

</script>

<template>
  <div
    ref="layer"
    class="alayer"
    :class="{ creating, ['tool-' + tools.tool]: creating, grabbing: drag?.handle === 'move' }"
    :aria-label="t('annot.layer', { n: page + 1 })"
    @pointerdown="onLayerDown"
    @pointermove="onLayerMove"
    @pointerup="onLayerUp"
  >
    <!-- Notes : contenu au survol, même hors mode annotation. -->
    <template v-if="!interactive">
      <span v-for="a in annots.filter((x) => x.body.type === 'note' && x.contents)" :key="a.id" class="note-tip" :style="css(a.rect)" :title="a.contents ?? ''" />
    </template>

    <!-- Zones cliquables -->
    <template v-if="hitsActive">
      <template v-for="a in annots" :key="a.id">
        <span
          v-for="(r, i) in hitRects(a)"
          :key="i"
          class="hit"
          :class="{ sel: a.id === tab.selected, movable: canMove(a) }"
          :style="css(r)"
          :aria-label="t(`annot.kinds.${a.body.type === 'line' && a.body.arrow ? 'arrow' : a.body.type}`)"
          @pointerdown="onHitDown(a, $event)"
          @pointermove="onDragMove"
          @pointerup="onDragUp"
          @click="onHitClick(a)"
          @dblclick="onDblClick(a)"
        />
      </template>
    </template>

    <!-- Aperçu pendant le glisser (l'original est masqué) -->
    <AnnotPreview v-if="drag?.hidden" :annot="drag.preview" :k="k" :crop="drag.crop" />

    <!-- Sélection : cadre et poignées -->
    <template v-if="interactive && shown">
      <svg v-if="shown.body.type === 'line'" class="sel-svg">
        <line :x1="shown.body.from.x * k" :y1="shown.body.from.y * k" :x2="shown.body.to.x * k" :y2="shown.body.to.y * k" />
      </svg>
      <template v-else-if="'quads' in shown.body && shown.body.quads.length > 1">
        <span v-for="(q, i) in shown.body.quads" :key="i" class="sel-box" :style="css(q)" />
      </template>
      <span v-else class="sel-box" :class="{ dragging: !!drag }" :style="css(shown.rect)" />
      <span
        v-for="h in handles"
        :key="h"
        class="hd"
        :class="'hd-' + h"
        :style="{ left: handleAt(shown, h).x * k - 4 + 'px', top: handleAt(shown, h).y * k - 4 + 'px' }"
        @pointerdown="onHandleDown(h, $event)"
        @pointermove="onDragMove"
        @pointerup="onDragUp"
      />
    </template>

    <!-- Aperçu de création -->
    <svg v-if="draft && draftShape" class="draft">
      <template v-if="isMarkup(tools.tool)">
        <rect v-for="(q, i) in draft.rects" :key="i" :x="q.x * k" :y="q.y * k" :width="q.w * k" :height="q.h * k" :fill="tools.color" fill-opacity="0.45" />
        <rect v-if="!draft.rects.length" :x="draftShape.r.x * k" :y="draftShape.r.y * k" :width="draftShape.r.w * k" :height="draftShape.r.h * k" class="ghost" />
      </template>
      <line v-else-if="tools.tool === 'line' || tools.tool === 'arrow'" :x1="draftShape.from.x * k" :y1="draftShape.from.y * k" :x2="draftShape.to.x * k" :y2="draftShape.to.y * k" :stroke="tools.color" :stroke-width="tools.width * k" stroke-linecap="round" />
      <ellipse v-else-if="tools.tool === 'ellipse'" :cx="(draftShape.r.x + draftShape.r.w / 2) * k" :cy="(draftShape.r.y + draftShape.r.h / 2) * k" :rx="(draftShape.r.w / 2) * k" :ry="(draftShape.r.h / 2) * k" fill="none" :stroke="tools.color" :stroke-width="tools.width * k" />
      <rect v-else :x="draftShape.r.x * k" :y="draftShape.r.y * k" :width="draftShape.r.w * k" :height="draftShape.r.h * k" :class="{ redact: tools.tool === 'redact' }" fill="none" :stroke="tools.color" :stroke-width="tools.width * k" />
    </svg>

    <!-- Zone de texte en cours d'édition -->
    <div v-if="editing" class="ft-edit" :style="css(editing.rect)">
      <textarea
        ref="editor"
        v-model="editing.text"
        rows="1"
        :aria-label="t('annot.editText')"
        :placeholder="t('annot.textPlaceholder')"
        spellcheck="true"
        :style="{ fontFamily: FONT_CSS[editing.font], fontSize: editing.size * k + 'px', color: editing.color, padding: 2 * k + 'px', lineHeight: 1.2 }"
        @input="refit"
        @blur="commitEdit()"
        @keydown="onEditorKey"
        @pointerdown.stop
      />
      <span v-for="h in BOX_HANDLES" :key="h" class="hd static" :class="'hd-' + h" :style="{ left: handlePos({ ...editing.rect, x: 0, y: 0 }, h).x * k - 4 + 'px', top: handlePos({ ...editing.rect, x: 0, y: 0 }, h).y * k - 4 + 'px' }" />
    </div>

    <!-- Note en cours d'édition -->
    <div v-if="noteEdit" class="note-pop" :style="{ left: Math.min(noteEdit.at.x * k + 28, pageW * k - 250) + 'px', top: noteEdit.at.y * k + 'px' }" @pointerdown.stop>
      <div class="np-h"><MessageSquare class="ic xs" aria-hidden="true" />{{ t("annot.noteTitle") }}</div>
      <textarea ref="noteInput" v-model="noteEdit.text" :aria-label="t('annot.noteTitle')" :placeholder="t('annot.notePlaceholder')" rows="4" @keydown="onNoteKey" @blur="commitNote()" />
    </div>

    <!-- Mini-barre flottante -->
    <div v-if="mini && selected" ref="miniEl" class="mini" :style="{ left: mini.left + 'px', top: mini.top + 'px' }" role="toolbar" :aria-label="t(`annot.kinds.${selected.body.type}`)" @pointerdown.stop>
      <template v-if="recolorable">
        <button
          v-for="p in PALETTE"
          :key="p.key"
          class="sw"
          :class="{ on: selKey === p.key }"
          :style="{ background: selected.body.type === 'highlight' ? p.highlight : p.color }"
          :aria-label="t(`annot.colors.${p.key}`)"
          :title="t(`annot.colors.${p.key}`)"
          @click="recolor(p.key)"
        />
        <div class="sep" />
      </template>
      <template v-if="selected.body.type === 'freeText'">
        <select class="msel" :value="selected.body.size" :aria-label="t('annot.textSize')" @change="setSize(+($event.target as HTMLSelectElement).value)">
          <option v-for="s in SIZES" :key="s" :value="s">{{ t("annot.sizePt", { n: s }) }}</option>
        </select>
        <div class="sep" />
      </template>
      <template v-else-if="['square', 'circle', 'line'].includes(selected.body.type)">
        <select class="msel" :value="selected.width" :aria-label="t('annot.width')" @change="setWidth(+($event.target as HTMLSelectElement).value)">
          <option v-for="w in WIDTHS" :key="w" :value="w">{{ t("annot.sizePt", { n: w }) }}</option>
        </select>
        <div class="sep" />
      </template>
      <button class="tb" :aria-label="t('annot.duplicate')" :title="t('annot.duplicate')" @click="duplicate"><Copy class="ic s" aria-hidden="true" /></button>
      <button class="tb" :aria-label="t('annot.delete')" :title="t('annot.delete')" @click="remove"><Trash2 class="ic s" aria-hidden="true" /></button>
    </div>
  </div>
</template>

<style scoped>
.alayer { position: absolute; inset: 0; z-index: 4; pointer-events: none; }
.alayer.creating { pointer-events: auto; cursor: crosshair; }
/* Outil zone de texte : « T » encadré, distinct du curseur en I de la sélection. */
.alayer.tool-text { cursor: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24'%3E%3Crect x='2.5' y='2.5' width='19' height='19' rx='3' fill='white' stroke='%231b1b20' stroke-width='1.5'/%3E%3Cpath d='M7.5 7.5h9M12 7.5v9' stroke='%231b1b20' stroke-width='1.8' stroke-linecap='round'/%3E%3C/svg%3E") 12 12, crosshair; }
.alayer.tool-note, .alayer.tool-check { cursor: copy; }
.note-tip { position: absolute; pointer-events: auto; cursor: help; }
.hit { position: absolute; pointer-events: auto; border-radius: 2px; cursor: pointer; }
/* Main ouverte au survol, fermée pendant le déplacement (`move` manque dans certains thèmes GTK). */
.hit.movable { cursor: grab; }
.alayer.grabbing, .alayer.grabbing * { cursor: grabbing !important; }
.hit:hover { box-shadow: 0 0 0 1.5px color-mix(in oklab, var(--accent) 60%, transparent); }
.hit.sel:hover { box-shadow: none; }
.sel-box { position: absolute; border: 1px dashed var(--accent); pointer-events: none; margin: -3px; padding: 3px; box-sizing: content-box; }
.sel-box.dragging { border-style: solid; background: color-mix(in oklab, var(--accent) 8%, transparent); }
.sel-svg, .draft { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; pointer-events: none; }
.sel-svg line { stroke: var(--accent); stroke-width: 1; stroke-dasharray: 4 3; }
.draft .ghost { fill: color-mix(in oklab, var(--accent) 10%, transparent); stroke: var(--accent); stroke-dasharray: 4 3; }
.draft .redact { fill: rgba(224, 49, 49, .12); stroke: #e03131; }
.hd { position: absolute; width: 8px; height: 8px; background: #fff; border: 1.5px solid var(--accent); border-radius: 2px; pointer-events: auto; z-index: 2; }
.hd.static { pointer-events: none; }
.hd-nw, .hd-se { cursor: nwse-resize; }
.hd-ne, .hd-sw { cursor: nesw-resize; }
.hd-n, .hd-s { cursor: ns-resize; }
.hd-e, .hd-w { cursor: ew-resize; }
.hd-from, .hd-to { cursor: crosshair; border-radius: 50%; }
.ft-edit { position: absolute; border: 1px dashed var(--accent); pointer-events: auto; }
.ft-edit textarea { width: 100%; height: 100%; border: 0; resize: none; background: transparent; outline: none; overflow: hidden; box-sizing: border-box; display: block; white-space: pre-wrap; overflow-wrap: anywhere; margin: 0; }
.ft-edit textarea::placeholder { color: color-mix(in oklab, currentColor 45%, transparent); }
.ft-edit textarea:focus-visible { box-shadow: none; }
.note-pop { position: absolute; z-index: 9; width: 240px; padding: 8px; border-radius: 10px; background: #fff9db; color: #5c4400; box-shadow: 0 0 0 1px rgba(92, 68, 0, .18), var(--shadow); pointer-events: auto; font-family: system-ui, sans-serif; }
.np-h { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; margin-bottom: 6px; }
.note-pop textarea { width: 100%; border: 0; resize: vertical; background: rgba(255, 255, 255, .6); border-radius: 6px; padding: 6px; font: 12.5px/1.45 system-ui, sans-serif; color: #3b2c00; box-sizing: border-box; }
.mini { position: absolute; z-index: 8; display: flex; align-items: center; gap: 4px; padding: 4px 6px; border-radius: 10px; background: var(--surface); box-shadow: 0 0 0 1px var(--line), var(--shadow); font-family: system-ui, sans-serif; font-size: 12px; color: var(--text); white-space: nowrap; pointer-events: auto; }
.mini .tb { width: 28px; height: 28px; }
.mini .sep { height: 18px; margin: 0 3px; }
.sw { width: 16px; height: 16px; border-radius: 50%; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .18); flex: none; border: 0; padding: 0; }
.sw.on { box-shadow: 0 0 0 2px var(--surface), 0 0 0 3.5px var(--text-2); }
.msel { height: 26px; border: 0; border-radius: 6px; background: var(--surface-2); color: var(--text); font: inherit; font-size: 12px; padding: 0 4px; }
</style>
