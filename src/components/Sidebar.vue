<script setup lang="ts">
// Barre latérale (planches 02 et 09) : miniatures, sommaire et signets, annotations, résultats.
import { ArrowUpRight, Check as CheckIcon, Circle, EyeOff, Highlighter, Image as ImageIcon, MessageSquare, List, Minus, Rows2, Square, Strikethrough, Type, Underline, X } from "lucide-vue-next";
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import ThumbCanvas from "./ThumbCanvas.vue";
import OutlineNode from "./OutlineNode.vue";
import { type DocTab, type SidebarTab, useTabs } from "../stores/tabs";
import type { Annot, SearchHit } from "../bindings";
import { kindKey, pdfDateTime } from "../lib/annot-geom";

const props = defineProps<{ tab: DocTab }>();
const { t, locale } = useI18n();
const tabs = useTabs();
const info = computed(() => props.tab.info!);

function select(which: SidebarTab) {
  props.tab.sidebarTab = which;
  if (which === "outline") void tabs.loadOutline(props.tab);
  if (which === "annots") void tabs.loadAnnotations(props.tab);
}
onMounted(() => select(props.tab.sidebarTab === "search" && !props.tab.search.query ? "thumbs" : props.tab.sidebarTab));

// --- Miniatures virtualisées ------------------------------------------------------------
const THUMB_W = 100;
const ITEM_GAP = 16;
const LABEL_H = 25;
const scroller = ref<HTMLElement>();
const scrollTop = ref(0);
const viewH = ref(600);
const items = computed(() => {
  let y = 8;
  return info.value.pages.map((p, i) => {
    const h = Math.round((THUMB_W * p.height) / p.width);
    const it = { i, y, h };
    y += h + LABEL_H + ITEM_GAP;
    return it;
  });
});
const totalH = computed(() => {
  const last = items.value.at(-1);
  return last ? last.y + last.h + LABEL_H + 16 : 0;
});
const visibleThumbs = computed(() => {
  const top = scrollTop.value - 400;
  const bottom = scrollTop.value + viewH.value + 400;
  return items.value.filter((it) => it.y + it.h + LABEL_H > top && it.y < bottom);
});

function onScroll() {
  if (!scroller.value) return;
  scrollTop.value = scroller.value.scrollTop;
  viewH.value = scroller.value.clientHeight;
}

// Garde la miniature de la page courante visible.
watch(
  () => props.tab.page,
  async (p) => {
    if (props.tab.sidebarTab !== "thumbs") return;
    await nextTick();
    const el = scroller.value;
    const it = items.value[p];
    if (!el || !it) return;
    if (it.y < el.scrollTop || it.y + it.h + LABEL_H > el.scrollTop + el.clientHeight) {
      el.scrollTo({ top: it.y - el.clientHeight / 2 + it.h / 2 });
    }
  },
);

function pageLabel(i: number) {
  return info.value.pages[i].label ?? String(i + 1);
}

// --- Annotations (planche 03) --------------------------------------------------------------
const ICONS: Record<string, unknown> = {
  highlight: Highlighter, underline: Underline, strikeOut: Strikethrough, freeText: Type, note: MessageSquare,
  square: Square, circle: Circle, line: Minus, arrow: ArrowUpRight, check: CheckIcon, image: ImageIcon, redact: EyeOff, other: MessageSquare,
};
/** Fond de la pastille : teinte claire de la couleur de l'annotation. */
function tint(hex: string) {
  return `color-mix(in oklab, ${hex} 38%, white)`;
}
const annots = computed(() => props.tab.edit?.annots ?? []);
const annotGroups = computed(() => {
  const m = new Map<number, Annot[]>();
  for (const a of annots.value) m.set(a.page, [...(m.get(a.page) ?? []), a]);
  return [...m.entries()].sort((a, b) => a[0] - b[0]);
});
const onPage = computed(() => annots.value.filter((a) => a.page === props.tab.page).length);
function excerpt(a: Annot) {
  if (a.body.type === "freeText") return a.body.text;
  if (a.excerpt) return `« ${a.excerpt} »`;
  return a.contents ?? "";
}
function meta(a: Annot) {
  const who = !a.author || a.author === props.tab.edit?.author ? t("annot.you") : a.author;
  const when = pdfDateTime(a.modified, locale.value);
  return when ? `${who} · ${when}` : who;
}
function gotoAnnot(a: Annot) {
  tabs.goto(props.tab, a.page, a.rect.y);
  if (props.tab.annotating) props.tab.selected = a.id;
  else props.tab.flash = { page: a.page, rect: a.rect, seq: Date.now() };
}

