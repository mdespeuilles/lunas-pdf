<script setup lang="ts">
// Une page : bitmap (basse résolution d'abord), tuiles au fort zoom, texte, liens, surlignages.
import { computed, onMounted, ref, watch } from "vue";
import AnnotLayer from "./AnnotLayer.vue";
import TextLayer from "./TextLayer.vue";
import TileCanvas from "./TileCanvas.vue";
import type { LinkInfo, PageGeom, Rect } from "../bindings";
import { pageLinks } from "../lib/page-data";
import { bitmaps } from "../lib/bitmap-cache";
import { MAX_FULL_BITMAP_PX, PT_TO_PX, type PageBox, tilesFor } from "../lib/layout";
import type { DocTab } from "../stores/tabs";

export interface PageHighlight {
  rects: Rect[];
  current: boolean;
}

const props = defineProps<{
  doc: number;
  box: PageBox;
  geom: PageGeom;
  zoom: number;
  dpr: number;
  priority: number;
  epoch: number;
  /** Zone visible de la page en px CSS (pour les tuiles). */
  visible: { x0: number; y0: number; x1: number; y1: number } | null;
  highlights: PageHighlight[];
  flash: Rect | null;
  /** Révision du rendu de la page (après modification des annotations). */
  rev: number;
  tab: DocTab;
}>();
const emit = defineEmits<{ link: [link: LinkInfo] }>();

const canvas = ref<HTMLCanvasElement>();
const k = computed(() => props.zoom * PT_TO_PX);
const pxW = computed(() => Math.round(props.box.w * props.dpr));
const pxH = computed(() => Math.round(props.box.h * props.dpr));
const tiled = computed(() => pxW.value * pxH.value > MAX_FULL_BITMAP_PX);
/** Taille de la bitmap de base (réduite si la page est tuilée). */
const base = computed(() => {
  if (!tiled.value) return { w: pxW.value, h: pxH.value };
  const s = Math.sqrt(4_000_000 / (pxW.value * pxH.value));
  return { w: Math.round(pxW.value * s), h: Math.round(pxH.value * s) };
});
const tiles = computed(() =>
  tiled.value && props.visible
    ? tilesFor(pxW.value, pxH.value, {
        x0: props.visible.x0 * props.dpr,
        y0: props.visible.y0 * props.dpr,
        x1: props.visible.x1 * props.dpr,
        y1: props.visible.y1 * props.dpr,
      })
    : [],
);

let drawn = 0;
function draw(bmp: ImageBitmap) {
  const c = canvas.value;
  if (!c) return;
  c.width = bmp.width;
  c.height = bmp.height;
  c.getContext("2d")!.drawImage(bmp, 0, 0);
  drawn = bmp.width;
}

async function load() {
  const { w, h } = base.value;
  // Basse résolution immédiate si une bitmap de la page est déjà en cache (miniature, ancien zoom).
  if (drawn !== w) {
    const prev = bitmaps.bestFor(props.doc, props.box.index, w);
    if (prev && prev.width !== drawn) draw(prev);
  }
  const rev = props.rev;
  const bmp = await bitmaps
    .load({ doc: props.doc, page: props.box.index, width: w, height: h, priority: props.priority, epoch: props.epoch, rev })
    .catch(() => null);
  if (bmp && bmp.width === base.value.w && rev === props.rev) draw(bmp);
}

onMounted(load);
watch(() => [base.value.w, base.value.h, props.epoch, props.rev], load);

// Liens (chargés à la demande, mis en cache par page).
const links = ref<LinkInfo[]>([]);
onMounted(async () => (links.value = await pageLinks(props.doc, props.box.index)));

const flashOn = ref(false);
watch(
  () => props.flash,
  (f) => {
    if (!f) return;
    flashOn.value = false;
    requestAnimationFrame(() => (flashOn.value = true));
  },
  { immediate: true },
);

function px(r: Rect) {
  return { left: `${r.x * k.value}px`, top: `${r.y * k.value}px`, width: `${r.w * k.value}px`, height: `${r.h * k.value}px` };
}
</script>

<template>
  <div class="page" :style="{ left: box.x + 'px', top: box.y + 'px', width: box.w + 'px', height: box.h + 'px' }" :data-page="box.index">
    <canvas ref="canvas" class="base" />
    <TileCanvas
      v-for="tl in tiles"
      :key="`${pxW}:${tl.x}:${tl.y}`"
      :doc="doc"
      :page="box.index"
      :full-w="pxW"
      :full-h="pxH"
      :tile="tl"
      :dpr="dpr"
      :priority="priority + 1"
      :epoch="epoch"
      :rev="rev"
    />
    <div class="hl">
      <template v-for="(h, i) in highlights" :key="i">
        <span v-for="(r, j) in h.rects" :key="j" class="m" :class="{ cur: h.current }" :style="px(r)" />
      </template>
      <span v-if="flash && flashOn" class="flash" :style="px(flash)" />
    </div>
    <TextLayer :doc="doc" :page="box.index" :width-pt="geom.width" :height-pt="geom.height" :scale="k" />
    <AnnotLayer v-if="tab.edit" :tab="tab" :page="box.index" :k="box.w / geom.width" :page-w="geom.width" :page-h="geom.height" />
    <a
      v-for="(l, i) in links"
      :key="i"
      class="link"
      :style="px(l.rect)"
      :href="l.target.type === 'uri' ? l.target.uri : '#'"
      :title="l.target.type === 'uri' ? l.target.uri : undefined"
      draggable="false"
      @click.prevent="emit('link', l)"
    />
  </div>
</template>

<style scoped>
.page { position: absolute; background: #fff; box-shadow: 0 0 0 1px rgba(0, 0, 0, .06), 0 2px 6px rgba(0, 0, 0, .08), 0 12px 32px rgba(0, 0, 0, .10); contain: strict; }
:root.dark .page { box-shadow: 0 0 0 1px rgba(0, 0, 0, .5), 0 12px 40px rgba(0, 0, 0, .5); }
.base { position: absolute; inset: 0; width: 100%; height: 100%; }
.hl { position: absolute; inset: 0; pointer-events: none; z-index: 1; mix-blend-mode: multiply; }
.m { position: absolute; background: rgba(255, 196, 0, .42); border-radius: 2px; box-shadow: 0 0 0 1px rgba(255, 196, 0, .42); }
.m.cur { background: #ff9f1c; box-shadow: 0 0 0 2px #ff9f1c, 0 2px 10px rgba(255, 140, 0, .55); }
.flash { position: absolute; border-radius: 3px; box-shadow: 0 0 0 3px var(--accent); animation: flash 1.6s ease-out forwards; }
@keyframes flash { 0%, 60% { opacity: 1; } 100% { opacity: 0; } }
.link { position: absolute; z-index: 3; cursor: pointer; border-radius: 2px; }
.link:hover { background: color-mix(in oklab, var(--accent) 12%, transparent); }
.link:focus-visible { box-shadow: var(--ring); }
</style>
