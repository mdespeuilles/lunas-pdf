<script setup lang="ts">
// Pastille « Champ n sur N » (planche 04) : navigation entre les champs, Tab / Maj Tab.
import { ChevronLeft, ChevronRight } from "lucide-vue-next";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { stops, useForm } from "../composables/form";
import type { DocTab } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const form = useForm();
const list = computed(() => stops(props.tab.edit?.fields ?? []));
const current = computed(() => list.value.findIndex((s) => s.field.id === props.tab.focusedField));
</script>

<template>
  <div class="fnav" role="navigation" :aria-label="t('form.nav')">
    <button :aria-label="t('form.prev')" :title="t('form.prev')" @pointerdown.prevent @click="form.step(tab, -1)">
      <ChevronLeft class="ic xs" aria-hidden="true" />
    </button>
    <span class="t" aria-live="polite">{{ t("form.field", { n: current >= 0 ? current + 1 : "–" }) }} <span class="dim">{{ t("form.of", { n: list.length }) }}</span></span>
    <button :aria-label="t('form.next')" :title="t('form.next')" @pointerdown.prevent @click="form.step(tab, 1)">
      <ChevronRight class="ic xs" aria-hidden="true" />
    </button>
    <span class="vsep" />
    <span class="dim keys"><span class="kbd">Tab</span>{{ t("form.nextShort") }}<span class="kbd" style="margin-left: 6px">{{ t("form.shiftTab") }}</span>{{ t("form.prevShort") }}</span>
  </div>
</template>

<style scoped>
.fnav { position: absolute; left: 50%; bottom: 18px; transform: translateX(-50%); display: flex; align-items: center; gap: 6px; height: 34px; padding: 0 6px; border-radius: 17px; background: rgba(28, 28, 33, .84); color: #f2f2f5; font-size: 12px; font-variant-numeric: tabular-nums; backdrop-filter: blur(10px); box-shadow: 0 4px 16px rgba(0, 0, 0, .22); z-index: 6; white-space: nowrap; }
.fnav button { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 12px; border: 0; background: transparent; color: #e6e6ea; padding: 0; }
.fnav button:hover { background: rgba(255, 255, 255, .14); }
.fnav button:focus-visible { box-shadow: var(--ring); }
.t { padding: 0 4px; }
.dim { color: #b8b8c2; }
.vsep { width: 1px; height: 16px; background: rgba(255, 255, 255, .18); }
.keys { display: flex; align-items: center; gap: 5px; padding-right: 8px; }
.kbd { background: rgba(255, 255, 255, .14); box-shadow: none; color: #e4e4ea; }
@media (max-width: 720px) { .vsep, .keys { display: none; } }
</style>
