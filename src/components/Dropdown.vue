<script setup lang="ts">
// Menu déroulant accessible : fermeture au clic extérieur et sur Échap, navigation aux flèches.
import { nextTick, onBeforeUnmount, ref, watch } from "vue";

const props = withDefaults(defineProps<{ align?: "left" | "right"; width?: number }>(), { align: "left", width: 236 });
const open = ref(false);
const root = ref<HTMLElement>();
const menu = ref<HTMLElement>();

function items(): HTMLElement[] {
  return [...(menu.value?.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])') ?? [])];
}

async function toggle(focusFirst = false) {
  open.value = !open.value;
  if (open.value) {
    await nextTick();
    const list = items();
    (list.find((i) => i.classList.contains("on")) ?? (focusFirst ? list[0] : undefined))?.focus();
  }
}

function close() {
  open.value = false;
}

function onKey(e: KeyboardEvent) {
  const list = items();
  const i = list.indexOf(document.activeElement as HTMLElement);
  if (e.key === "Escape") {
    e.stopPropagation();
    close();
    (root.value?.querySelector("[aria-haspopup]") as HTMLElement | null)?.focus();
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    list[(i + 1) % list.length]?.focus();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    list[(i - 1 + list.length) % list.length]?.focus();
  } else if (e.key === "Tab") {
    close();
  }
}

function onDocDown(e: MouseEvent) {
  if (root.value && !root.value.contains(e.target as Node)) close();
}
watch(open, (o) => (o ? document.addEventListener("mousedown", onDocDown, true) : document.removeEventListener("mousedown", onDocDown, true)));
onBeforeUnmount(() => document.removeEventListener("mousedown", onDocDown, true));

defineExpose({ toggle, close });
</script>

<template>
  <div ref="root" class="dd">
    <slot name="trigger" :open="open" :toggle="toggle" />
    <div
      v-if="open"
      ref="menu"
      class="pop"
      role="menu"
      :style="{ width: props.width + 'px', [props.align]: '0' }"
      @keydown="onKey"
      @click="close"
    >
      <slot />
    </div>
  </div>
</template>

<style scoped>
.dd { position: relative; display: flex; align-items: center; }
.pop { top: calc(100% + 6px); }
</style>
