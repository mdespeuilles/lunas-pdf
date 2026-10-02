<script setup lang="ts">
// Barre d'outils de lecture (planche 02).
import {
  ChevronDown, ChevronLeft, ChevronRight, Check, Columns2, GalleryVertical, LayoutGrid, MoveHorizontal,
  PanelLeft, PenLine, RectangleVertical, Scan, Share, ZoomIn, ZoomOut,
} from "lucide-vue-next";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import AppMenu from "./AppMenu.vue";
import Dropdown from "./Dropdown.vue";
import SearchBox from "./SearchBox.vue";
import { ZOOM_PRESETS } from "../lib/layout";
import { percent } from "../lib/format";
import { toggleAnnotate } from "../composables/signed";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab; disabled?: boolean }>();
const { t, locale } = useI18n();
const tabs = useTabs();
const searchBox = ref<InstanceType<typeof SearchBox>>();
const pageInput = ref<HTMLInputElement>();
const count = computed(() => props.tab.info?.pages.length ?? 0);
const pageText = ref("");
watch(() => props.tab.page, (p) => (pageText.value = String(p + 1)), { immediate: true });
const zoomLabel = computed(() => percent(props.tab.zoom, locale.value as "fr" | "en"));

function commitPage() {
  const n = parseInt(pageText.value, 10);
  if (Number.isFinite(n)) tabs.goto(props.tab, n - 1);
  pageText.value = String(props.tab.page + 1);
}

defineExpose({
  focusSearch: () => searchBox.value?.focus(),
  toggleCase: () => searchBox.value?.toggleCase(),
  focusPage: () => {
    pageInput.value?.focus();
    pageInput.value?.select();
  },
});
</script>

<template>
  <div class="toolbar" role="toolbar" :aria-label="tab.name">
    <div class="row" :class="{ off: disabled }">
      <button class="tb" :class="{ on: tab.sidebarOpen }" :disabled="disabled" :aria-pressed="tab.sidebarOpen" :aria-label="t('toolbar.sidebar')" :title="t('toolbar.sidebar')" @click="tabs.toggleSidebar(tab)">
        <PanelLeft class="ic" aria-hidden="true" />
      </button>
      <div class="sep" />
      <div class="pgnav">
        <template v-if="!disabled">
          <button class="tb" :aria-label="t('toolbar.prevPage')" :title="t('toolbar.prevPage')" :disabled="tab.page <= 0" @click="tabs.step(tab, -1)">
            <ChevronLeft class="ic" aria-hidden="true" />
          </button>
          <input
            ref="pageInput"
            v-model="pageText"
            class="pgin"
            inputmode="numeric"
            :aria-label="t('toolbar.gotoPage')"
            :title="t('toolbar.gotoPage')"
            @keydown.enter="($event.target as HTMLInputElement).blur()"
            @keydown.esc="pageText = String(tab.page + 1); ($event.target as HTMLInputElement).blur()"
            @blur="commitPage"
          />
          <span>{{ t("toolbar.of", { n: count }) }}</span>
          <button class="tb" :aria-label="t('toolbar.nextPage')" :title="t('toolbar.nextPage')" :disabled="tab.page >= count - 1" @click="tabs.step(tab, 1)">
            <ChevronRight class="ic" aria-hidden="true" />
          </button>
        </template>
        <span v-else class="muted">Page – {{ t("toolbar.of", { n: "–" }) }}</span>
      </div>
      <template v-if="!disabled">
        <div class="sep" />
        <button class="tb" :aria-label="t('toolbar.zoomOut')" :title="t('toolbar.zoomOut')" @click="tabs.zoomBy(tab, -1)">
          <ZoomOut class="ic" aria-hidden="true" />
        </button>
        <Dropdown :width="236">
          <template #trigger="{ open, toggle }">
            <button class="zoomsel" :class="{ open }" :aria-label="t('toolbar.zoomLevel')" aria-haspopup="menu" :aria-expanded="open" @click="toggle(true)">
              {{ zoomLabel }}<ChevronDown class="ic xs" aria-hidden="true" />
            </button>
          </template>
          <button class="mi" role="menuitem" :class="{ sel: tab.fit === 'width' }" @click="tabs.setFit(tab, 'width')">
            <span class="ck"><Check v-if="tab.fit === 'width'" class="ic xs" /></span>{{ t("toolbar.fitWidth") }}<span class="kbd">W</span>
          </button>
          <button class="mi" role="menuitem" :class="{ sel: tab.fit === 'page' }" @click="tabs.setFit(tab, 'page')">
            <span class="ck"><Check v-if="tab.fit === 'page'" class="ic xs" /></span>{{ t("toolbar.fitPage") }}<span class="kbd">F</span>
          </button>
          <button class="mi" role="menuitem" @click="tabs.setZoom(tab, 1)">
            <span class="ck" />{{ t("toolbar.actualSize") }}<span class="kbd">Ctrl 0</span>
          </button>
          <div class="msep" />
          <button v-for="z in ZOOM_PRESETS" :key="z" class="mi" role="menuitem" :class="{ on: !tab.fit && Math.abs(tab.zoom - z) < 0.001 }" @click="tabs.setZoom(tab, z)">
            <span class="ck" />{{ percent(z, locale as "fr" | "en") }}
          </button>
          <div class="msep" />
          <button class="mi" role="menuitem" @click="tabs.zoomBy(tab, 1)"><span class="ck" />{{ t("toolbar.zoomInItem") }}<span class="kbd">Ctrl +</span></button>
          <button class="mi" role="menuitem" @click="tabs.zoomBy(tab, -1)"><span class="ck" />{{ t("toolbar.zoomOutItem") }}<span class="kbd">Ctrl −</span></button>
        </Dropdown>
        <button class="tb" :aria-label="t('toolbar.zoomIn')" :title="t('toolbar.zoomIn')" @click="tabs.zoomBy(tab, 1)">
          <ZoomIn class="ic" aria-hidden="true" />
        </button>
        <div class="seg fit" role="radiogroup">
          <button :class="{ on: tab.fit === 'width' }" role="radio" :aria-checked="tab.fit === 'width'" :aria-label="t('toolbar.fitWidthKey')" :title="t('toolbar.fitWidthKey')" @click="tabs.setFit(tab, 'width')">
            <MoveHorizontal class="ic s" aria-hidden="true" />
          </button>
          <button :class="{ on: tab.fit === 'page' }" role="radio" :aria-checked="tab.fit === 'page'" :aria-label="t('toolbar.fitPageKey')" :title="t('toolbar.fitPageKey')" @click="tabs.setFit(tab, 'page')">
            <Scan class="ic s" aria-hidden="true" />
          </button>
        </div>
        <div class="sep" />
        <div class="seg" role="radiogroup">
          <button :class="{ on: tab.mode === 'single' }" role="radio" :aria-checked="tab.mode === 'single'" :aria-label="t('toolbar.single')" :title="t('toolbar.single')" @click="tabs.setMode(tab, 'single')">
            <RectangleVertical class="ic s" aria-hidden="true" />
          </button>
          <button :class="{ on: tab.mode === 'continuous' }" role="radio" :aria-checked="tab.mode === 'continuous'" :aria-label="t('toolbar.continuous')" :title="t('toolbar.continuous')" @click="tabs.setMode(tab, 'continuous')">
            <GalleryVertical class="ic s" aria-hidden="true" />
          </button>
          <button :class="{ on: tab.mode === 'double' }" role="radio" :aria-checked="tab.mode === 'double'" :aria-label="t('toolbar.double')" :title="t('toolbar.double')" @click="tabs.setMode(tab, 'double')">
            <Columns2 class="ic s" aria-hidden="true" />
          </button>
        </div>
      </template>
    </div>
    <div class="grow" />
    <SearchBox v-if="!disabled" ref="searchBox" :tab="tab" />
    <div class="sep" />
    <div class="row off-soon">
      <button
        class="tb w"
        :class="{ on: tab.annotating }"
        :disabled="disabled"
        :aria-pressed="tab.annotating"
        :aria-label="t('toolbar.annotateKey')"
        :title="t('toolbar.annotateKey')"
        @click="toggleAnnotate(tab)"
      >
        <PenLine class="ic" aria-hidden="true" />{{ t("toolbar.annotate") }}
      </button>
      <button class="tb" :disabled="disabled" :aria-label="t('toolbar.organize')" :title="t('toolbar.organize')" @click="tabs.toggleOrganizing(tab, true)">
        <LayoutGrid class="ic" aria-hidden="true" />
      </button>
      <button class="tb" disabled :aria-label="t('toolbar.share')" :title="t('toolbar.comingSoon')">
        <Share class="ic" aria-hidden="true" />
      </button>
    </div>
    <AppMenu variant="more" />
  </div>
