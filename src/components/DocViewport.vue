<script setup lang="ts">
// Zone de lecture : virtualisation (pages visibles ± une hauteur d'écran), zoom ancré,
// modes page unique / continu / double page, indicateur de page.
import { ChevronDown, ChevronUp } from "lucide-vue-next";
import { computed, nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import PageView, { type PageHighlight } from "./PageView.vue";
import type { LinkInfo } from "../bindings";
import { PT_TO_PX, clampZoom, computeLayout, currentPage, fitZoom, scrollTarget, visiblePages } from "../lib/layout";
import { openUrl } from "../lib/window";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const tabs = useTabs();
const ui = useUi();
const info = computed(() => props.tab.info!);

const scroller = ref<HTMLElement>();
const vw = ref(800);
const vh = ref(600);
const scrollTop = ref(0);
const scrollLeft = ref(0);
const dpr = ref(window.devicePixelRatio || 1);
/** Vrai pendant un défilement programmé : la page courante n'est pas recalculée. */
let programmatic = false;

const layout = computed(() =>
  computeLayout(info.value.pages, props.tab.mode, props.tab.zoom, vw.value, props.tab.mode === "single" ? props.tab.page : 0),
);
const boxByIndex = computed(() => new Map(layout.value.boxes.map((b) => [b.index, b])));
const shown = computed(() => visiblePages(layout.value, scrollTop.value, vh.value, vh.value));
const onScreen = computed(() => new Set(visiblePages(layout.value, scrollTop.value, vh.value, 0)));

const hitsByPage = computed(() => {
  const m = new Map<number, PageHighlight[]>();
  props.tab.search.hits.forEach((h, i) => {
    const list = m.get(h.page) ?? [];
    list.push({ rects: h.rects, current: i === props.tab.search.current });
    m.set(h.page, list);
  });
  return m;
});

function visibleRect(index: number) {
  const b = boxByIndex.value.get(index);
  if (!b) return null;
  return {
    x0: Math.max(0, scrollLeft.value - b.x),
    y0: Math.max(0, scrollTop.value - b.y),
    x1: Math.min(b.w, scrollLeft.value + vw.value - b.x),
    y1: Math.min(b.h, scrollTop.value + vh.value - b.y),
  };
}

// --- Ancre de zoom : le point sous le curseur (ou le haut de la vue) reste en place ----------
interface Anchor {
  page: number;
  xPt: number;
  yPt: number;
  sx: number;
  sy: number;
}
let anchor: Anchor | null = null;

function anchorAt(sx: number, sy: number): Anchor | null {
  const k = props.tab.zoom * PT_TO_PX;
  const x = scrollLeft.value + sx;
  const y = scrollTop.value + sy;
  const b = layout.value.boxes.find((bb) => y >= bb.y - 8 && y <= bb.y + bb.h + 16) ?? layout.value.boxes.find((bb) => bb.index === props.tab.page);
  if (!b) return null;
  return { page: b.index, xPt: (x - b.x) / k, yPt: (y - b.y) / k, sx, sy };
}

watch(
  () => props.tab.zoom,
  async () => {
    const a = anchor ?? anchorAt(vw.value / 2, 0);
    anchor = null;
    tabs.bumpEpoch(props.tab);
    await nextTick();
    const el = scroller.value;
    const b = a && boxByIndex.value.get(a.page);
    if (!el || !a || !b) return;
    const k = props.tab.zoom * PT_TO_PX;
    programmatic = true;
    el.scrollTo({ left: b.x + a.xPt * k - a.sx, top: b.y + a.yPt * k - a.sy });
    syncScroll();
  },
  { flush: "pre" },
);

// Mise à l'échelle automatique en mode « ajuster ».
function applyFit() {
  if (props.tab.fit) props.tab.zoom = fitZoom(info.value.pages, props.tab.mode, props.tab.fit, vw.value, vh.value);
}
watch(() => [props.tab.fit, props.tab.mode, vw.value, vh.value], applyFit);

// --- Navigation demandée (barre d'outils, miniatures, sommaire, recherche) --------------------
watch(
  () => props.tab.nav,
  async (nav) => {
    if (!nav) return;
    await nextTick();
    const el = scroller.value;
    if (!el) return;
    const y = scrollTarget(layout.value, nav.page, props.tab.zoom, nav.yPt);
    if (y === null) return;
    const b = boxByIndex.value.get(nav.page)!;
    // Ne bouge pas si la cible est déjà bien visible (ex. résultat suivant sur la même page).
    const targetY = nav.yPt !== undefined ? b.y + nav.yPt * props.tab.zoom * PT_TO_PX : b.y;
    const alreadyVisible = nav.yPt !== undefined && targetY > el.scrollTop + 40 && targetY < el.scrollTop + el.clientHeight - 80;
    if (!alreadyVisible) {
      if (Math.abs(el.scrollTop - y) > vh.value * 2) tabs.bumpEpoch(props.tab);
      programmatic = true;
      el.scrollTo({ top: y, left: el.scrollLeft });
    }
    syncScroll();
  },
);

// --- Défilement --------------------------------------------------------------------------------
let lastEpochTop = 0;
function syncScroll() {
  const el = scroller.value;
  if (!el) return;
  scrollTop.value = el.scrollTop;
  scrollLeft.value = el.scrollLeft;
  // Saut important (barre de défilement, Fin…) : abandonner les rendus en attente.
  if (Math.abs(el.scrollTop - lastEpochTop) > vh.value * 3) {
    lastEpochTop = el.scrollTop;
    tabs.bumpEpoch(props.tab);
  }
}

function onScroll() {
  syncScroll();
  if (programmatic) {
    programmatic = false;
    return;
  }
  if (props.tab.mode !== "single") props.tab.page = currentPage(layout.value, scrollTop.value, vh.value);
  saveScroll();
}

function saveScroll() {
  const el = scroller.value;
  if (el) (props.tab as DocTab & { scroll?: { top: number; left: number } }).scroll = { top: el.scrollTop, left: el.scrollLeft };
}

// Zoom Ctrl + molette (ou pincement du pavé tactile), ancré sous le curseur.
// En page unique, la molette en bout de page passe à la page voisine.
function onWheel(e: WheelEvent) {
  const el = scroller.value!;
  if (e.ctrlKey || e.metaKey) {
    e.preventDefault();
    const r = el.getBoundingClientRect();
    anchor = anchorAt(e.clientX - r.left, e.clientY - r.top);
    const factor = Math.exp(-e.deltaY * (e.deltaMode === 1 ? 0.05 : 0.0025));
    props.tab.fit = null;
    props.tab.zoom = clampZoom(props.tab.zoom * factor);
    return;
  }
  if (props.tab.mode === "single") {
    const atBottom = el.scrollTop + el.clientHeight >= el.scrollHeight - 2;
    const atTop = el.scrollTop <= 0;
    if (e.deltaY > 0 && atBottom && props.tab.page < info.value.pages.length - 1) {
      e.preventDefault();
      tabs.step(props.tab, 1);
    } else if (e.deltaY < 0 && atTop && props.tab.page > 0) {
      e.preventDefault();
      tabs.goto(props.tab, props.tab.page - 1);
      void nextTick(() => {
        programmatic = true;
        el.scrollTo({ top: el.scrollHeight });
      });
    }
  }
}

/** Clic hors d'une annotation : désélection. */
function onBackgroundDown(e: PointerEvent) {
  const el = e.target as HTMLElement;
  if (props.tab.selected && !el.closest(".hit, .hd, .mini, .ft-edit, .note-pop")) props.tab.selected = null;
}

function onLink(l: LinkInfo) {
  if (l.target.type === "page") tabs.goto(props.tab, l.target.page);
  else void openUrl(l.target.uri);
}

function onCopy(e: ClipboardEvent) {
  if (!info.value.canCopy) {
    e.preventDefault();
    ui.notify(t("errors.copyForbidden"));
  }
}

// --- Taille de la zone et densité de pixels ------------------------------------------------------
let ro: ResizeObserver | undefined;
let mql: MediaQueryList | undefined;
function watchDpr() {
  mql?.removeEventListener("change", watchDpr);
  dpr.value = window.devicePixelRatio || 1;
  mql = window.matchMedia(`(resolution: ${dpr.value}dppx)`);
  mql.addEventListener("change", watchDpr);
}

onMounted(() => {
  const el = scroller.value!;
  ro = new ResizeObserver(() => {
    if (!el.clientWidth) return; // onglet masqué
    vw.value = el.clientWidth;
    vh.value = el.clientHeight;
  });
  ro.observe(el);
  vw.value = el.clientWidth || 800;
  vh.value = el.clientHeight || 600;
  watchDpr();
  applyFit();
  void nextTick(() => tabs.goto(props.tab, props.tab.page));
});
onActivated(() => {
  const s = (props.tab as DocTab & { scroll?: { top: number; left: number } }).scroll;
  if (s && scroller.value) {
    programmatic = true;
    scroller.value.scrollTo(s);
  }
});
onBeforeUnmount(() => {
  ro?.disconnect();
  mql?.removeEventListener("change", watchDpr);
});

const pageCount = computed(() => info.value.pages.length);
</script>

<template>
  <div class="canvas">
    <div
      ref="scroller"
      class="scroller"
      tabindex="0"
      :aria-label="tab.name"
      @scroll.passive="onScroll"
      @wheel="onWheel"
      @pointerdown.capture="onBackgroundDown"
      @copy="onCopy"
    >
      <div class="content" :style="{ width: layout.width + 'px', height: layout.height + 'px' }">
        <PageView
          v-for="i in shown"
          :key="i"
          :doc="info.id"
          :box="boxByIndex.get(i)!"
          :geom="info.pages[i]"
          :zoom="tab.zoom"
          :dpr="dpr"
          :priority="onScreen.has(i) ? 200 : 100"
          :epoch="tab.epoch"
          :visible="onScreen.has(i) ? visibleRect(i) : null"
          :highlights="hitsByPage.get(i) ?? []"
          :flash="tab.flash && tab.flash.page === i ? tab.flash.rect : null"
          :rev="tab.pageRev[i] ?? 0"
          :tab="tab"
          @link="onLink"
        />
      </div>
    </div>
    <div class="pind" role="status">
      <button :aria-label="t('toolbar.prevPage')" :disabled="tab.page <= 0" @click="tabs.step(tab, -1)">
        <ChevronUp class="ic xs" aria-hidden="true" />
      </button>
      <span class="t">{{ t("pageIndicator.page", { n: info.pages[tab.page]?.label ?? tab.page + 1 }) }} <span class="dim">{{ t("pageIndicator.of", { n: pageCount }) }}</span></span>
      <button :aria-label="t('toolbar.nextPage')" :disabled="tab.page >= pageCount - 1" @click="tabs.step(tab, 1)">
        <ChevronDown class="ic xs" aria-hidden="true" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.canvas { flex: 1; position: relative; overflow: hidden; background: var(--canvas); min-width: 0; display: flex; }
.scroller { flex: 1; overflow: auto; position: relative; }
.scroller:focus-visible { box-shadow: inset var(--ring); }
.content { position: relative; margin: 0 auto; }
.pind { position: absolute; left: 50%; bottom: 18px; transform: translateX(-50%); display: flex; align-items: center; gap: 6px; height: 30px; padding: 0 6px; border-radius: 15px; background: rgba(28, 28, 33, .80); color: #f2f2f5; font-size: 12px; font-variant-numeric: tabular-nums; backdrop-filter: blur(10px); box-shadow: 0 4px 16px rgba(0, 0, 0, .2); z-index: 6; white-space: nowrap; }
.pind .t { padding: 0 6px; }
.pind .dim { color: #b8b8c2; }
.pind button { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 11px; border: 0; background: transparent; color: #e6e6ea; padding: 0; }
.pind button:hover:not(:disabled) { background: rgba(255, 255, 255, .14); }
.pind button:disabled { opacity: .4; }
</style>
