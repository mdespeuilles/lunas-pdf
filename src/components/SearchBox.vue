<script setup lang="ts">
// Recherche (planche 09) : compact au repos, élargi pendant la saisie.
import { ChevronDown, ChevronUp, Search, X } from "lucide-vue-next";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const tabs = useTabs();
const input = ref<HTMLInputElement>();
const text = ref(props.tab.search.query);
const focused = ref(false);
const s = computed(() => props.tab.search);
const active = computed(() => focused.value || !!text.value);

let timer: ReturnType<typeof setTimeout> | undefined;
watch(text, (q) => {
  clearTimeout(timer);
  timer = setTimeout(() => tabs.search(props.tab, q), 180);
});

const count = computed(() => {
  if (!s.value.query) return "";
  if (!s.value.hits.length) return s.value.running ? t("search.searching") : t("search.none");
  return t("search.count", { i: s.value.current + 1, n: s.value.hits.length });
});

function onKey(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    clearTimeout(timer);
    if (text.value !== s.value.query) tabs.search(props.tab, text.value);
    else tabs.nextHit(props.tab, e.shiftKey ? -1 : 1);
  } else if (e.key === "Escape") {
    e.stopPropagation();
    clear();
    input.value?.blur();
  }
}

function clear() {
  clearTimeout(timer);
  text.value = "";
  tabs.search(props.tab, "");
}

function toggleCase() {
  tabs.search(props.tab, text.value, !s.value.caseSensitive);
}

defineExpose({
  focus: () => {
    input.value?.focus();
    input.value?.select();
  },
  toggleCase,
});
</script>

<template>
  <div class="search" :class="{ active }" role="search" @click="input?.focus()">
    <Search class="ic s" aria-hidden="true" />
    <input
      ref="input"
      v-model="text"
      class="q"
      type="text"
      spellcheck="false"
      :placeholder="t('search.placeholder')"
      :aria-label="t('search.label')"
      @focus="focused = true"
      @blur="focused = false"
      @keydown="onKey"
    />
    <span v-if="!active" class="kbd">Ctrl F</span>
    <template v-else>
      <span class="cnt" aria-live="polite">{{ count }}</span>
      <button class="sb" :aria-label="t('search.prev')" :title="t('search.prev')" :disabled="!s.hits.length" @mousedown.prevent @click="tabs.nextHit(tab, -1)">
        <ChevronUp class="ic s" aria-hidden="true" />
      </button>
      <button class="sb" :aria-label="t('search.next')" :title="t('search.next')" :disabled="!s.hits.length" @mousedown.prevent @click="tabs.nextHit(tab, 1)">
        <ChevronDown class="ic s" aria-hidden="true" />
      </button>
      <button class="sb" :class="{ on: s.caseSensitive }" :aria-pressed="s.caseSensitive" :aria-label="t('search.caseSensitive')" :title="t('search.caseSensitive')" @mousedown.prevent @click="toggleCase">Aa</button>
      <button class="sb" :aria-label="t('search.clear')" :title="t('search.clear')" @mousedown.prevent @click="clear">
        <X class="ic xs" aria-hidden="true" />
      </button>
    </template>
  </div>
</template>

<style scoped>
.search { display: flex; align-items: center; gap: 7px; width: 210px; height: 30px; padding: 0 6px 0 9px; border-radius: 8px; background: var(--hover); color: var(--text-3); font-size: 12.5px; flex: none; transition: width .15s; }
.search.active { gap: 6px; width: 360px; padding: 0 4px 0 9px; background: var(--surface); box-shadow: 0 0 0 1px var(--accent), var(--ring); color: var(--text-2); }
.q { flex: 1; min-width: 0; border: 0; background: transparent; color: var(--text); font: inherit; font-size: 12.5px; padding: 0; caret-color: var(--accent); }
.q::placeholder { color: var(--text-3); }
.q:focus-visible { box-shadow: none; }
.cnt { font-size: 12px; color: var(--text-3); font-variant-numeric: tabular-nums; white-space: nowrap; }
.sb { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; border: 0; background: transparent; color: var(--text-2); padding: 0; font: inherit; font-size: 11.5px; font-weight: 600; flex: none; }
.sb:hover { background: var(--hover); color: var(--text); }
.sb.on { background: var(--accent-soft); color: var(--accent-text); }
.sb:disabled { opacity: .4; }
@media (max-width: 1100px) { .search.active { width: 280px; } .search { width: 170px; } }
</style>
