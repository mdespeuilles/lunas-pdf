<script setup lang="ts">
// « Nouvelle signature » (planche 07) : Dessiner, Importer une image, Taper.
import { Check, Eraser, ImageUp, PenLine, Type, X } from "lucide-vue-next";
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { commands, inTauri, unwrap } from "../lib/api";
import { SIG_SCALE, TYPED_FONTS, imageCanvas, loadImage, removeWhite, toPng, trim, typedCanvas } from "../lib/signature-image";
import { placeSignature } from "../composables/place-signature";
import { useSettings } from "../stores/settings";
import { useSignatures } from "../stores/signatures";
import { useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";

const { t } = useI18n();
const sigs = useSignatures();
const tabs = useTabs();
const ui = useUi();
const settings = useSettings();
const d = computed(() => sigs.dialog!);
const tab = computed(() => tabs.tabs.find((x) => x.key === d.value.tabKey));

type Mode = "draw" | "import" | "type";
const mode = ref<Mode>("draw");
const remember = ref(true);
const busy = ref(false);
const card = ref<HTMLElement>();

// --- Dessiner -------------------------------------------------------------------------------
const INKS = [
  { key: "black", color: "#1b1b20" },
  { key: "blue", color: "#1c3fa8" },
] as const;
const WIDTHS = [
  { key: "thin", px: 1.6, dot: 3 },
  { key: "medium", px: 2.6, dot: 5 },
  { key: "thick", px: 4, dot: 8 },
] as const;
const ink = ref<string>(INKS[0].color);
const width = ref<number>(WIDTHS[1].px);
const pad = ref<HTMLCanvasElement>();
type Pt = { x: number; y: number; p: number };
const strokes = ref<{ color: string; width: number; pts: Pt[] }[]>([]);
let drawing: { color: string; width: number; pts: Pt[] } | null = null;

function padPoint(e: PointerEvent): Pt {
  const r = pad.value!.getBoundingClientRect();
  return { x: (e.clientX - r.left) * SIG_SCALE, y: (e.clientY - r.top) * SIG_SCALE, p: e.pointerType === "pen" && e.pressure > 0 ? e.pressure : 0.5 };
}

function redraw() {
  const c = pad.value;
  if (!c) return;
  const ctx = c.getContext("2d")!;
  ctx.clearRect(0, 0, c.width, c.height);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  for (const s of strokes.value) {
    ctx.strokeStyle = s.color;
    ctx.fillStyle = s.color;
    const pts = s.pts;
    if (pts.length === 1) {
      ctx.beginPath();
      ctx.arc(pts[0].x, pts[0].y, (s.width * SIG_SCALE) / 2, 0, Math.PI * 2);
      ctx.fill();
      continue;
    }
    // Courbes lissées par les milieux des segments ; épaisseur selon la pression du stylet.
    for (let i = 1; i < pts.length; i++) {
      const a = pts[i - 1];
      const b = pts[i];
      const m0 = i === 1 ? a : { x: (pts[i - 2].x + a.x) / 2, y: (pts[i - 2].y + a.y) / 2 };
      const m1 = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
      ctx.lineWidth = s.width * SIG_SCALE * (0.6 + 0.8 * ((a.p + b.p) / 2));
      ctx.beginPath();
      ctx.moveTo(m0.x, m0.y);
      ctx.quadraticCurveTo(a.x, a.y, m1.x, m1.y);
      if (i === pts.length - 1) ctx.lineTo(b.x, b.y);
      ctx.stroke();
    }
  }
}

function onPadDown(e: PointerEvent) {
  if (e.button !== 0) return;
  (e.target as HTMLElement).setPointerCapture(e.pointerId);
  drawing = { color: ink.value, width: width.value, pts: [padPoint(e)] };
  strokes.value.push(drawing);
  redraw();
}
function onPadMove(e: PointerEvent) {
  if (!drawing) return;
  for (const ev of e.getCoalescedEvents?.() ?? [e]) drawing.pts.push(padPoint(ev));
  redraw();
}
function onPadUp() {
  drawing = null;
}
function clearPad() {
  strokes.value = [];
  redraw();
}

function sizePad() {
  const c = pad.value;
  if (!c) return;
  const r = c.getBoundingClientRect();
  c.width = Math.round(r.width * SIG_SCALE);
  c.height = Math.round(r.height * SIG_SCALE);
  redraw();
}

// --- Importer -------------------------------------------------------------------------------
const imported = ref<{ name: string; canvas: HTMLCanvasElement } | null>(null);
const dropWhite = ref(true);
const preview = ref("");

async function pickImage() {
  let path: string | null = null;
  if (inTauri) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ title: t("signature.pickImage"), filters: [{ name: t("annot.imageFilter"), extensions: ["png", "jpg", "jpeg", "svg", "PNG", "JPG", "JPEG", "SVG"] }] });
    path = typeof r === "string" ? r : null;
  } else {
    path = (window as unknown as { __FEUILLET_E2E__?: { image?: string } }).__FEUILLET_E2E__?.image ?? null;
  }
  if (!path) return;
  try {
    const bytes = new Uint8Array(await unwrap(commands.readImageFile(path)));
    const img = await loadImage(bytes, path);
    imported.value = { name: path.split(/[\\/]/).pop() ?? path, canvas: imageCanvas(img) };
  } catch (e) {
    ui.notify(t("signature.imageError", { msg: String(e) }));
  }
}

