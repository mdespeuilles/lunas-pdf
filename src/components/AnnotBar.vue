<script setup lang="ts">
// Barre d'annotation (planche 03) : outils, panneau contextuel, annuler / rétablir, « Terminé ».
import {
  ArrowUpRight, ChevronDown, Ellipse, EyeOff, Highlighter, Image as ImageIcon, MessageSquare, Redo2, Signature,
  Slash, Square, SquareCheck, Strikethrough, TextCursor, Type, Underline, Undo2,
} from "lucide-vue-next";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import Dropdown from "./Dropdown.vue";
import { inTauri, commands, unwrap } from "../lib/api";
import { PALETTE, SIZES, WIDTHS, type Tool, useAnnotTools, colorFor } from "../stores/annotTools";
import { type DocTab, useTabs } from "../stores/tabs";
import { placeImageBytes } from "../composables/place-signature";
import { useSignatures } from "../stores/signatures";
import { useUi } from "../stores/ui";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const tools = useAnnotTools();
const tabs = useTabs();
const ui = useUi();
const sigs = useSignatures();

interface ToolDef {
  tool: Tool;
  key: string;
  icon: unknown;
  sep?: boolean;
  fly?: boolean;
}
const TOOLS: ToolDef[] = [
  { tool: "select", key: "V", icon: TextCursor, sep: true },
  { tool: "highlight", key: "H", icon: Highlighter },
  { tool: "underline", key: "U", icon: Underline },
  { tool: "strike", key: "K", icon: Strikethrough, sep: true },
  { tool: "text", key: "T", icon: Type },
  { tool: "note", key: "N", icon: MessageSquare, sep: true },
  { tool: "rect", key: "R", icon: Square },
  { tool: "ellipse", key: "O", icon: Ellipse },
  { tool: "line", key: "L", icon: Slash },
  { tool: "arrow", key: "A", icon: ArrowUpRight, sep: true },
  { tool: "check", key: "C", icon: SquareCheck, fly: true },
  { tool: "signature", key: "S", icon: Signature },
  { tool: "image", key: "I", icon: ImageIcon },
  { tool: "redact", key: "X", icon: EyeOff },
];

function label(d: ToolDef) {
  const name = t(`annot.tools.${d.tool}`);
  return d.tool === "check" ? `${name} (${d.key}) — ${t("annot.checkHint")}` : `${name} (${d.key})`;
}

// Infobulle de la planche (nom + touche).
const tip = ref<{ text: string; key: string; left: number } | null>(null);
const bar = ref<HTMLElement>();
function showTip(d: ToolDef, e: Event) {
  const el = e.currentTarget as HTMLElement;
  const br = bar.value!.getBoundingClientRect();
  const r = el.getBoundingClientRect();
  tip.value = { text: d.tool === "redact" ? `${t("annot.tools.redact")} — ${t("annot.redactHint")}` : t(`annot.tools.${d.tool}`), key: d.key, left: r.left - br.left };
}

async function pick(tool: Tool) {
  tools.cancelPlacing();
  if (tool === "signature") {
    // Menu des signatures sous le bouton S (planche 07).
    const r = bar.value?.querySelector<HTMLElement>("[data-tool='signature']")?.getBoundingClientRect();
    sigs.picker = { tabKey: props.tab.key, x: r?.left ?? 200, y: (r?.bottom ?? 100) + 6, target: null };
    return;
  }
  if (tool === "image") {
    await placeImage();
    return;
  }
  tools.tool = tool;
  props.tab.selected = null;
}

/** Outil « Tampon ou image » : choix du fichier puis pose au clic. */
async function placeImage() {
  if (!props.tab.info) return;
  let path: string | null = null;
  if (inTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ title: t("annot.pickImage"), filters: [{ name: t("annot.imageFilter"), extensions: ["png", "jpg", "jpeg", "PNG", "JPG", "JPEG"] }] });
    path = typeof r === "string" ? r : null;
  } else {
    path = (window as unknown as { __FEUILLET_E2E__?: { image?: string } }).__FEUILLET_E2E__?.image ?? null;
  }
  if (!path) return;
  try {
    const bytes = new Uint8Array(await unwrap(commands.readImageFile(path)));
    await placeImageBytes(props.tab, bytes, { maxW: 160, mime: /\.jpe?g$/i.test(path) ? "image/jpeg" : "image/png" });
  } catch (e) {
    ui.notify(String(e));
  }
}

