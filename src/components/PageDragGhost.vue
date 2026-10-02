<script setup lang="ts">
// Aperçu d'une page glissée depuis la barre latérale, avec l'action qui sera faite au dépôt.
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ThumbCanvas from "./ThumbCanvas.vue";
import { pageDrag } from "../composables/page-drag";
import { useTabs } from "../stores/tabs";

const { t } = useI18n();
const tabs = useTabs();
const d = computed(() => pageDrag.value);
const geom = computed(() => (d.value?.src.info ? d.value.src.info.pages[d.value.pages[0]] : null));
const pill = computed(() => {
  const s = d.value;
  if (!s) return "";
  if (s.spring && s.spring !== tabs.activeKey) return t("pageDrag.openTab", { name: tabs.tabs.find((x) => x.key === s.spring)?.name ?? "" });
  if (!s.target) return "";
  const copy = s.target.tab.key !== s.src.key;
  const n = s.target.tab.info?.pages.length ?? 0;
  if (s.target.at >= n) return t(copy ? "organize.copyEnd" : "organize.moveEnd");
  return t(copy ? "organize.copyBefore" : "organize.moveBefore", { n: s.target.at + 1 });
});
</script>

<template>
  <div v-if="d && geom && d.src.info" class="pghost" :style="{ left: d.x + 12 + 'px', top: d.y + 12 + 'px' }" aria-hidden="true">
    <div class="gp">
      <ThumbCanvas :doc="d.src.info.id" :page="d.pages[0]" :width="64" :height="Math.round((64 * geom.height) / geom.width)" :rev="d.src.pageRev[d.pages[0]] ?? 0" />
    </div>
    <span v-if="pill" class="dpill">{{ pill }}</span>
  </div>
</template>

<style scoped>
.pghost { position: fixed; z-index: 60; pointer-events: none; }
.gp { background: #fff; border-radius: 3px; overflow: hidden; box-shadow: 0 0 0 1px rgba(0, 0, 0, .1), 0 14px 32px rgba(0, 0, 0, .28); transform: rotate(-3deg); width: 64px; }
.dpill { display: inline-block; margin-top: 8px; padding: 5px 10px; border-radius: 8px; background: var(--tip-bg); color: var(--tip-fg); font-size: 12px; white-space: nowrap; box-shadow: 0 6px 18px rgba(0, 0, 0, .25); }
</style>
