<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import Modal from "./Modal.vue";
import { inTauri } from "../lib/api";
import { useUi } from "../stores/ui";

const { t } = useI18n();
const ui = useUi();
const version = ref("0.1.0");
onMounted(async () => {
  if (inTauri) version.value = await (await import("@tauri-apps/api/app")).getVersion();
});
</script>

<template>
  <Modal :title="t('menu.about')" :close-label="t('prefs.close')" :width="360" @close="ui.aboutOpen = false">
    <div class="about">
      <img src="/app-icon.svg" alt="" width="64" height="64" />
      <strong>Feuillet</strong>
      <span>{{ t("about.version", { v: version }) }}</span>
      <p>{{ t("about.text") }}</p>
      <span class="dim">{{ t("about.engine") }}</span>
    </div>
  </Modal>
</template>

<style scoped>
.about { display: flex; flex-direction: column; align-items: center; gap: 4px; text-align: center; }
strong { font-size: 17px; margin-top: 8px; }
p { margin: 8px 0; color: var(--text-2); }
.dim { color: var(--text-3); font-size: 12px; }
</style>
