<script setup lang="ts">
// Aperçu d'une annotation pendant un déplacement ou un redimensionnement : reproduit son
// apparence PDF (mêmes règles que `appearance.rs`) pendant que l'original est masqué.
import { computed } from "vue";
import type { Annot } from "../bindings";
import { FREETEXT_LEADING, FREETEXT_PAD } from "../lib/afm";

const props = defineProps<{ annot: Annot; k: number; crop?: string | null }>();

const FONT_CSS = { sans: "Helvetica, Arial, 'Liberation Sans', sans-serif", serif: "'Times New Roman', Times, 'Liberation Serif', serif", mono: "'Courier New', Courier, 'Liberation Mono', monospace" } as const;

const r = computed(() => props.annot.rect);
const box = computed(() => ({ left: `${r.value.x * props.k}px`, top: `${r.value.y * props.k}px`, width: `${r.value.w * props.k}px`, height: `${r.value.h * props.k}px` }));
const sw = computed(() => Math.max(0.1, props.annot.width) * props.k);

/** Pointe de flèche ouverte (même géométrie que `appearance::arrow_head`). */
const arrow = computed(() => {
  const b = props.annot.body;
  if (b.type !== "line" || !b.arrow) return null;
  const [ax, ay, bx, by] = [b.from.x, b.from.y, b.to.x, b.to.y].map((v) => v * props.k);
  const len = Math.hypot(bx - ax, by - ay) || 1;
  const [ux, uy] = [(bx - ax) / len, (by - ay) / len];
  const l = Math.max(props.annot.width * 3 + 6, 8) * props.k;
  const [c, s] = [Math.cos(Math.PI / 6), Math.sin(Math.PI / 6)];
  const p1 = [bx - l * (ux * c - uy * s), by - l * (uy * c + ux * s)];
  const p2 = [bx - l * (ux * c + uy * s), by - l * (uy * c - ux * s)];
  return `M${p1[0]} ${p1[1]} L${bx} ${by} L${p2[0]} ${p2[1]}`;
});

/** Coche, croix, point dans un carré de 24 (mêmes tracés que l'apparence). */
const checkPath = computed(() => {
  const b = props.annot.body;
  if (b.type !== "check") return null;
  const s = Math.min(r.value.w, r.value.h) * props.k;
  const ox = (r.value.w * props.k - s) / 2;
  const oy = (r.value.h * props.k - s) / 2;
  const p = (x: number, y: number) => `${ox + (x / 24) * s} ${oy + (y / 24) * s}`;
  if (b.style === "check") return { d: `M${p(20, 6)} L${p(9, 17)} L${p(4, 12)}`, dot: null };
  if (b.style === "cross") return { d: `M${p(18, 6)} L${p(6, 18)} M${p(6, 6)} L${p(18, 18)}`, dot: null };
  return { d: null, dot: { cx: ox + s / 2, cy: oy + s / 2, r: s * 0.3 } };
});
</script>

<template>
  <!-- Ligne : coordonnées de page (le SVG couvre la page entière). -->
  <svg v-if="annot.body.type === 'line'" class="full">
    <line :x1="annot.body.from.x * k" :y1="annot.body.from.y * k" :x2="annot.body.to.x * k" :y2="annot.body.to.y * k" :stroke="annot.color" :stroke-width="sw" stroke-linecap="round" />
    <path v-if="arrow" :d="arrow" fill="none" :stroke="annot.color" :stroke-width="sw" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
  <div
    v-else-if="annot.body.type === 'freeText'"
    class="box ft"
    :style="{ ...box, fontFamily: FONT_CSS[annot.body.font], fontSize: annot.body.size * k + 'px', lineHeight: FREETEXT_LEADING, color: annot.color, padding: FREETEXT_PAD * k + 'px' }"
  >{{ annot.body.text }}</div>
  <img v-else-if="crop" class="box" :style="box" :src="crop" alt="" draggable="false" />
  <svg v-else class="box" :style="box">
    <rect v-if="annot.body.type === 'square'" :x="sw / 2" :y="sw / 2" :width="Math.max(0, r.w * k - sw)" :height="Math.max(0, r.h * k - sw)" fill="none" :stroke="annot.color" :stroke-width="sw" />
    <ellipse v-else-if="annot.body.type === 'circle'" :cx="(r.w * k) / 2" :cy="(r.h * k) / 2" :rx="Math.max(0, (r.w * k - sw) / 2)" :ry="Math.max(0, (r.h * k - sw) / 2)" fill="none" :stroke="annot.color" :stroke-width="sw" />
    <template v-else-if="checkPath">
      <path v-if="checkPath.d" :d="checkPath.d" fill="none" :stroke="annot.color" :stroke-width="sw" stroke-linecap="round" stroke-linejoin="round" />
      <circle v-if="checkPath.dot" :cx="checkPath.dot.cx" :cy="checkPath.dot.cy" :r="checkPath.dot.r" :fill="annot.color" />
    </template>
    <template v-else-if="annot.body.type === 'note'">
      <rect :width="r.w * k" :height="r.h * k" :rx="4 * k" :fill="annot.color" />
      <path
        :d="`M${5.5 * k} ${6.5 * k} H${16.5 * k} V${14 * k} H${10 * k} L${7 * k} ${16.5 * k} V${14 * k} H${5.5 * k} Z`"
        fill="none"
        stroke="#5c4400"
        :stroke-width="1.4 * k"
        stroke-linejoin="round"
      />
    </template>
    <rect v-else-if="annot.body.type === 'redact'" x="0.5" y="0.5" :width="Math.max(0, r.w * k - 1)" :height="Math.max(0, r.h * k - 1)" fill="none" stroke="#e03131" />
  </svg>
</template>

<style scoped>
.full { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; pointer-events: none; }
.box { position: absolute; pointer-events: none; overflow: visible; }
.ft { box-sizing: border-box; white-space: pre-wrap; overflow-wrap: anywhere; overflow: hidden; }
</style>
