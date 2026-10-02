<script setup lang="ts">
// Miniature d'une page (priorité basse, jamais annulée par un changement d'époque).
import { onMounted, ref, watch } from "vue";
import { bitmaps } from "../lib/bitmap-cache";

const props = defineProps<{ doc: number; page: number; width: number; height: number; rev?: number }>();
const canvas = ref<HTMLCanvasElement>();
const loaded = ref(false);
const NEVER_STALE = 4294967295;

async function draw() {
  const dpr = window.devicePixelRatio || 1;
  const w = Math.round(props.width * dpr);
  const h = Math.round(props.height * dpr);
  const bmp = await bitmaps.load({ doc: props.doc, page: props.page, width: w, height: h, priority: 20, epoch: NEVER_STALE, rev: props.rev }).catch(() => null);
  const c = canvas.value;
  if (!bmp || !c) return;
  c.width = bmp.width;
  c.height = bmp.height;
  c.getContext("2d")!.drawImage(bmp, 0, 0);
  loaded.value = true;
}

onMounted(draw);
watch(() => [props.doc, props.page, props.width, props.rev], draw);
</script>

<template>
  <canvas ref="canvas" class="thumb" :class="{ loaded }" :style="{ width: width + 'px', height: height + 'px' }" />
</template>

<style scoped>
.thumb { display: block; background: #fff; }
</style>
