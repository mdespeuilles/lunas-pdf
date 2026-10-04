<script setup lang="ts">
// Menu de l'outil Signature (planche 07) : signatures enregistrées, création, import.
import { ImagePlus, PenLine, Trash2 } from "lucide-vue-next";
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { SavedSignature } from "../bindings";
import { dataUrlBytes, placeSignature } from "../composables/place-signature";
import { useAnnotTools } from "../stores/annotTools";
import { useSignatures } from "../stores/signatures";
import { useTabs } from "../stores/tabs";

const { t, locale } = useI18n();
const sigs = useSignatures();
const tabs = useTabs();
const tools = useAnnotTools();
const el = ref<HTMLElement>();
const p = computed(() => sigs.picker!);
const tab = computed(() => tabs.tabs.find((x) => x.key === p.value.tabKey));
const pos = computed(() => {
  const w = 320;
  return { left: `${Math.max(8, Math.min(p.value.x, window.innerWidth - w - 8))}px`, top: `${Math.min(p.value.y, window.innerHeight - 120)}px` };
});

function close() {
  sigs.picker = null;
}

async function use(s: SavedSignature) {
  const target = p.value.target;
  const tb = tab.value;
  close();
  if (tb) await placeSignature(tb, dataUrlBytes(s.data ?? ""), target);
}

function create() {
  sigs.dialog = { tabKey: p.value.tabKey, target: p.value.target };
  close();
}

function importImage() {
  close();
  tools.request = { tool: "image", seq: Date.now() };
}

function added(s: SavedSignature) {
  return t("signature.addedOn", { date: new Intl.DateTimeFormat(locale.value, { day: "numeric", month: "short" }).format(s.created) });
}

function onDown(e: PointerEvent) {
  if (!el.value?.contains(e.target as Node)) close();
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    close();
  } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    const items = [...(el.value?.querySelectorAll<HTMLElement>("[data-item]") ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
    e.preventDefault();
  }
}

onMounted(async () => {
  window.addEventListener("pointerdown", onDown, true);
  if (!sigs.loaded) await sigs.load();
  await nextTick();
  el.value?.querySelector<HTMLElement>("[data-item]")?.focus();
});
onBeforeUnmount(() => window.removeEventListener("pointerdown", onDown, true));
</script>

<template>
  <div ref="el" class="pop sigpop" role="menu" :aria-label="t('signature.menu')" :style="pos" @keydown="onKey">
    <template v-if="sigs.list.length">
      <div class="mh6">{{ t("signature.saved") }}</div>
      <div v-for="s in sigs.list" :key="s.id" class="si">
        <button class="pick" role="menuitem" data-item :aria-label="`${t('signature.signature')} — ${added(s)}`" @click="use(s)">
          <span class="sprev"><img :src="s.data" alt="" /></span>
          <span class="sl">{{ t("signature.signature") }}<span>{{ added(s) }}</span></span>
        </button>
        <button class="cl" :aria-label="t('signature.delete')" :title="t('signature.delete')" @click="sigs.remove(s.id)"><Trash2 class="ic xs" aria-hidden="true" /></button>
      </div>
      <div class="msep" />
    </template>
    <button class="mi" role="menuitem" data-item @click="create"><PenLine class="ic s" aria-hidden="true" />{{ t("signature.create") }}</button>
    <button class="mi" role="menuitem" data-item @click="importImage"><ImagePlus class="ic s" aria-hidden="true" />{{ t("signature.importStamp") }}<span class="kbd">I</span></button>
  </div>
</template>

<style scoped>
.sigpop { position: fixed; z-index: 40; width: 320px; padding: 6px; box-sizing: border-box; }
.mh6 { padding: 8px 8px 6px; font-size: 11px; font-weight: 600; color: var(--text-3); text-transform: uppercase; letter-spacing: .05em; }
.si { display: flex; align-items: center; gap: 4px; padding: 0 4px 0 0; border-radius: 8px; }
.si:hover, .si:focus-within { background: var(--hover); }
.pick { flex: 1; display: flex; align-items: center; gap: 12px; padding: 6px 8px; border: 0; background: transparent; border-radius: 8px; text-align: left; font: inherit; color: var(--text); min-width: 0; }
.pick:focus-visible { box-shadow: var(--ring); }
.sprev { display: grid; place-items: center; width: 150px; height: 52px; border-radius: 6px; background: #fff; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .08); flex: none; overflow: hidden; }
.sprev img { max-width: 140px; max-height: 46px; }
.sl { flex: 1; display: flex; flex-direction: column; gap: 2px; font-size: 12.5px; }
.sl span { font-size: 11.5px; color: var(--text-3); }
.mi { display: flex; align-items: center; gap: 10px; width: 100%; height: 30px; padding: 0 8px; border: 0; border-radius: 6px; background: transparent; font: inherit; font-size: 13px; color: var(--text); }
/* Survol discret, comme les signatures au-dessus : texte et raccourci gardent leur couleur
   (le style commun des menus les passe en blanc pour un fond d'accent). */
.mi:hover, .mi:focus-visible { background: var(--hover); color: var(--text); outline: none; }
.mi:hover .kbd, .mi:focus-visible .kbd { background: var(--hover); color: var(--text-2); box-shadow: inset 0 0 0 1px var(--line); }
.mi .kbd { margin-left: auto; }
.msep { height: 1px; background: var(--line); margin: 5px 4px; }
</style>