// Raccourcis I et S : la barre exécute l'action.
watch(() => tools.request, (r) => r && void pick(r.tool));

const ctx = computed(() => {
  const tl = tools.tool;
  if (tl === "text") return "text";
  if (["rect", "ellipse", "line", "arrow"].includes(tl)) return "shape";
  if (tl === "check") return "check";
  if (["highlight", "underline", "strike", "note"].includes(tl)) return "color";
  return null;
});

function done() {
  tools.cancelPlacing();
  tabs.toggleAnnotating(props.tab, false);
  tools.tool = "select";
}
</script>

<template>
  <div ref="bar" class="annobar" role="toolbar" :aria-label="t('annot.bar')" @mouseleave="tip = null">
    <template v-for="d in TOOLS" :key="d.tool">
      <Dropdown v-if="d.fly" :width="168">
        <template #trigger="{ toggle }">
          <button
            class="tb"
            :class="{ on: tools.tool === d.tool }"
            :aria-label="label(d)"
            :aria-pressed="tools.tool === d.tool"
            aria-haspopup="menu"
            @click="tools.tool === d.tool ? toggle(true) : pick(d.tool)"
            @mouseenter="showTip(d, $event)"
            @focus="showTip(d, $event)"
            @blur="tip = null"
          >
            <component :is="d.icon" class="ic" aria-hidden="true" /><span class="fly" />
          </button>
        </template>
        <button v-for="cs in (['check', 'cross', 'dot'] as const)" :key="cs" class="mi" role="menuitem" :class="{ on: tools.checkStyle === cs }" @click="tools.checkStyle = cs; pick('check')">
          <span class="ck" />{{ t(`annot.checkStyles.${cs}`) }}
        </button>
      </Dropdown>
      <button
        v-else
        class="tb"
        :class="{ on: tools.tool === d.tool || (d.tool === 'signature' && !!sigs.picker) }"
        :aria-label="label(d)"
        :aria-pressed="tools.tool === d.tool"
        :aria-haspopup="d.tool === 'signature' ? 'menu' : undefined"
        :data-tool="d.tool"
        @click="pick(d.tool)"
        @mouseenter="showTip(d, $event)"
        @focus="showTip(d, $event)"
        @blur="tip = null"
      >
        <component :is="d.icon" class="ic" aria-hidden="true" /><span v-if="d.tool === 'signature'" class="fly" />
      </button>
      <div v-if="d.sep" class="sep" />
    </template>

    <div class="grow" />

    <template v-if="ctx === 'text'">
      <span class="ctxl">{{ t("annot.ctxText") }}</span>
      <label class="sel-s">
        <span class="sr-only">{{ t("annot.font") }}</span>
        <select v-model="tools.font" :aria-label="t('annot.font')">
          <option v-for="f in (['sans', 'serif', 'mono'] as const)" :key="f" :value="f">{{ t(`annot.fonts.${f}`) }}</option>
        </select>
        <ChevronDown class="ic xs" aria-hidden="true" />
      </label>
      <label class="sel-s">
        <select v-model.number="tools.size" :aria-label="t('annot.size')">
          <option v-for="s in SIZES" :key="s" :value="s">{{ t("annot.sizePt", { n: s }) }}</option>
        </select>
        <ChevronDown class="ic xs" aria-hidden="true" />
      </label>
      <div class="sep" />
    </template>
    <template v-else-if="ctx === 'shape'">
      <span class="ctxl">{{ t("annot.ctxShape") }}</span>
      <label class="sel-s">
        <select v-model.number="tools.width" :aria-label="t('annot.width')">
          <option v-for="w in WIDTHS" :key="w" :value="w">{{ t("annot.sizePt", { n: w }) }}</option>
        </select>
        <ChevronDown class="ic xs" aria-hidden="true" />
      </label>
      <div class="sep" />
    </template>
    <template v-else-if="ctx === 'check'">
      <span class="ctxl">{{ t("annot.ctxCheck") }}</span>
      <div class="seg" role="radiogroup" :aria-label="t('annot.ctxCheck')">
        <button v-for="cs in (['check', 'cross', 'dot'] as const)" :key="cs" role="radio" :aria-checked="tools.checkStyle === cs" :class="{ on: tools.checkStyle === cs }" @click="tools.checkStyle = cs">
          {{ t(`annot.checkStyles.${cs}`) }}
        </button>
      </div>
      <div class="sep" />
    </template>
    <template v-if="ctx">
      <div class="swatches" role="radiogroup">
        <button
          v-for="p in PALETTE"
          :key="p.key"
          class="sw"
          role="radio"
          :aria-checked="tools.colorKey === p.key"
          :class="{ on: tools.colorKey === p.key }"
          :style="{ background: colorFor(p.key, tools.tool) }"
          :aria-label="t(`annot.colors.${p.key}`)"
          :title="t(`annot.colors.${p.key}`)"
          @click="tools.colorKey = p.key"
        />
      </div>
      <div class="sep" />
    </template>

    <button class="tb" :disabled="!tab.edit?.canUndo" :aria-label="t('annot.undo')" :title="t('annot.undo')" @click="tabs.undo(tab)"><Undo2 class="ic" aria-hidden="true" /></button>
    <button class="tb" :disabled="!tab.edit?.canRedo" :aria-label="t('annot.redo')" :title="t('annot.redo')" @click="tabs.redo(tab)"><Redo2 class="ic" aria-hidden="true" /></button>
    <button class="btn pri done" @click="done">{{ t("annot.done") }}</button>

    <div v-if="tip" class="tip" :style="{ left: tip.left + 'px' }" role="tooltip">{{ tip.text }}<span class="kbd">{{ tip.key }}</span></div>
  </div>
