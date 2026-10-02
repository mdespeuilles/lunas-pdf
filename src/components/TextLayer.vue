<script setup lang="ts">
// Couche de texte invisible et sélectionnable, superposée au rendu de la page.
// Construite en points PDF puis mise à l'échelle par une transformation CSS (pas de recalcul au zoom).
import { onMounted, ref } from "vue";
import type { PageText } from "../bindings";
import { pageText } from "../lib/page-data";

const props = defineProps<{ doc: number; page: number; widthPt: number; heightPt: number; scale: number }>();
const runs = ref<{ text: string; style: Record<string, string>; eol: boolean }[]>([]);

let measureCtx: CanvasRenderingContext2D | null = null;

function measure(text: string, size: number): number {
  measureCtx ??= document.createElement("canvas").getContext("2d");
  measureCtx!.font = `${size}px sans-serif`;
  return measureCtx!.measureText(text).width;
}

onMounted(async () => {
  let text: PageText;
  try {
    text = await pageText(props.doc, props.page);
  } catch {
    return;
  }
  runs.value = text.runs.map((r) => {
    const size = r.rect.h;
    const visible = r.text.trimEnd();
    const w = visible ? measure(visible, size) : 0;
    const sx = w > 0 ? r.rect.w / w : 1;
    return {
      text: r.text,
      eol: r.eol,
      style: {
        left: `${r.rect.x}px`,
        top: `${r.rect.y}px`,
        fontSize: `${size}px`,
        transform: `scaleX(${sx.toFixed(4)})`,
      },
    };
  });
});
</script>

<template>
  <div class="textLayer" :style="{ width: widthPt + 'px', height: heightPt + 'px', transform: `scale(${scale})` }">
    <template v-for="(r, i) in runs" :key="i"><span :style="r.style">{{ r.text }}</span><br v-if="r.eol" /></template>
  </div>
</template>

<style scoped>
.textLayer { position: absolute; left: 0; top: 0; transform-origin: 0 0; line-height: 1; user-select: text; -webkit-user-select: text; cursor: text; z-index: 2; }
.textLayer span { position: absolute; white-space: pre; color: transparent; transform-origin: 0 0; font-family: sans-serif; }
.textLayer ::selection { background: color-mix(in oklab, var(--accent) 32%, transparent); color: transparent; }
</style>