// --- Résultats de recherche ------------------------------------------------------------
const hitGroups = computed(() => {
  const m = new Map<number, { hit: SearchHit; index: number }[]>();
  props.tab.search.hits.forEach((hit, index) => m.set(hit.page, [...(m.get(hit.page) ?? []), { hit, index }]));
  return [...m.entries()];
});
const resultsList = ref<HTMLElement>();
watch(
  () => props.tab.search.current,
  async () => {
    await nextTick();
    resultsList.value?.querySelector(".ri.on")?.scrollIntoView({ block: "nearest" });
  },
);
function closeResults() {
  tabs.search(props.tab, "");
}
</script>

<template>
  <aside class="side" :class="{ wide: tab.sidebarTab === 'search', annots: tab.sidebarTab === 'annots' }" :aria-label="t('sidebar.tabs')">
    <template v-if="tab.sidebarTab === 'search'">
      <div class="sideh">
        <span>{{ t("search.results") }}</span>
        <button class="cl" :aria-label="t('search.backToThumbs')" :title="t('search.backToThumbs')" @click="closeResults">
          <X class="ic xs" aria-hidden="true" />
        </button>
      </div>
      <div class="sub2" aria-live="polite">
        <template v-if="tab.search.hits.length">
          {{ t("search.summary", { count: tab.search.hits.length, q: tab.search.query, pages: t("search.inPages", { n: hitGroups.length }, hitGroups.length) }, tab.search.hits.length) }}
        </template>
        <template v-else>{{ tab.search.running ? t("search.searching") : t("search.none") }}</template>
      </div>
      <div ref="resultsList" class="res scroll">
        <template v-for="[page, list] in hitGroups" :key="page">
          <div class="rg">{{ t("search.page", { n: pageLabel(page) }) }}<span>{{ list.length }}</span></div>
          <button v-for="{ hit, index } in list" :key="index" class="ri" :class="{ on: index === tab.search.current }" @click="tabs.selectHit(tab, index)">
            {{ hit.before ? "…" + hit.before.trimStart() : "" }}<b>{{ hit.matched }}</b>{{ hit.after ? hit.after.trimEnd() + "…" : "" }}
          </button>
        </template>
      </div>
    </template>

    <template v-else>
      <div class="seg" role="tablist">
        <button role="tab" :aria-selected="tab.sidebarTab === 'thumbs'" :class="{ on: tab.sidebarTab === 'thumbs' }" :aria-label="t('sidebar.thumbnails')" :title="t('sidebar.thumbnails')" @click="select('thumbs')">
          <Rows2 class="ic s" aria-hidden="true" />
        </button>
        <button role="tab" :aria-selected="tab.sidebarTab === 'outline'" :class="{ on: tab.sidebarTab === 'outline' }" :aria-label="t('sidebar.outline')" :title="t('sidebar.outline')" @click="select('outline')">
          <List class="ic s" aria-hidden="true" />
        </button>
        <button role="tab" :aria-selected="tab.sidebarTab === 'annots'" :class="{ on: tab.sidebarTab === 'annots' }" :aria-label="t('sidebar.annotations')" :title="t('sidebar.annotations')" @click="select('annots')">
          <MessageSquare class="ic s" aria-hidden="true" />
        </button>
      </div>

      <template v-if="tab.sidebarTab === 'thumbs'">
        <div class="sideh">
          <span>{{ t("sidebar.thumbnails") }}</span>
          <span class="count">{{ t("sidebar.pageCount", { n: info.pages.length }, info.pages.length) }}</span>
        </div>
        <div ref="scroller" class="scroll" @scroll.passive="onScroll" @vue:mounted="onScroll">
          <div class="thumbs" :style="{ height: totalH + 'px' }">
            <button
              v-for="it in visibleThumbs"
              :key="it.i"
              class="th-i"
              :class="{ on: it.i === tab.page }"
              :style="{ top: it.y + 'px' }"
              :aria-label="t('sidebar.page', { n: pageLabel(it.i) })"
              :aria-current="it.i === tab.page ? 'page' : undefined"
              @click="tabs.goto(tab, it.i)"
            >
              <span class="thp"><ThumbCanvas :doc="info.id" :page="it.i" :width="THUMB_W" :height="it.h" :rev="tab.pageRev[it.i] ?? 0" /></span>
              <span class="num">{{ pageLabel(it.i) }}</span>
            </button>
          </div>
        </div>
      </template>

      <template v-else-if="tab.sidebarTab === 'outline'">
        <div class="sideh"><span>{{ t("sidebar.outline") }}</span></div>
        <div class="scroll">
          <p v-if="tab.outline && !tab.outline.length" class="empty">{{ t("sidebar.noOutline") }}</p>
          <ul v-else class="tree" role="tree">
            <OutlineNode v-for="(item, i) in tab.outline ?? []" :key="i" :item="item" :tab="tab" :depth="0" />
          </ul>
        </div>
      </template>

      <template v-else>
        <div class="sideh">
          <span>{{ t("sidebar.annotations") }}</span>
          <span v-if="tab.edit" class="count">{{ t("annot.onPage", { n: onPage }) }}</span>
        </div>
        <div class="scroll">
          <p v-if="tab.edit && !annots.length" class="empty">{{ t("sidebar.noAnnotations") }}</p>
          <div class="alist">
            <template v-for="[page, list] in annotGroups" :key="page">
              <div class="ag">{{ t("sidebar.page", { n: pageLabel(page) }) }}</div>
              <button v-for="a in list" :key="a.id" class="ai" :class="{ on: a.id === tab.selected }" @click="gotoAnnot(a)">
                <span class="aic" :style="{ background: tint(a.color) }"><component :is="ICONS[kindKey(a)] ?? MessageSquare" class="ic s" aria-hidden="true" /></span>
                <span class="am-w">
                  <span class="at">{{ t(`annot.kinds.${kindKey(a)}`) }}</span>
                  <span v-if="excerpt(a)" class="ax">{{ excerpt(a) }}</span>
                  <span class="am">{{ meta(a) }}</span>
                </span>
              </button>
            </template>
          </div>
        </div>
      </template>
    </template>
  </aside>
