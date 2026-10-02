<script setup lang="ts">
// Nœud du sommaire (arbre accessible, dépliable).
import { ChevronRight } from "lucide-vue-next";
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import type { OutlineItem } from "../bindings";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ item: OutlineItem; tab: DocTab; depth: number }>();
const { t } = useI18n();
const tabs = useTabs();
const open = ref(props.depth === 0 && props.item.children.length <= 12);

function go() {
  if (props.item.page !== null) tabs.goto(props.tab, props.item.page);
}
</script>

<template>
  <li role="treeitem" :aria-expanded="item.children.length ? open : undefined">
    <div class="node" :style="{ paddingLeft: 4 + depth * 14 + 'px' }">
      <button v-if="item.children.length" class="tw" :class="{ open }" :aria-label="open ? t('sidebar.collapse') : t('sidebar.expand')" @click="open = !open">
        <ChevronRight class="ic xs" aria-hidden="true" />
      </button>
      <span v-else class="tw" />
      <button class="lbl" :disabled="item.page === null" :class="{ cur: item.page === tab.page }" @click="go">
        <span class="title">{{ item.title }}</span>
        <span v-if="item.page !== null" class="pg">{{ tab.info?.pages[item.page]?.label ?? item.page + 1 }}</span>
      </button>
    </div>
    <ul v-if="open && item.children.length" role="group">
      <OutlineNode v-for="(c, i) in item.children" :key="i" :item="c" :tab="tab" :depth="depth + 1" />
    </ul>
  </li>
</template>

<style scoped>
ul { list-style: none; margin: 0; padding: 0; }
.node { display: flex; align-items: center; gap: 2px; }
.tw { width: 20px; height: 20px; flex: none; display: grid; place-items: center; border: 0; background: transparent; color: var(--text-3); padding: 0; border-radius: 5px; }
button.tw:hover { background: var(--hover); color: var(--text); }
.tw.open :deep(svg) { transform: rotate(90deg); }
.tw :deep(svg) { transition: transform .12s; }
.lbl { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 8px; padding: 5px 6px; border: 0; background: transparent; border-radius: 6px; text-align: left; font-size: 12.5px; color: var(--text); }
.lbl:hover:not(:disabled) { background: var(--hover); }
.lbl:disabled { color: var(--text-3); }
.lbl.cur .title { color: var(--accent-text); font-weight: 500; }
.title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pg { color: var(--text-3); font-size: 11.5px; font-variant-numeric: tabular-nums; }
</style>
