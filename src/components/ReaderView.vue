<script setup lang="ts">
// Vue lecture d'un onglet : barre d'outils, barre latérale, document, états verrouillé/erreur.
import { FileWarning, TriangleAlert, X } from "lucide-vue-next";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import AnnotBar from "./AnnotBar.vue";
import DocViewport from "./DocViewport.vue";
import Sidebar from "./Sidebar.vue";
import Toolbar from "./Toolbar.vue";
import UnlockCard from "./UnlockCard.vue";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const tabs = useTabs();
const toolbar = ref<InstanceType<typeof Toolbar>>();
const xfaDismissed = ref(false);

const errorText = computed(() => {
  const e = props.tab.error;
  if (!e) return "";
  if (e.kind === "notFound") return t("errors.notFound");
  if (e.kind === "invalid") return t("errors.invalid");
  return t("errors.engine", { msg: "message" in e ? e.message : e.kind });
});

defineExpose({
  focusSearch: () => toolbar.value?.focusSearch(),
  toggleCase: () => toolbar.value?.toggleCase(),
  focusPage: () => toolbar.value?.focusPage(),
});
</script>

<template>
  <section class="reader">
    <Toolbar ref="toolbar" :tab="tab" :disabled="tab.status !== 'ready'" />
    <AnnotBar v-if="tab.annotating && tab.status === 'ready'" :tab="tab" />
    <div v-if="tab.status === 'ready' && tab.info?.form === 'xfa' && !xfaDismissed" class="banner warn" role="status">
      <TriangleAlert class="ic bi" aria-hidden="true" />
      <span>{{ t("errors.xfa") }}</span>
      <div class="grow" />
      <button class="cl" style="width: 26px; height: 26px" :aria-label="t('errors.dismiss')" @click="xfaDismissed = true">
        <X class="ic s" aria-hidden="true" />
      </button>
    </div>
    <div class="body">
      <template v-if="tab.status === 'ready' && tab.info">
        <Sidebar v-if="tab.sidebarOpen" :tab="tab" />
        <DocViewport :tab="tab" />
      </template>
      <UnlockCard v-else-if="tab.status === 'locked'" :key="tab.key" :tab="tab" />
      <div v-else-if="tab.status === 'error'" class="center">
        <div class="card" role="alert">
          <span class="lic bad"><FileWarning class="ic l" aria-hidden="true" /></span>
          <h2>{{ t("errors.title", { name: tab.name }) }}</h2>
          <p>{{ errorText }}</p>
          <button class="btn" @click="tabs.close(tab.key)">{{ t("unlock.closeTab") }}</button>
        </div>
      </div>
      <div v-else class="center loading" aria-busy="true" />
    </div>
  </section>
</template>

<style scoped>
.reader { flex: 1; display: flex; flex-direction: column; min-height: 0; }
.body { flex: 1; display: flex; min-height: 0; position: relative; }
.center { flex: 1; display: flex; align-items: center; justify-content: center; background: var(--canvas); padding: 16px; }
.card { width: 420px; max-width: 100%; padding: 30px; border-radius: 16px; background: var(--surface); box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); display: flex; flex-direction: column; align-items: center; text-align: center; gap: 8px; }
.card h2 { margin: 0; font-size: 17px; font-weight: 600; overflow-wrap: anywhere; }
.card p { margin: 0 0 12px; color: var(--text-2); line-height: 1.5; }
.lic { display: grid; place-items: center; width: 56px; height: 56px; border-radius: 16px; margin-bottom: 8px; }
.lic.bad { background: var(--bad-bg); color: var(--bad); }
.loading::after { content: ""; width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--line-2); border-top-color: var(--accent); animation: spin .8s linear infinite; opacity: 0; animation-delay: .3s; animation-fill-mode: forwards; }
@keyframes spin { from { transform: rotate(0); opacity: 1; } to { transform: rotate(360deg); opacity: 1; } }
</style>