</template>

<style scoped>
.side { width: 232px; flex: none; background: var(--sidebar); border-right: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
.side.wide { width: 268px; }
.side.annots { width: 248px; }
.side .seg { margin: 10px 10px 4px; }
.side .seg > button { flex: 1; }
.sideh { display: flex; align-items: center; justify-content: space-between; padding: 8px 14px 6px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-3); }
.side.wide .sideh { padding: 12px 14px 4px; }
.count { text-transform: none; letter-spacing: 0; font-weight: 500; }
.scroll { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; }
.empty { margin: 8px 14px; color: var(--text-3); font-size: 12px; }

.thumbs { position: relative; }
.th-i { position: absolute; left: 0; right: 0; display: flex; flex-direction: column; align-items: center; gap: 7px; font-size: 11.5px; color: var(--text-2); font-variant-numeric: tabular-nums; border: 0; background: transparent; padding: 0; }
.thp { display: block; border-radius: 3px; overflow: hidden; box-shadow: 0 0 0 1px rgba(0, 0, 0, .10), 0 1px 3px rgba(0, 0, 0, .10); background: #fff; }
.th-i:hover .thp { box-shadow: 0 0 0 1px var(--line-2), 0 2px 6px rgba(0, 0, 0, .14); }
.th-i.on .thp { box-shadow: 0 0 0 2px var(--accent), 0 0 0 6px var(--accent-soft); }
.th-i:focus-visible { box-shadow: none; }
.th-i:focus-visible .thp { box-shadow: 0 0 0 2px var(--accent), var(--ring); }
.th-i .num { padding: 1px 7px; border-radius: 9px; }
.th-i.on .num { background: var(--accent); color: var(--on-accent); }

.sub2 { padding: 0 14px 6px; font-size: 12px; color: var(--text-2); }
.res { display: flex; flex-direction: column; gap: 1px; padding: 0 8px 12px; }
.rg { display: flex; justify-content: space-between; padding: 10px 6px 4px; font-size: 11.5px; font-weight: 600; color: var(--text-2); }
.scroll > .rg { padding: 10px 14px 4px; }
.rg span { font-weight: 500; color: var(--text-3); }
.ri { padding: 7px 8px; border-radius: 7px; font-size: 12px; color: var(--text-2); line-height: 1.45; border: 0; background: transparent; text-align: left; overflow-wrap: anywhere; }
.ri:hover { background: var(--hover); }
.ri b { color: var(--text); font-weight: 600; background: rgba(255, 196, 0, .38); border-radius: 2px; padding: 0 1px; }
.ri.on { background: var(--accent); color: rgba(255, 255, 255, .88); }
.ri.on b { color: #fff; background: rgba(255, 255, 255, .24); }

.tree { list-style: none; margin: 0; padding: 2px 8px 12px; }
.alist { display: flex; flex-direction: column; gap: 2px; padding: 0 8px 12px; }
.ag { padding: 8px 6px 4px; font-size: 11.5px; font-weight: 600; color: var(--text-2); }
.ai { display: flex; gap: 10px; padding: 8px; border-radius: 8px; border: 0; background: transparent; text-align: left; color: var(--text); width: 100%; }
.ai:hover { background: var(--hover); }
.ai.on { background: var(--accent-soft); }
.aic { display: grid; place-items: center; width: 26px; height: 26px; border-radius: 7px; flex: none; color: #1b1b20; }
.am-w { display: flex; flex-direction: column; min-width: 0; }
.at { font-size: 12.5px; font-weight: 500; }
.ax { font-size: 12px; color: var(--text-2); margin-top: 1px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 170px; }
.am { font-size: 11px; color: var(--text-3); margin-top: 2px; }
</style>