function importedResult(): HTMLCanvasElement | null {
  const src = imported.value?.canvas;
  if (!src) return null;
  const c = document.createElement("canvas");
  c.width = src.width;
  c.height = src.height;
  c.getContext("2d")!.drawImage(src, 0, 0);
  if (dropWhite.value) removeWhite(c);
  return trim(c, 4) ?? c;
}

watch([imported, dropWhite], () => {
  preview.value = importedResult()?.toDataURL() ?? "";
});

// --- Taper ----------------------------------------------------------------------------------
const name = ref(settings.settings.authorName || tab.value?.edit?.author || "");
const font = ref<string>(TYPED_FONTS[0].family);

// --- Validation -----------------------------------------------------------------------------
const ready = computed(() =>
  mode.value === "draw" ? strokes.value.length > 0 : mode.value === "import" ? !!imported.value : name.value.trim() !== "",
);

async function result(): Promise<HTMLCanvasElement | null> {
  if (mode.value === "draw") return pad.value ? trim(pad.value, 2 * SIG_SCALE) : null;
  if (mode.value === "import") return importedResult();
  return typedCanvas(name.value.trim(), font.value, "#1b1b20");
}

async function add() {
  if (!ready.value || busy.value) return;
  busy.value = true;
  try {
    const c = await result();
    if (!c) return;
    const png = await toPng(c);
    if (remember.value) await sigs.save(png, c.width, c.height).catch((e) => ui.notify(String(e)));
    const target = d.value.target;
    const tb = tab.value;
    close();
    if (tb) await placeSignature(tb, png, target);
  } finally {
    busy.value = false;
  }
}

function close() {
  sigs.dialog = null;
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    close();
  }
}

watch(mode, async (m) => {
  if (m === "draw") {
    await nextTick();
    sizePad();
  }
});
onMounted(async () => {
  await nextTick();
  sizePad();
  window.addEventListener("resize", sizePad);
  card.value?.querySelector<HTMLElement>(".seg button.on")?.focus();
});
onBeforeUnmount(() => window.removeEventListener("resize", sizePad));
</script>

