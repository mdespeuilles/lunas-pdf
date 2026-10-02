<script setup lang="ts">
// Bandeau de signature (planches 05 et 06) : valide, invalide ou non vérifiable.
import { ShieldCheck, ShieldQuestion, ShieldX, X } from "lucide-vue-next";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { sigLevel, signers } from "../composables/signatures";
import type { DocTab } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t, locale } = useI18n();
const list = computed(() => props.tab.signatures ?? []);
const level = computed(() => sigLevel(list.value));
const names = computed(() => {
  const n = signers(list.value);
  return n.length ? new Intl.ListFormat(locale.value, { type: "conjunction" }).format(n) : t("sig.unknownSigner");
});
const text = computed(() => {
  const l = list.value;
  if (level.value === "bad") return t("sig.banner.badText", { name: names.value });
  if (level.value === "warn") {
    if (l.some((s) => s.status === "unknown" && !s.intact)) return t("sig.banner.unreadableText");
    return l.some((s) => s.status === "unknown" && s.trusted && !s.certValidAtSigning) ? t("sig.banner.expiredText") : t("sig.banner.warnText");
  }
  return l.every((s) => s.coversWhole) ? t("sig.banner.okText") : t("sig.banner.okLaterText");
});
</script>

<template>
  <div class="banner" :class="level" role="status">
    <ShieldCheck v-if="level === 'ok'" class="ic bi" aria-hidden="true" />
    <ShieldX v-else-if="level === 'bad'" class="ic bi" aria-hidden="true" />
    <ShieldQuestion v-else class="ic bi" aria-hidden="true" />
    <span>
      <b>{{ t(`sig.banner.${level}`) }}</b>
      <template v-if="level === 'ok'"> {{ t("sig.banner.by", { name: names }) }}</template>
      <span class="dim"> · {{ text }}</span>
    </span>
    <div class="grow" />
    <button class="btn" :class="tab.sigPanel ? 'press' : 'gh'" :aria-expanded="tab.sigPanel" @click="tab.sigPanel = !tab.sigPanel">{{ t("sig.details") }}</button>
    <button class="cl" style="width: 26px; height: 26px" :aria-label="t('sig.hideBanner')" @click="tab.sigBannerHidden = true">
      <X class="ic s" aria-hidden="true" />
    </button>
  </div>
</template>

<style scoped>
.btn.press { background: var(--press); color: var(--text); }
</style>
