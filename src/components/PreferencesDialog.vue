<script setup lang="ts">
// Préférences (pas de planche dédiée : composée avec les composants et tokens du design).
import { Check, X } from "lucide-vue-next";
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { TrustedRoot } from "../bindings";
import { commands, inTauri, unwrap } from "../lib/api";
import { useTabs } from "../stores/tabs";
import Modal from "./Modal.vue";
import { ACCENTS, useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";

const { t, locale } = useI18n();
const settings = useSettings();
const ui = useUi();
const tabs = useTabs();

// Autorités de signature approuvées depuis le panneau « Signature numérique ».
const trusted = ref<TrustedRoot[]>([]);
const lists = ref<{ generated: number; eu: number; microsoft: number } | null>(null);
onMounted(async () => {
  if (!inTauri && !("__LUNAS_PDF_E2E__" in window)) return;
  trusted.value = await commands.listTrustedRoots();
  lists.value = await commands.trustListsInfo();
});
async function untrust(r: TrustedRoot) {
  await unwrap(commands.removeTrustedRoot(r.id));
  trusted.value = trusted.value.filter((x) => x.id !== r.id);
  await tabs.reloadAllSignatures();
}
</script>

<template>
  <Modal :title="t('prefs.title')" :close-label="t('prefs.close')" @close="ui.prefsOpen = false">
    <div class="field">
      <span class="lbl" id="p-theme">{{ t("prefs.theme") }}</span>
      <div class="seg" role="radiogroup" aria-labelledby="p-theme">
        <button v-for="v in (['system', 'light', 'dark'] as const)" :key="v" role="radio" :aria-checked="settings.settings.theme === v" :class="{ on: settings.settings.theme === v }" @click="settings.update({ theme: v })">
          {{ t(`prefs.theme${v[0].toUpperCase()}${v.slice(1)}`) }}
        </button>
      </div>
    </div>
    <div class="field">
      <span class="lbl" id="p-accent">{{ t("prefs.accent") }}</span>
      <div class="swatches" role="radiogroup" aria-labelledby="p-accent">
        <button
          v-for="c in ACCENTS"
          :key="c"
          class="sw"
          role="radio"
          :aria-checked="settings.settings.accent === c"
          :aria-label="t('prefs.accentNamed', { c })"
          :style="{ background: c }"
          @click="settings.update({ accent: c })"
        >
          <Check v-if="settings.settings.accent === c" class="ic xs" aria-hidden="true" />
        </button>
        <label class="sw custom" :class="{ sel: !ACCENTS.includes(settings.settings.accent) }" :title="t('prefs.customColor')" :style="{ background: ACCENTS.includes(settings.settings.accent) ? undefined : settings.settings.accent }">
          <input type="color" :value="settings.settings.accent" :aria-label="t('prefs.customColor')" @change="settings.update({ accent: ($event.target as HTMLInputElement).value })" />
        </label>
      </div>
    </div>
    <div class="field">
      <label class="lbl" for="p-lang">{{ t("prefs.language") }}</label>
      <select id="p-lang" class="input" :value="settings.settings.language" @change="settings.update({ language: ($event.target as HTMLSelectElement).value as 'system' | 'fr' | 'en' })">
        <option value="system">{{ t("prefs.langSystem") }}</option>
        <option value="fr">Français</option>
        <option value="en">English</option>
      </select>
    </div>
    <div class="field">
      <label class="lbl" for="p-author">{{ t("prefs.authorName") }}</label>
      <input
        id="p-author"
        class="input"
        :value="settings.settings.authorName"
        :placeholder="t('prefs.authorPlaceholder')"
        @change="settings.update({ authorName: ($event.target as HTMLInputElement).value.trim() })"
      />
    </div>
    <label class="field check">
      <input type="checkbox" class="sr-only" :checked="settings.settings.windowControls" @change="settings.update({ windowControls: ($event.target as HTMLInputElement).checked })" />
      <span class="cb" :class="{ on: settings.settings.windowControls }" aria-hidden="true">
        <Check v-if="settings.settings.windowControls" class="ic xs" />
      </span>
      <span>
        <span class="ttl">{{ t("prefs.windowControls") }}</span>
        <span class="hint">{{ t("prefs.windowControlsHint") }}</span>
      </span>
    </label>
    <div v-if="lists" class="field">
      <span>
        <span class="ttl">{{ t("prefs.trustLists") }}</span>
        <span class="hint">{{ t("prefs.trustListsHint", { eu: lists.eu, ms: lists.microsoft, date: new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(lists.generated) }) }}</span>
      </span>
    </div>
    <div v-if="trusted.length" class="field trusted">
      <span>
        <span class="ttl">{{ t("prefs.trustedRoots") }}</span>
        <span class="hint">{{ t("prefs.trustedRootsHint") }}</span>
      </span>
      <ul>
        <li v-for="r in trusted" :key="r.id">
          <span class="nm" :title="r.id">{{ r.name }}</span>
          <button class="cl" :aria-label="t('prefs.untrust', { name: r.name })" :title="t('prefs.untrust', { name: r.name })" @click="untrust(r)"><X class="ic xs" aria-hidden="true" /></button>
        </li>
      </ul>
    </div>
  </Modal>
</template>

<style scoped>
.field { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 10px 0; border-top: 1px solid var(--line); }
.field:first-of-type { border-top: 0; }
.lbl { font-weight: 500; }
.swatches { display: flex; gap: 8px; }
.sw { position: relative; width: 24px; height: 24px; border-radius: 50%; border: 0; padding: 0; display: grid; place-items: center; color: #fff; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .12); }
.sw:focus-visible, .custom:focus-within { box-shadow: var(--ring); }
.custom { background: conic-gradient(#f03e3e, #fab005, #40c057, #228be6, #be4bdb, #f03e3e); overflow: hidden; }
.custom.sel { box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--text-3); }
.custom input { position: absolute; inset: 0; opacity: 0; width: 100%; height: 100%; cursor: pointer; }
select.input { min-width: 180px; }
.check { justify-content: flex-start; align-items: flex-start; }
.check .cb { margin-top: 1px; }
.check:focus-within .cb { box-shadow: inset 0 0 0 1.5px var(--line-2), var(--ring); }
.ttl { display: block; font-weight: 500; }
.hint { display: block; color: var(--text-2); font-size: 12px; margin-top: 2px; }
.trusted { flex-direction: column; align-items: stretch; gap: 8px; }
.trusted ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; }
.trusted li { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 4px 4px 10px; border-radius: 7px; background: var(--hover); }
.trusted .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
