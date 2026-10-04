<script setup lang="ts">
// Menu principal (☰ sur l'accueil, ⋯ dans la barre d'outils). Sans raccourcis affichés : leur
// notation diffère selon le système (Ctrl / ⌘).
import { Menu, MoreHorizontal } from "lucide-vue-next";
import { useI18n } from "vue-i18n";
import Dropdown from "./Dropdown.vue";
import { useUi } from "../stores/ui";
import { useTabs } from "../stores/tabs";
import { openFromDialog } from "../composables/open";
import { printTab } from "../composables/print";
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
      {{ t("menu.open") }}
    </button>
    <template v-if="tabs.active?.status === 'ready'">
      <button class="mi" role="menuitem" :disabled="!tabs.active.dirty" @click="saveTab(tabs.active)">
        {{ t("menu.save") }}
      </button>
      <button class="mi" role="menuitem" @click="saveTab(tabs.active, true)">
        {{ t("menu.saveAs") }}
      </button>
      <button class="mi" role="menuitem" @click="ui.exportTab = tabs.active.key">
        {{ t("menu.export") }}
      </button>
      <button class="mi" role="menuitem" @click="printTab(tabs.active)">
        {{ t("menu.print") }}
      </button>
    </template>
    <button v-if="tabs.active" class="mi" role="menuitem" @click="requestClose(tabs.active)">
      {{ t("menu.closeTab") }}
    </button>
    <div class="msep" />
    <button class="mi" role="menuitem" @click="ui.prefsOpen = true">
      {{ t("menu.preferences") }}
    </button>
    <button class="mi" role="menuitem" @click="ui.aboutOpen = true">{{ t("menu.about") }}</button>
  </Dropdown>
</template>

<style scoped>
.menu-btn { height: 30px; padding: 0 10px; }
</style>
