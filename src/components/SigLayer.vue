<script setup lang="ts">
// Signatures visibles d'une page : zone cliquable qui ouvre le panneau de détails.
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { DocTab } from "../stores/tabs";

const props = defineProps<{ tab: DocTab; page: number; k: number }>();
const { t } = useI18n();
const items = computed(() => (props.tab.signatures ?? []).map((s, i) => ({ s, i })).filter(({ s }) => s.page === props.page && s.rect));

function open(i: number) {
  props.tab.sigSelected = i;
  props.tab.sigPanel = true;
}
</script>

<template>
  <button
    v-for="{ s, i } in items"
    :key="s.field"
    class="sigz"
    :class="s.status"
    :style="{ left: `${s.rect!.x * k}px`, top: `${s.rect!.y * k}px`, width: `${s.rect!.w * k}px`, height: `${s.rect!.h * k}px` }"
    :aria-label="t('sig.open', { name: s.signer ?? t('sig.unknownSigner') })"
    :title="t('sig.open', { name: s.signer ?? t('sig.unknownSigner') })"
    @click="open(i)"
  />
</template>

<style scoped>
.sigz { position: absolute; z-index: 3; border: 0; padding: 0; background: transparent; cursor: pointer; border-radius: 3px; }
.sigz:hover { box-shadow: 0 0 0 2px var(--ok); }
.sigz.invalid:hover { box-shadow: 0 0 0 2px var(--bad); }
.sigz.unknown:hover { box-shadow: 0 0 0 2px var(--warn); }
.sigz:focus-visible { box-shadow: var(--ring); }
</style>
