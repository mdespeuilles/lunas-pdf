<script setup lang="ts">
// Tuile pleine résolution pour les forts zooms.
import { onMounted, ref, watch } from "vue";
import { bitmaps } from "../lib/bitmap-cache";
import type { TileRect } from "../lib/protocol";

const props = defineProps<{ doc: number; page: number; fullW: number; fullH: number; tile: TileRect; dpr: number; priority: number; epoch: number; rev: number }>();
const canvas = ref<HTMLCanvasElement>();

async function load() {
  const bmp = await bitmaps
    .load({ doc: props.doc, page: props.page, width: props.fullW, height: props.fullH, tile: props.tile, priority: props.priority, epoch: props.epoch, rev: props.rev })
    .catch(() => null);
  const c = canvas.value;
  if (!bmp || !c) return;
  c.width = bmp.width;
  c.height = bmp.height;
  c.getContext("2d")!.drawImage(bmp, 0, 0);
}
onMounted(load);
watch(() => [props.epoch, props.rev], load);
</script>

<template>
  <canvas
    ref="canvas"
    class="tile"
    :style="{ left: tile.x / dpr + 'px', top: tile.y / dpr + 'px', width: tile.w / dpr + 'px', height: tile.h / dpr + 'px' }"
  />
</template>

<style scoped>
.tile { position: absolute; }
</style>
