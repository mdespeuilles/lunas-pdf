<script setup lang="ts">
// Barre de titre personnalisée (planches 01/02) : onglets, région de déplacement, boutons de fenêtre.
import { FileText, Home, Lock, Maximize2, Minus, Plus, Square, X } from "lucide-vue-next";
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import AppMenu from "./AppMenu.vue";
import { appWindow } from "../lib/window";
import { useSettings } from "../stores/settings";
import { useTabs } from "../stores/tabs";
import { openFromDialog } from "../composables/open";
import { confirmQuit, requestClose } from "../composables/save";
import { pageDrag } from "../composables/page-drag";

const { t } = useI18n();
const tabs = useTabs();
const settings = useSettings();
const maximized = ref(false);

onMounted(async () => {
  maximized.value = await appWindow.isMaximized();
  await appWindow.onResized(async () => (maximized.value = await appWindow.isMaximized()));
});

/** Fermeture de la fenêtre : propose d'enregistrer les documents modifiés. */
async function quit() {
  if (await confirmQuit()) await appWindow.destroy();
}

function onTabMouseDown(e: MouseEvent, key: string) {
  // Clic du milieu : fermer l'onglet.
  if (e.button === 1) {
    e.preventDefault();
    const tab = tabs.tabs.find((x) => x.key === key);
    if (tab) void requestClose(tab);
  }
}
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <div class="tabs" role="tablist" :aria-label="t('titlebar.home')">
      <div v-if="!tabs.tabs.length" class="tab on" role="tab" aria-selected="true">
        <Home class="ic s" aria-hidden="true" />
        <span class="nm">{{ t("titlebar.home") }}</span>
      </div>
      <div
        v-for="tab in tabs.tabs"
        :key="tab.key"
        class="tab"
        :class="{ on: tab.key === tabs.activeKey, spring: pageDrag?.spring === tab.key }"
        :data-tab-key="tab.key"
        role="tab"
        :aria-selected="tab.key === tabs.activeKey"
        :title="tab.path"
        tabindex="0"
        @click="tabs.activeKey = tab.key"
        @keydown.enter="tabs.activeKey = tab.key"
        @mousedown="onTabMouseDown($event, tab.key)"
      >
        <Lock v-if="tab.status === 'locked' || tab.info?.encrypted" class="ic s" aria-hidden="true" />
        <FileText v-else class="ic s" aria-hidden="true" />
        <span class="nm">{{ tab.name }}</span>
        <span v-if="tab.dirty" class="dirty" :aria-label="t('titlebar.modified')" />
        <button class="cl" :aria-label="tab.dirty ? `${t('titlebar.modified')} — ${t('titlebar.closeTab')}` : t('titlebar.closeTab')" @click.stop="requestClose(tab)">
          <X class="ic xs" aria-hidden="true" />
        </button>
      </div>
      <button class="cl add" :aria-label="t('titlebar.openDoc')" :title="t('titlebar.openDoc')" @click="openFromDialog()">
        <Plus class="ic s" aria-hidden="true" />
      </button>
    </div>
    <div class="drag" data-tauri-drag-region />
    <AppMenu v-if="!tabs.tabs.length" variant="main" />
    <div v-if="settings.settings.windowControls" class="wctl">
      <button class="wbtn" :aria-label="t('titlebar.minimize')" :title="t('titlebar.minimize')" @click="appWindow.minimize()">
        <Minus class="ic xs" aria-hidden="true" />
      </button>
      <button
        class="wbtn"
        :aria-label="maximized ? t('titlebar.restore') : t('titlebar.maximize')"
        :title="maximized ? t('titlebar.restore') : t('titlebar.maximize')"
        @click="appWindow.toggleMaximize()"
      >
        <Maximize2 v-if="maximized" class="ic xs" aria-hidden="true" />
        <Square v-else class="ic xs" aria-hidden="true" />
      </button>
      <button class="wbtn close" :aria-label="t('titlebar.closeWindow')" :title="t('titlebar.closeWindow')" @click="quit">
        <X class="ic xs" aria-hidden="true" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar { display: flex; align-items: center; gap: 4px; height: 42px; padding: 0 10px; background: var(--chrome); flex: none; }
.tabs { display: flex; align-items: center; gap: 4px; min-width: 0; flex: 0 1 auto; overflow: hidden; }
.tab { display: flex; align-items: center; gap: 8px; height: 30px; width: 210px; min-width: 96px; flex: 0 1 210px; padding: 0 5px 0 10px; border-radius: 8px; color: var(--text-2); font-size: 12.5px; }
.tab:hover:not(.on) { background: var(--hover); }
.tab .nm { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tab.on { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--line), 0 1px 2px rgba(0, 0, 0, .06); }
:root.dark .tab.on { background: var(--surface-2); }
/* Onglet survolé pendant un glisser de pages : il s'affiche après un court délai. */
.tab.spring { box-shadow: 0 0 0 2px var(--accent); }
.tab:focus-visible { box-shadow: var(--ring); }
.dirty { width: 8px; height: 8px; border-radius: 50%; background: var(--text-2); flex: none; }
.tab .dirty + .cl { display: none; }
.tab:hover .dirty { display: none; }
.tab:hover .dirty + .cl { display: grid; }
.add { width: 30px; height: 30px; border-radius: 8px; }
.drag { flex: 1; align-self: stretch; }
.wctl { display: flex; gap: 10px; padding-left: 8px; }
.wbtn { width: 24px; height: 24px; border-radius: 50%; display: grid; place-items: center; background: var(--hover); color: var(--text-2); border: 0; padding: 0; }
.wbtn:hover { background: var(--press); color: var(--text); }
.wbtn.close:hover { background: var(--bad); color: #fff; }
</style>
