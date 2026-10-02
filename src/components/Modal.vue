<script setup lang="ts">
// Boîte de dialogue modale : voile, focus piégé, fermeture sur Échap.
import { X } from "lucide-vue-next";
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";

const props = defineProps<{ title: string; closeLabel: string; width?: number }>();
const emit = defineEmits<{ close: [] }>();
const card = ref<HTMLElement>();
let previous: HTMLElement | null = null;

function focusables() {
  return [...(card.value?.querySelectorAll<HTMLElement>("button, input, select, [tabindex]:not([tabindex='-1'])") ?? [])].filter((e) => !e.hasAttribute("disabled"));
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    emit("close");
  } else if (e.key === "Tab") {
    const f = focusables();
    if (!f.length) return;
    const i = f.indexOf(document.activeElement as HTMLElement);
    if (e.shiftKey && i <= 0) {
      e.preventDefault();
      f.at(-1)!.focus();
    } else if (!e.shiftKey && i === f.length - 1) {
      e.preventDefault();
      f[0].focus();
    }
  }
}

onMounted(async () => {
  previous = document.activeElement as HTMLElement | null;
  await nextTick();
  // Premier élément après la croix de fermeture, sinon la croix : le focus doit entrer dans
  // la boîte (Échap et Tab y sont gérés).
  const f = focusables();
  (f[1] ?? f[0])?.focus();
});
onBeforeUnmount(() => previous?.focus());
</script>

<template>
  <div class="scrim" @mousedown.self="emit('close')">
    <div ref="card" class="dlg" role="dialog" aria-modal="true" aria-labelledby="dlg-title" :style="{ width: (props.width ?? 440) + 'px' }" @keydown="onKey">
      <header>
        <h2 id="dlg-title">{{ title }}</h2>
        <button class="cl" style="width: 26px; height: 26px" :aria-label="closeLabel" @click="emit('close')"><X class="ic s" aria-hidden="true" /></button>
      </header>
      <slot />
    </div>
  </div>
</template>

<style scoped>
.scrim { position: fixed; inset: 0; z-index: 100; background: var(--scrim); display: flex; align-items: center; justify-content: center; padding: 16px; }
.dlg { max-width: 100%; max-height: 100%; overflow: auto; border-radius: 16px; background: var(--surface); box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); padding: 20px 24px 24px; }
header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
h2 { margin: 0; font-size: 17px; font-weight: 600; }
</style>
