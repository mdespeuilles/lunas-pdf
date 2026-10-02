<script setup lang="ts">
// Grille de pages d'un document (planche 08) : miniatures, sélection, repère d'insertion.
// Le glisser-déposer est piloté par OrganizeView (il peut traverser deux grilles).
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import ThumbCanvas from "./ThumbCanvas.vue";
import type { DocTab } from "../stores/tabs";

const props = defineProps<{
  tab: DocTab;
  /** Largeur d'une miniature portrait, en px. */
  size: number;
  /** Repère d'insertion avant cette page (longueur : à la fin). */
  dropAt: number | null;
  /** Pages en cours de déplacement (estompées). */
  dragging: number[];
}>();
const emit = defineEmits<{ press: [index: number, e: PointerEvent]; open: [index: number]; focusPane: [] }>();
const { t } = useI18n();
const scroller = ref<HTMLElement>();
const cells = ref<HTMLElement[]>([]);
/** Miniatures déjà affichées (chargées à l'approche de la zone visible). */
const seen = ref(new Set<number>());

const pages = computed(() => props.tab.info?.pages ?? []);
/** Boîte de chaque miniature : le grand côté vaut √2 × `size` (format A). */
const boxes = computed(() =>
  pages.value.map((g) => {
    const ratio = g.width / g.height;
    return ratio <= 1 ? { w: props.size, h: Math.round(props.size / ratio) } : { w: Math.round(props.size * Math.SQRT2), h: Math.round((props.size * Math.SQRT2) / ratio) };
  }),
);
const cellW = computed(() => Math.round(props.size * Math.SQRT2) + 8);
const cellH = computed(() => Math.round(props.size * Math.SQRT2) + 28);
const selected = computed(() => new Set(props.tab.orgSel));
const dimmed = computed(() => new Set(props.dragging));

let io: IntersectionObserver | null = null;
function observe() {
  io?.disconnect();
  io = new IntersectionObserver(
    (entries) => {
      const next = new Set(seen.value);
      for (const e of entries) if (e.isIntersecting) next.add(Number((e.target as HTMLElement).dataset.index));
      seen.value = next;
    },
    { root: scroller.value, rootMargin: "600px 0px" },
  );
  for (const c of cells.value) io.observe(c);
}
onMounted(observe);
watch(() => pages.value.length, () => void nextTick(observe));
onBeforeUnmount(() => io?.disconnect());

/** Sélection au clavier : flèches (Maj : étendre), Espace (basculer), Entrée (ouvrir). */
function onKey(e: KeyboardEvent, i: number) {
  const cols = Math.max(1, Math.round((scroller.value?.querySelector(".pgrid")?.clientWidth ?? cellW.value) / (cellW.value + 28)));
  const step: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -cols, ArrowDown: cols };
  let target = -1;
  if (step[e.key] !== undefined) target = Math.min(pages.value.length - 1, Math.max(0, i + step[e.key]));
  else if (e.key === "Home") target = 0;
  else if (e.key === "End") target = pages.value.length - 1;
  if (target >= 0) {
    e.preventDefault();
    if (e.shiftKey) {
      const a = Math.min(i, target);
      const b = Math.max(i, target);
      props.tab.orgSel = [...new Set([...props.tab.orgSel, ...Array.from({ length: b - a + 1 }, (_, k) => a + k)])];
    } else if (!e.ctrlKey && !e.metaKey) {
      props.tab.orgSel = [target];
    }
    cells.value.find((c) => Number(c.dataset.index) === target)?.focus();
    return;
  }
  if (e.key === " ") {
    e.preventDefault();
    props.tab.orgSel = selected.value.has(i) ? props.tab.orgSel.filter((x) => x !== i) : [...props.tab.orgSel, i];
  } else if (e.key === "Enter") {
    e.preventDefault();
    emit("open", i);
  }
}

/** Garde la page sélectionnée visible (après une opération). */
watch(
  () => props.tab.orgSel[0],
  (i) => void nextTick(() => cells.value.find((c) => Number(c.dataset.index) === i)?.scrollIntoView({ block: "nearest" })),
);

defineExpose({ scroller });
</script>

<template>
  <div ref="scroller" class="scroll" :data-pane="tab.key" @pointerdown="emit('focusPane')">
    <div
      class="pgrid"
      role="listbox"
      aria-multiselectable="true"
      :aria-label="t('organize.pagesOf', { name: tab.name })"
      :style="{ gridTemplateColumns: `repeat(auto-fill, ${cellW}px)`, gridAutoRows: `${cellH}px` }"
    >
      <div
        v-for="(g, i) in pages"
        :key="i"
        ref="cells"
        class="pt"
        :class="{ sel: selected.has(i), gone: dimmed.has(i) }"
        role="option"
        :aria-selected="selected.has(i)"
        :aria-label="t('organize.page', { n: i + 1 })"
        :tabindex="(tab.orgSel[0] ?? 0) === i ? 0 : -1"
        :data-index="i"
        @pointerdown="emit('press', i, $event)"
        @dblclick="emit('open', i)"
        @keydown="onKey($event, i)"
      >
        <span v-if="dropAt === i" class="ins" :style="{ height: cellH - 12 + 'px' }" aria-hidden="true" />
        <div class="pp" :style="{ width: boxes[i].w + 'px', height: boxes[i].h + 'px' }">
          <ThumbCanvas v-if="seen.has(i) && tab.info" :doc="tab.info.id" :page="i" :width="boxes[i].w" :height="boxes[i].h" :rev="tab.pageRev[i] ?? 0" />
        </div>
        <span class="num">{{ g.label ?? i + 1 }}</span>
      </div>
      <!-- Repère après la dernière page. -->
      <div v-if="dropAt === pages.length" class="pt end" aria-hidden="true">
        <span class="ins" :style="{ height: cellH - 12 + 'px' }" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.scroll { flex: 1; overflow: auto; min-height: 0; padding-bottom: 28px; }
.pgrid { display: grid; gap: 22px 28px; padding: 8px 28px; justify-content: start; }
.pt { position: relative; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; gap: 8px; font-size: 11.5px; color: var(--text-2); font-variant-numeric: tabular-nums; outline: none; user-select: none; -webkit-user-select: none; cursor: grab; }
.pp { position: relative; background: #fff; border-radius: 3px; box-shadow: 0 0 0 1px rgba(0, 0, 0, .08), 0 2px 6px rgba(0, 0, 0, .10); overflow: hidden; flex: none; }
.pp :deep(canvas) { border-radius: 3px; }
.pt.sel .pp { box-shadow: 0 0 0 2.5px var(--accent), 0 0 0 7px var(--accent-soft); }
.pt.sel .num { background: var(--accent); color: var(--on-accent); }
.pt:focus-visible .pp { box-shadow: 0 0 0 2.5px var(--accent), var(--ring); }
.pt.gone .pp { opacity: .3; box-shadow: 0 0 0 1.5px var(--accent); }
.num { padding: 1px 7px; border-radius: 9px; }
.ins { position: absolute; top: 0; left: -16px; width: 3px; border-radius: 2px; background: var(--accent); z-index: 3; }
.ins::before, .ins::after { content: ""; position: absolute; left: -3.5px; width: 10px; height: 10px; border-radius: 50%; background: var(--accent); }
.ins::before { top: -5px; }
.ins::after { bottom: -5px; }
</style>