</template>

<style scoped>
.annobar { display: flex; align-items: center; gap: 2px; height: 44px; padding: 0 10px; background: var(--bg); border-bottom: 1px solid var(--line); flex: none; position: relative; z-index: 4; min-width: 0; }
.tb .fly { position: absolute; right: 3px; bottom: 3px; width: 0; height: 0; border-left: 4px solid transparent; border-bottom: 4px solid currentColor; opacity: .6; }
.ctxl { font-size: 12px; color: var(--text-3); margin-right: 2px; white-space: nowrap; }
.sel-s { position: relative; display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 6px 0 10px; border-radius: 7px; background: var(--hover); font-size: 12.5px; color: var(--text); }
.sel-s select { appearance: none; -webkit-appearance: none; border: 0; background: transparent; color: inherit; font: inherit; padding: 0 16px 0 0; margin-right: -16px; cursor: default; }
.sel-s select:focus-visible { box-shadow: none; }
.sel-s:focus-within { box-shadow: var(--ring); }
.sel-s .ic { pointer-events: none; }
.swatches { display: flex; align-items: center; gap: 8px; padding: 0 4px; }
.sw { width: 18px; height: 18px; border-radius: 50%; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .18); flex: none; border: 0; padding: 0; }
.sw.on { box-shadow: 0 0 0 2px var(--bg), 0 0 0 3.5px var(--text-2); }
.sw:focus-visible { box-shadow: 0 0 0 2px var(--bg), var(--ring); }
.done { height: 28px; margin-left: 6px; }
.tip { position: absolute; top: 40px; z-index: 40; display: flex; align-items: center; gap: 8px; padding: 5px 6px 5px 9px; border-radius: 7px; background: var(--tip-bg); color: var(--tip-fg); font-size: 12px; white-space: nowrap; box-shadow: 0 4px 14px rgba(0, 0, 0, .22); pointer-events: none; }
.tip .kbd { background: rgba(255, 255, 255, .14); box-shadow: none; color: #e4e4ea; }
@media (max-width: 1180px) { .ctxl { display: none; } }
</style>
