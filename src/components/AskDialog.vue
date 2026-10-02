<script setup lang="ts">
// Dialogues de confirmation : modifications non enregistrées (planche 12), caviardage,
// modification d'un document signé (planche 06).
import { Check, ShieldAlert } from "lucide-vue-next";
import { computed, nextTick, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";

const { t } = useI18n();
const ui = useUi();
const tabs = useTabs();
const ask = computed(() => ui.ask!);
const tab = computed(() => tabs.tabs.find((x) => x.key === ask.value.tabKey));
const primary = ref<HTMLButtonElement>();
const n = computed(() => tab.value?.edit?.unsavedCount ?? 1);
const signed = computed(() => ask.value.kind === "signedAnnotate" || ask.value.kind === "signedFill");
const verb = computed(() => (ask.value.kind === "signedFill" ? "Fill" : "Annotate"));
const signer = computed(() => {
  const names = [...new Set((tab.value?.signatures ?? []).map((s) => s.signer).filter(Boolean))] as string[];
  return names.length ? names.join(", ") : t("sig.unknownSigner");
});

onMounted(() => void nextTick(() => primary.value?.focus()));

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    ask.value.resolve("cancel");
  }
}
</script>

<template>
  <div class="scrim" @mousedown.self="ask.resolve('cancel')">
    <div class="modal" role="alertdialog" aria-modal="true" aria-labelledby="ask-title" aria-describedby="ask-body" @keydown="onKey">
      <div class="mbody">
        <div v-if="signed" class="srow">
          <span class="wic"><ShieldAlert class="ic" aria-hidden="true" /></span>
          <div class="scol">
            <h3 id="ask-title">{{ t(`save.signed${verb}Title`) }}</h3>
            <p id="ask-body">{{ t("save.signedBody", { signer, name: tab?.name ?? "" }) }}</p>
            <label class="rem">
              <button class="cb" role="checkbox" :aria-checked="ui.askRemember" :class="{ on: ui.askRemember }" @click="ui.askRemember = !ui.askRemember">
                <Check v-if="ui.askRemember" class="ic xs" aria-hidden="true" />
              </button>
              <span @click="ui.askRemember = !ui.askRemember">{{ t("save.signedRemember") }}</span>
            </label>
          </div>
        </div>
        <template v-else-if="ask.kind === 'unsaved'">
          <h3 id="ask-title">{{ t("save.unsavedTitle") }}</h3>
          <p id="ask-body">{{ t("save.unsavedBody", { name: tab?.name ?? "", n }, n) }}</p>
        </template>
        <template v-else>
          <h3 id="ask-title">{{ t("save.redactTitle") }}</h3>
          <p id="ask-body">{{ t("save.redactBody") }}</p>
        </template>
      </div>
      <div class="mft">
        <template v-if="signed">
          <button class="btn gh" style="margin-right: auto" @click="ask.resolve('cancel')">{{ t("save.cancel") }}</button>
          <button class="btn out" @click="ask.resolve('confirm')">{{ t(`save.signed${verb}Anyway`) }}</button>
          <button ref="primary" class="btn pri" @click="ask.resolve('copy')">{{ t(`save.signed${verb}Copy`) }}</button>
        </template>
        <template v-else-if="ask.kind === 'unsaved'">
          <button class="btn dng" @click="ask.resolve('discard')">{{ t("save.discard") }}</button>
          <button class="btn gh" @click="ask.resolve('cancel')">{{ t("save.cancel") }}</button>
          <button ref="primary" class="btn pri" @click="ask.resolve('save')">{{ t("save.save") }}<span class="kbd">Ctrl S</span></button>
        </template>
        <template v-else>
          <span class="grow" />
          <button class="btn gh" @click="ask.resolve('cancel')">{{ t("save.cancel") }}</button>
          <button ref="primary" class="btn pri danger" @click="ask.resolve('confirm')">{{ t("save.redactConfirm") }}</button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scrim { position: fixed; inset: 0; z-index: 110; background: var(--scrim); display: flex; align-items: center; justify-content: center; padding: 16px; }
.modal { width: 460px; max-width: 100%; background: var(--surface); border-radius: 14px; box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); display: flex; flex-direction: column; overflow: hidden; }
.mbody { display: flex; flex-direction: column; gap: 8px; padding: 22px 22px 20px; }
h3 { margin: 0; font-size: 15px; font-weight: 600; }
p { margin: 0; color: var(--text-2); font-size: 13px; line-height: 1.5; }
.mft { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--line); background: var(--surface-2); }
:root.dark .mft { background: rgba(0, 0, 0, .12); }
.btn.dng { background: transparent; color: var(--bad); margin-right: auto; }
.btn.dng:hover { background: var(--bad-bg); }
.btn.danger { background: var(--bad); }
.srow { display: flex; gap: 14px; }
.scol { display: flex; flex-direction: column; gap: 8px; }
.wic { display: grid; place-items: center; width: 36px; height: 36px; border-radius: 10px; background: var(--warn-bg); color: var(--warn); flex: none; }
.rem { display: flex; align-items: center; gap: 8px; font-size: 12.5px; color: var(--text-2); margin-top: 4px; cursor: pointer; }
.cb { display: grid; place-items: center; width: 16px; height: 16px; padding: 0; border-radius: 4px; border: 0; box-shadow: inset 0 0 0 1.5px var(--line-2); background: var(--surface); color: var(--on-accent); }
.cb.on { background: var(--accent); box-shadow: none; }
.cb:focus-visible { box-shadow: var(--ring); }
</style>