<template>
  <div class="scrim" @mousedown.self="close">
    <div ref="card" class="modal" role="dialog" aria-modal="true" aria-labelledby="sig-title" @keydown="onKey">
      <div class="mhd">
        <h3 id="sig-title">{{ t("signature.new") }}</h3>
        <button class="cl" style="width: 26px; height: 26px" :aria-label="t('sig.close')" @click="close"><X class="ic s" aria-hidden="true" /></button>
      </div>
      <div class="mbd">
        <div class="seg tabs" role="tablist">
          <button role="tab" :aria-selected="mode === 'draw'" :class="{ on: mode === 'draw' }" @click="mode = 'draw'"><PenLine class="ic xs" aria-hidden="true" />{{ t("signature.draw") }}</button>
          <button role="tab" :aria-selected="mode === 'import'" :class="{ on: mode === 'import' }" @click="mode = 'import'"><ImageUp class="ic xs" aria-hidden="true" />{{ t("signature.import") }}</button>
          <button role="tab" :aria-selected="mode === 'type'" :class="{ on: mode === 'type' }" @click="mode = 'type'"><Type class="ic xs" aria-hidden="true" />{{ t("signature.type") }}</button>
        </div>

        <template v-if="mode === 'draw'">
          <div class="pad">
            <span class="bx" aria-hidden="true">×</span>
            <span class="base" aria-hidden="true" />
            <canvas
              ref="pad"
              role="img"
              :aria-label="t('signature.padLabel')"
              @pointerdown="onPadDown"
              @pointermove="onPadMove"
              @pointerup="onPadUp"
              @pointercancel="onPadUp"
            />
          </div>
          <div class="row">
            <button v-for="i in INKS" :key="i.key" class="sw" :class="{ on: ink === i.color }" :style="{ background: i.color }" :aria-label="t(`signature.ink.${i.key}`)" :aria-pressed="ink === i.color" @click="ink = i.color" />
            <span style="width: 8px" />
            <div class="seg" style="flex: none">
              <button v-for="w in WIDTHS" :key="w.key" class="wd" :class="{ on: width === w.px }" :aria-label="t(`signature.width.${w.key}`)" :aria-pressed="width === w.px" @click="width = w.px">
                <i :style="{ width: w.dot + 'px', height: w.dot + 'px' }" />
              </button>
            </div>
            <span class="grow" />
            <button class="btn gh" :disabled="!strokes.length" @click="clearPad"><Eraser class="ic s" aria-hidden="true" />{{ t("signature.clear") }}</button>
          </div>
          <span class="hint">{{ t("signature.drawHint") }}</span>
        </template>

        <template v-else-if="mode === 'import'">
          <div class="chk">
            <img v-if="preview" :src="preview" alt="" class="imgprev" />
            <button v-else class="btn out" @click="pickImage">{{ t("signature.chooseImage") }}</button>
          </div>
          <div v-if="imported" class="row">
            <span class="fname grow">{{ imported.name }} · {{ t("signature.pixels", { w: imported.canvas.width, h: imported.canvas.height }) }}</span>
            <button class="btn out" @click="pickImage">{{ t("signature.replace") }}</button>
          </div>
          <button class="row swrow" role="switch" :aria-checked="dropWhite" @click="dropWhite = !dropWhite">
            <span class="sw-t" :class="{ on: dropWhite }" aria-hidden="true" />{{ t("signature.removeWhite") }}
          </button>
          <span class="hint">{{ t("signature.importHint") }}</span>
        </template>

        <template v-else>
          <label class="field">
            <span>{{ t("signature.yourName") }}</span>
            <input v-model="name" class="inp" type="text" autocomplete="name" spellcheck="false" />
          </label>
          <div class="fonts" role="radiogroup" :aria-label="t('signature.style')">
            <button v-for="f in TYPED_FONTS" :key="f.family" class="fopt" :class="{ on: font === f.family }" role="radio" :aria-checked="font === f.family" @click="font = f.family">
              <span class="radio" :class="{ on: font === f.family }" aria-hidden="true" />
              <span class="fprev" :style="{ fontFamily: `'${f.family}', cursive`, fontSize: f.size + 'px' }">{{ name.trim() || t("signature.namePlaceholder") }}</span>
            </button>
          </div>
        </template>
      </div>
      <div class="mft">
        <label class="row rem">
          <button class="cb" role="checkbox" :aria-checked="remember" :class="{ on: remember }" @click="remember = !remember"><Check v-if="remember" class="ic xs" aria-hidden="true" /></button>
          <span @click="remember = !remember">{{ t("signature.remember") }}</span>
        </label>
        <button class="btn gh" @click="close">{{ t("save.cancel") }}</button>
        <button class="btn pri" :disabled="!ready || busy" @click="add">{{ t("signature.add") }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scrim { position: fixed; inset: 0; z-index: 100; background: var(--scrim); display: flex; align-items: center; justify-content: center; padding: 16px; }
.modal { width: 520px; max-width: 100%; max-height: 100%; overflow: auto; background: var(--surface); border-radius: 14px; box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); display: flex; flex-direction: column; }
.mhd { display: flex; align-items: center; gap: 10px; padding: 14px 12px 4px 20px; }
.mhd h3 { margin: 0; font-size: 15px; font-weight: 600; flex: 1; }
.mbd { padding: 12px 20px 18px; display: flex; flex-direction: column; gap: 14px; }
.mft { display: flex; align-items: center; gap: 8px; padding: 12px 16px 12px 20px; border-top: 1px solid var(--line); background: var(--surface-2); }
:root.dark .mft { background: rgba(0, 0, 0, .12); }
.tabs > button { flex: 1; height: 28px; }
.seg > button.on { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--line), 0 1px 2px rgba(0, 0, 0, .08); }
:root.dark .seg > button.on { background: #44444b; }
.pad { position: relative; height: 180px; border-radius: 10px; background: #fff; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .12); overflow: hidden; }
.pad canvas { position: absolute; inset: 0; width: 100%; height: 100%; cursor: crosshair; touch-action: none; }
.bx { position: absolute; left: 28px; bottom: 48px; font-size: 14px; color: #9a9aa2; }
.base { position: absolute; left: 28px; right: 28px; bottom: 42px; border-top: 1px solid #c9c9cf; }
.row { display: flex; align-items: center; gap: 10px; }
.grow { flex: 1; }
.sw { width: 20px; height: 20px; border-radius: 50%; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .18); flex: none; border: 0; padding: 0; }
.sw.on { box-shadow: 0 0 0 2px var(--surface), 0 0 0 3.5px var(--text-2); }
.sw:focus-visible { box-shadow: var(--ring); }
.wd { display: grid; place-items: center; width: 30px; height: 26px; border-radius: 7px; border: 0; background: transparent; padding: 0; min-width: 0; }
.wd i { display: block; border-radius: 50%; background: var(--text); }
.hint { font-size: 12px; color: var(--text-3); }
.chk { position: relative; height: 180px; border-radius: 10px; overflow: hidden; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, .12); background-color: #fff; background-image: linear-gradient(45deg, #ececf0 25%, transparent 25%), linear-gradient(-45deg, #ececf0 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #ececf0 75%), linear-gradient(-45deg, transparent 75%, #ececf0 75%); background-size: 16px 16px; background-position: 0 0, 0 8px, 8px -8px, -8px 0; display: grid; place-items: center; }
.imgprev { max-width: 90%; max-height: 150px; }
.fname { font-size: 11.5px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.swrow { border: 0; background: transparent; padding: 0; font: inherit; font-size: 13px; color: var(--text); text-align: left; }
.swrow:focus-visible { box-shadow: var(--ring); border-radius: 6px; }
.sw-t { position: relative; width: 30px; height: 18px; border-radius: 9px; background: var(--line-2); flex: none; }
.sw-t::after { content: ""; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; box-shadow: 0 1px 2px rgba(0, 0, 0, .25); transition: left .15s; }
.sw-t.on { background: var(--accent); }
.sw-t.on::after { left: 14px; }
.field { display: flex; flex-direction: column; gap: 6px; }
.field > span { font-size: 12px; font-weight: 600; color: var(--text-2); }
.inp { height: 34px; border-radius: 7px; border: 0; box-shadow: inset 0 0 0 1px var(--line-2); background: var(--surface); color: var(--text); padding: 0 10px; font: inherit; font-size: 13.5px; }
.inp:focus { outline: none; box-shadow: inset 0 0 0 1px var(--accent), var(--ring); }
.fonts { display: flex; flex-direction: column; gap: 8px; }
.fopt { display: flex; align-items: center; gap: 12px; height: 60px; padding: 0 14px; border-radius: 10px; border: 0; box-shadow: inset 0 0 0 1px var(--line-2); background: var(--bg); text-align: left; overflow: hidden; }
.fopt.on { box-shadow: inset 0 0 0 2px var(--accent); background: var(--accent-soft); }
.fopt:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.radio { width: 16px; height: 16px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--line-2); background: var(--surface); flex: none; box-sizing: border-box; }
.radio.on { box-shadow: inset 0 0 0 5px var(--accent); background: #fff; }
.fprev { flex: 1; color: var(--text); line-height: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.rem { flex: 1; font-size: 12.5px; color: var(--text-2); cursor: pointer; }
.cb { display: grid; place-items: center; width: 16px; height: 16px; padding: 0; border-radius: 4px; border: 0; box-shadow: inset 0 0 0 1.5px var(--line-2); background: var(--surface); color: var(--on-accent); flex: none; }
.cb.on { background: var(--accent); box-shadow: none; }
.cb:focus-visible { box-shadow: var(--ring); }
.btn:disabled { opacity: .5; }
</style>