</template>

<style scoped>
.toolbar { display: flex; align-items: center; gap: 4px; height: 46px; padding: 0 10px; background: var(--chrome); border-bottom: 1px solid var(--line); flex: none; position: relative; z-index: 5; min-width: 0; }
.row { display: flex; align-items: center; gap: 4px; }
.row.off { opacity: .38; }
.pgnav { display: flex; align-items: center; gap: 4px; color: var(--text-2); font-size: 12.5px; font-variant-numeric: tabular-nums; white-space: nowrap; }
.pgin { width: 38px; height: 26px; border-radius: 6px; border: 0; box-shadow: inset 0 0 0 1px var(--line-2); background: var(--surface); color: var(--text); text-align: center; font: inherit; font-size: 12.5px; padding: 0; }
.pgin:focus { box-shadow: inset 0 0 0 1px var(--accent), var(--ring); }
.muted { color: var(--text-2); font-size: 12.5px; }
.zoomsel { display: flex; align-items: center; gap: 4px; height: 28px; padding: 0 6px 0 10px; border-radius: 7px; background: var(--hover); color: var(--text); font: inherit; font-size: 12.5px; font-weight: 500; border: 0; font-variant-numeric: tabular-nums; white-space: nowrap; min-width: 72px; justify-content: space-between; }
.zoomsel:hover, .zoomsel.open { background: var(--press); }
.fit { margin-left: 6px; }
.mi.sel .ck { color: var(--accent-text); }
.off-soon .tb:disabled:not(.w) { opacity: .38; pointer-events: auto; }
.off-soon .tb:disabled:hover { background: transparent; color: var(--text-2); }
@media (max-width: 1180px) { .fit, .fit + .sep { display: none; } }
@media (max-width: 1020px) { .off-soon { display: none; } }
</style>
