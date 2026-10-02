<script setup lang="ts">
// Menu principal (☰ sur l'accueil, ⋯ dans la barre d'outils).
import { Menu, MoreHorizontal } from "lucide-vue-next";
import { useI18n } from "vue-i18n";
import Dropdown from "./Dropdown.vue";
import { useUi } from "../stores/ui";
import { useTabs } from "../stores/tabs";
import { openFromDialog } from "../composables/open";
import { requestClose, saveTab } from "../composables/save";

defineProps<{ variant: "main" | "more" }>();
const { t } = useI18n();
const ui = useUi();
const tabs = useTabs();
</script>

<template>
  <Dropdown align="right" :width="240">
    <template #trigger="{ open, toggle }">
      <button
        v-if="variant === 'main'"
        class="btn gh menu-btn"
        :aria-label="t('titlebar.mainMenu')"
        aria-haspopup="menu"
        :aria-expanded="open"
        @click="toggle()"
      >
        <Menu class="ic" aria-hidden="true" />
      </button>
      <button v-else class="tb" :class="{ on: open }" :aria-label="t('toolbar.more')" aria-haspopup="menu" :aria-expanded="open" @click="toggle()">
        <MoreHorizontal class="ic" aria-hidden="true" />
      </button>
    </template>
    <button class="mi" role="menuitem" @click="openFromDialog()">
      <span class="ck" />{{ t("menu.open") }}<span class="kbd">Ctrl O</span>
    </button>
    <template v-if="tabs.active?.status === 'ready'">
      <button class="mi" role="menuitem" :disabled="!tabs.active.dirty" @click="saveTab(tabs.active)">
        <span class="ck" />{{ t("menu.save") }}<span class="kbd">Ctrl S</span>
      </button>
      <button class="mi" role="menuitem" @click="saveTab(tabs.active, true)">
        <span class="ck" />{{ t("menu.saveAs") }}<span class="kbd">Ctrl Maj S</span>
      </button>
    </template>
    <button v-if="tabs.active" class="mi" role="menuitem" @click="requestClose(tabs.active)">
      <span class="ck" />{{ t("menu.closeTab") }}<span class="kbd">Ctrl W</span>
    </button>
    <div class="msep" />
    <button class="mi" role="menuitem" @click="ui.prefsOpen = true">
      <span class="ck" />{{ t("menu.preferences") }}<span class="kbd">Ctrl ,</span>
    </button>
    <button class="mi" role="menuitem" @click="ui.aboutOpen = true"><span class="ck" />{{ t("menu.about") }}</button>
  </Dropdown>
</template>

<style scoped>
.menu-btn { height: 30px; padding: 0 10px; }
</style>
