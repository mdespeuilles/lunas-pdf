<script setup lang="ts">
import { Upload } from "lucide-vue-next";
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import AboutDialog from "./components/AboutDialog.vue";
import AskDialog from "./components/AskDialog.vue";
import SignatureDialog from "./components/SignatureDialog.vue";
import SignaturePicker from "./components/SignaturePicker.vue";
import { useSignatures } from "./stores/signatures";
import HomeView from "./components/HomeView.vue";
import PreferencesDialog from "./components/PreferencesDialog.vue";
import ReaderView from "./components/ReaderView.vue";
import TitleBar from "./components/TitleBar.vue";
import { events } from "./bindings";
import { commands, inTauri } from "./lib/api";
import { appWindow, onFileDrop } from "./lib/window";
import { openPaths } from "./composables/open";
import { confirmQuit } from "./composables/save";
import { useShortcuts } from "./composables/shortcuts";
import { useRecents } from "./stores/recents";
import { useSettings } from "./stores/settings";
import { useTabs } from "./stores/tabs";
import { useUi } from "./stores/ui";
import { bootLog } from "./boot";

const { t } = useI18n();
const tabs = useTabs();
const ui = useUi();
const sigs = useSignatures();
const settings = useSettings();
const recents = useRecents();
const dragging = ref(false);
const readers = ref<Record<string, InstanceType<typeof ReaderView>>>({});

useShortcuts(() => (tabs.activeKey ? readers.value[tabs.activeKey] : undefined));

onMounted(async () => {
  await settings.load();
  await appWindow.show();
  bootLog("démarrage : fenêtre affichée par l'interface");
  await appWindow.onCloseRequested(confirmQuit);
  await onFileDrop((e) => {
    if (e.type === "drop") {
      dragging.value = false;
      openPaths(e.paths);
    } else dragging.value = e.type !== "leave";
  });
  if (inTauri || "__FEUILLET_E2E__" in window) {
    await events.openFilesEvent.listen((e) => openPaths(e.payload.paths));
    // Listes de confiance rafraîchies en arrière-plan : revérification des signatures.
    await events.trustListsUpdatedEvent.listen(() => void useTabs().reloadAllSignatures());
    const pending = await commands.takePendingFiles();
    if (pending.length) openPaths(pending);
  }
});

// Les récents se mettent à jour à la fermeture du dernier onglet.
tabs.$onAction(({ name, after }) => {
  if (name === "close" || name === "openPaths") after(() => !tabs.tabs.length && recents.refresh());
});
</script>

<template>
  <div class="app">
    <TitleBar />
    <HomeView v-if="!tabs.tabs.length" :dragging="dragging" />
    <ReaderView
      v-for="tab in tabs.tabs"
      v-show="tab.key === tabs.activeKey"
      :key="tab.key"
      :ref="(el) => { if (el) readers[tab.key] = el as InstanceType<typeof ReaderView>; else delete readers[tab.key]; }"
      :tab="tab"
    />
    <div v-if="dragging && tabs.tabs.length" class="drop-over" aria-hidden="true">
      <div class="drop-card"><Upload class="ic xl" />{{ t("drop.overlay") }}</div>
    </div>
    <PreferencesDialog v-if="ui.prefsOpen" />
    <AboutDialog v-if="ui.aboutOpen" />
    <AskDialog v-if="ui.ask" />
    <SignaturePicker v-if="sigs.picker" />
    <SignatureDialog v-if="sigs.dialog" />
    <div v-if="ui.toast" :key="ui.toast.seq" class="toast" role="status">{{ ui.toast.text }}</div>
  </div>
</template>

<style scoped>
.app { height: 100%; display: flex; flex-direction: column; background: var(--bg); overflow: hidden; }
.drop-over { position: fixed; inset: 42px 0 0; z-index: 90; display: flex; align-items: center; justify-content: center; background: var(--accent-soft); border: 2px solid var(--accent); border-radius: 12px; margin: 8px; pointer-events: none; }
.drop-card { display: flex; align-items: center; gap: 12px; padding: 16px 22px; border-radius: 14px; background: var(--surface); box-shadow: var(--shadow); font-size: 15px; font-weight: 600; color: var(--accent-text); }
.toast { position: fixed; left: 50%; bottom: 64px; transform: translateX(-50%); z-index: 120; padding: 9px 14px; border-radius: 9px; background: var(--tip-bg); color: var(--tip-fg); font-size: 12.5px; box-shadow: var(--shadow); max-width: min(520px, calc(100% - 32px)); }
</style>
