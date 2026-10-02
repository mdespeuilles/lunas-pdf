<script setup lang="ts">
// Bandeau d'un document à formulaire (planche 04) : surlignage des champs, effacement.
import { TextCursorInput, TriangleAlert, X } from "lucide-vue-next";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useForm } from "../composables/form";
import { useSettings } from "../stores/settings";
import type { DocTab } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const settings = useSettings();
const form = useForm();
const customScripts = computed(() => (props.tab.edit?.fields ?? []).some((f) => f.customScript));
</script>

<template>
  <div class="banner" role="region" :aria-label="t('form.region')">
    <TextCursorInput class="ic bi" aria-hidden="true" />
    <span><b>{{ t("form.title") }}</b> {{ t("form.body") }}</span>
    <div class="grow" />
    <span v-if="customScripts" class="scripts" :title="t('form.customScriptsHint')">
      <TriangleAlert class="ic xs" aria-hidden="true" />{{ t("form.customScripts") }}
    </span>
    <button
      class="sw"
      role="switch"
      :aria-checked="settings.settings.highlightFields"
      @click="settings.update({ highlightFields: !settings.settings.highlightFields })"
    >
      <span class="sw-t" :class="{ on: settings.settings.highlightFields }" aria-hidden="true" />{{ t("form.highlight") }}
    </button>
    <button class="btn gh" :disabled="!form.canReset(tab)" @click="form.reset(tab)">{{ t("form.reset") }}</button>
    <button class="cl" style="width: 26px; height: 26px" :aria-label="t('form.hide')" @click="tab.formBannerHidden = true">
      <X class="ic s" aria-hidden="true" />
    </button>
  </div>
</template>

<style scoped>
.bi { color: var(--accent-text); }
.sw { display: flex; align-items: center; gap: 8px; height: 26px; padding: 0 6px; border: 0; border-radius: 7px; background: transparent; color: var(--text); font: inherit; font-size: 12.5px; font-weight: 500; }
.sw:focus-visible { box-shadow: var(--ring); }
.sw-t { position: relative; width: 30px; height: 18px; border-radius: 9px; background: var(--line-2); flex: none; transition: background .15s; }
.sw-t::after { content: ""; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; box-shadow: 0 1px 2px rgba(0, 0, 0, .25); transition: left .15s; }
.sw-t.on { background: var(--accent); }
.sw-t.on::after { left: 14px; }
.btn:disabled { opacity: .5; }
.scripts { display: flex; align-items: center; gap: 6px; color: var(--text-2); }
.scripts .ic { color: var(--warn); }
</style>
