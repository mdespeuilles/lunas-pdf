<script setup lang="ts">
// Nouvelle version disponible (repris de Lunas Mail) : bandeau discret en bas à droite, avec
// les nouveautés (section du CHANGELOG reprise dans latest.json).
import { computed, ref } from "vue";
import { storeToRefs } from "pinia";
import { useI18n } from "vue-i18n";
import { renderMarkdown } from "../lib/markdown";
import { useUpdates } from "../stores/updates";

const { t } = useI18n();
const updates = useUpdates();
const { available, dismissed, installing, progress, error } = storeToRefs(updates);
const notesOpen = ref(false);
const notes = computed(() => (available.value?.body ?? "").trim());
</script>

<template>
  <div v-if="available && !dismissed" class="update" role="status">
    <span class="text">
      <strong>{{ t("updates.available", { version: available.version }) }}</strong>
      <span v-if="installing" class="sub">{{ progress !== null ? t("updates.downloading", { n: progress }) : t("updates.installing") }}</span>
      <span v-else-if="error" class="sub err">{{ error }}</span>
      <button v-if="notes && !installing" class="link" @click="notesOpen = !notesOpen">{{ notesOpen ? t("updates.hideNotes") : t("updates.showNotes") }}</button>
      <!-- eslint-disable-next-line vue/no-v-html -- HTML échappé par renderMarkdown -->
      <span v-if="notesOpen && notes" class="notes" v-html="renderMarkdown(notes)" />
    </span>
    <template v-if="!installing">
      <button class="btn gh" @click="dismissed = true">{{ t("updates.later") }}</button>
      <button class="btn pri" @click="updates.install()">{{ t("updates.install") }}</button>
    </template>
    <span v-else class="spin" aria-hidden="true" />
  </div>
</template>

<style scoped>
.update { position: fixed; right: 16px; bottom: 16px; z-index: 90; display: flex; align-items: flex-start; gap: 10px; max-width: 480px; padding: 12px 14px; background: var(--surface); border-radius: 12px; box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); font-size: 13px; }
.text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
.sub { font-size: 12px; color: var(--text-3); }
.err { color: var(--bad); overflow-wrap: anywhere; user-select: text; }
.link { align-self: flex-start; border: 0; padding: 0; background: none; color: var(--accent-text); font: inherit; font-size: 12px; text-decoration: underline; }
.notes { max-height: 220px; overflow: auto; font-size: 12.5px; color: var(--text-2); user-select: text; }
.notes :deep(ul) { margin: 4px 0 0; padding-left: 16px; }
.notes :deep(p) { margin: 4px 0; }
.spin { width: 16px; height: 16px; margin-top: 2px; border-radius: 50%; border: 2px solid var(--line-2); border-top-color: var(--accent); animation: spin .8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
