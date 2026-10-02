<script setup lang="ts">
// « Exporter » (planche 10) : format, qualité des images, pages, mot de passe et autorisations.
import { Check, Eye, EyeOff, Printer, X } from "lucide-vue-next";
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { ExportFormat, ExportOptions, Quality } from "../bindings";
import { BackendError, commands, inTauri, unwrap } from "../lib/api";
import { formatSize, parseRange, passwordStrength } from "../lib/page-range";
import { printTab } from "../composables/print";
import type { DocTab } from "../stores/tabs";
import { useUi } from "../stores/ui";

const props = defineProps<{ tab: DocTab }>();
const emit = defineEmits<{ close: [] }>();
const { t, locale } = useI18n();
const ui = useUi();
const card = ref<HTMLElement>();

type Kind = "pdf" | "flattened" | "images";
const kind = ref<Kind>("pdf");
const imageType = ref<"png" | "jpg">("png");
const dpi = ref(150);
const QUALITIES: Quality[] = ["light", "balanced", "max"];
const qualityIndex = ref(1);
const quality = computed(() => QUALITIES[qualityIndex.value]);
const pagesMode = ref<"all" | "current" | "range">("all");
const range = ref("");
const protect = ref(false);
const password = ref("");
const confirm = ref("");
const showPw = ref(false);
const allowPrint = ref(true);
const allowCopy = ref(false);
const busy = ref(false);

const count = computed(() => props.tab.info?.pages.length ?? 0);
const isPdf = computed(() => kind.value !== "images");
const format = computed<ExportFormat>(() => (kind.value === "images" ? imageType.value : kind.value));
const pages = computed<number[] | null | undefined>(() => {
  if (pagesMode.value === "all") return null;
  if (pagesMode.value === "current") return [props.tab.page];
  return parseRange(range.value, count.value) ?? undefined;
});
const strength = computed(() => passwordStrength(password.value));
const pwError = computed(() => {
  if (!protect.value || !isPdf.value) return "";
  if (!password.value) return t("export.pwRequired");
  if (password.value !== confirm.value) return t("export.pwMismatch");
  return "";
});
const valid = computed(() => pages.value !== undefined && !pwError.value);

const options = computed<ExportOptions>(() => ({
  format: format.value,
  quality: quality.value,
  pages: pages.value ?? null,
  dpi: dpi.value,
  protection: protect.value && isPdf.value ? { password: password.value, allowPrint: allowPrint.value, allowCopy: allowCopy.value } : null,
}));

const stem = computed(() => props.tab.name.replace(/\.pdf$/i, ""));
const fileName = computed(() => {
  if (kind.value === "images") return `${stem.value}.${imageType.value}`;
  return kind.value === "flattened" ? `${stem.value} (${t("export.flatSuffix")}).pdf` : `${stem.value}.pdf`;
});

// Estimation de taille (formats PDF), après une courte pause.
const estimate = ref<number | null>(null);
const original = ref<number | null>(null);
let timer = 0;
let seq = 0;
watch(
  () => [JSON.stringify({ ...options.value, protection: null }), valid.value],
  () => {
    window.clearTimeout(timer);
    estimate.value = null;
    if (!isPdf.value || pages.value === undefined || !props.tab.info) return;
    const id = ++seq;
    timer = window.setTimeout(async () => {
      const r = await commands.exportEstimate(props.tab.info!.id, { ...options.value, protection: null }).catch(() => null);
      if (id === seq && r?.status === "ok") estimate.value = r.data.size;
    }, 300);
  },
  { immediate: true },
);
onMounted(async () => {
  if (!props.tab.info) return;
  const r = await commands.exportEstimate(props.tab.info.id, { format: "pdf", quality: "max", pages: null, dpi: 72, protection: null }).catch(() => null);
  if (r?.status === "ok") original.value = r.data.size;
  await nextTick();
  card.value?.querySelector<HTMLElement>(".rc.on")?.focus();
});

async function pickPath(): Promise<string | null> {
  const dir = props.tab.path.replace(/[^\\/]*$/, "");
  const defaultPath = dir + fileName.value;
  if (!inTauri) return (window as unknown as { __FEUILLET_E2E__?: { exportPath?: string } }).__FEUILLET_E2E__?.exportPath ?? null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const ext = kind.value === "images" ? imageType.value : "pdf";
  return save({ defaultPath, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
}

async function run() {
  if (!valid.value || busy.value || !props.tab.info) return;
  const path = await pickPath();
  if (!path) return;
  busy.value = true;
  try {
    const r = await unwrap(commands.exportDocument(props.tab.info.id, options.value, path));
    const name = (r.files[0] ?? path).split(/[\\/]/).pop() ?? path;
    ui.notify(r.files.length > 1 ? t("export.doneImages", { n: r.files.length }) : t("export.done", { name }));
    emit("close");
  } catch (e) {
    ui.notify(t("export.failed", { msg: e instanceof BackendError && "message" in e.error ? e.error.message : String(e) }));
  } finally {
    busy.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    emit("close");
  }
}

const qualityLabel = computed(() => t(`export.quality.${quality.value}`));
</script>

<template>
  <div class="scrim" @mousedown.self="emit('close')">
    <div ref="card" class="modal" role="dialog" aria-modal="true" aria-labelledby="exp-title" @keydown="onKey">
      <div class="mhd">
        <h3 id="exp-title">{{ t("export.title", { name: stem }) }}</h3>
        <button class="cl" style="width: 28px; height: 28px" :aria-label="t('sig.close')" @click="emit('close')"><X class="ic s" aria-hidden="true" /></button>
      </div>
      <div class="mbd">
        <div class="fs" role="radiogroup" :aria-label="t('export.format')">
          <span class="lbl">{{ t("export.format") }}</span>
          <button v-for="k in (['pdf', 'flattened', 'images'] as const)" :key="k" class="rc" :class="{ on: kind === k }" role="radio" :aria-checked="kind === k" @click="kind = k">
            <span class="radio" :class="{ on: kind === k }" aria-hidden="true" />
            <span class="grow"><b>{{ t(`export.kind.${k}`) }}</b><small>{{ t(`export.kindHint.${k}`) }}</small></span>
            <span v-if="k === 'images'" class="row" :class="{ dimmed: kind !== 'images' }" @click.stop="kind = 'images'">
              <span class="seg">
                <span v-for="it in (['png', 'jpg'] as const)" :key="it" class="sb" :class="{ on: imageType === it }" role="button" :aria-pressed="imageType === it" tabindex="0" @click="imageType = it" @keydown.enter="imageType = it">{{ it.toUpperCase() }}</span>
              </span>
              <select v-model.number="dpi" class="dpi" :aria-label="t('export.dpi')" @click.stop>
                <option v-for="d in [72, 150, 300]" :key="d" :value="d">{{ t("export.dpiValue", { n: d }) }}</option>
              </select>
            </span>
          </button>
        </div>

        <div v-if="isPdf || imageType === 'jpg'" class="fs">
          <span class="lbl">
            {{ t("export.imageQuality") }}
            <span v-if="isPdf && estimate !== null">≈ {{ formatSize(estimate, locale) }}<template v-if="original !== null"> · {{ t("export.original", { size: formatSize(original, locale) }) }}</template></span>
          </span>
          <input v-model.number="qualityIndex" class="range" type="range" min="0" max="2" step="1" :aria-label="t('export.compression')" :aria-valuetext="qualityLabel" />
          <div class="sl3">
            <span :class="{ cur: qualityIndex === 0 }">{{ t("export.quality.light") }}</span>
            <span :class="{ cur: qualityIndex === 1 }">{{ t("export.quality.balanced") }}</span>
            <span :class="{ cur: qualityIndex === 2 }">{{ t("export.quality.max") }}</span>
          </div>
        </div>

        <div class="fs">
          <span class="lbl">{{ t("export.pages") }}</span>
          <div class="row">
            <div class="seg" role="radiogroup" :aria-label="t('export.pages')">
              <button role="radio" :aria-checked="pagesMode === 'all'" :class="{ on: pagesMode === 'all' }" @click="pagesMode = 'all'">{{ t("export.allPages", { n: count }) }}</button>
              <button role="radio" :aria-checked="pagesMode === 'current'" :class="{ on: pagesMode === 'current' }" @click="pagesMode = 'current'">{{ t("export.currentPage") }}</button>
              <button role="radio" :aria-checked="pagesMode === 'range'" :class="{ on: pagesMode === 'range' }" @click="pagesMode = 'range'; nextTick(() => card?.querySelector<HTMLInputElement>('.rinp')?.focus())">{{ t("export.range") }}</button>
            </div>
            <input v-if="pagesMode === 'range'" v-model="range" class="rinp" :class="{ bad: range && pages === undefined }" :placeholder="t('export.rangeHint')" :aria-label="t('export.rangeLabel')" :aria-invalid="!!range && pages === undefined" />
          </div>
        </div>

        <template v-if="isPdf">
          <div class="hr" />
          <div class="fs">
            <button class="row swrow" role="switch" :aria-checked="protect" @click="protect = !protect">
              <span class="sw-t" :class="{ on: protect }" aria-hidden="true" />{{ t("export.protect") }}
            </button>
            <template v-if="protect">
              <div class="g2">
                <label class="fs" style="gap: 6px">
                  <span class="lbl">{{ t("export.password") }}</span>
                  <span class="inp">
                    <input v-model="password" :type="showPw ? 'text' : 'password'" autocomplete="new-password" :aria-label="t('export.password')" />
                    <button type="button" :aria-label="showPw ? t('export.hidePw') : t('export.showPw')" @click="showPw = !showPw"><component :is="showPw ? EyeOff : Eye" class="ic xs" aria-hidden="true" /></button>
                  </span>
                </label>
                <label class="fs" style="gap: 6px">
                  <span class="lbl">{{ t("export.confirm") }}</span>
                  <span class="inp">
                    <input v-model="confirm" :type="showPw ? 'text' : 'password'" autocomplete="new-password" :aria-label="t('export.confirm')" />
                    <Check v-if="confirm && confirm === password" class="ic xs okc" aria-hidden="true" />
                  </span>
                </label>
              </div>
              <div class="row strength">
                <span class="meter" aria-hidden="true"><i v-for="n in 4" :key="n" :class="{ f: n <= strength }" /></span>
                <span>{{ pwError || t(`export.strength.${strength}`) }}</span>
              </div>
              <div class="row perms">
                <button class="row ckrow" role="checkbox" :aria-checked="allowPrint" @click="allowPrint = !allowPrint"><span class="cb" :class="{ on: allowPrint }"><Check v-if="allowPrint" class="ic xs" aria-hidden="true" /></span>{{ t("export.allowPrint") }}</button>
                <button class="row ckrow" role="checkbox" :aria-checked="allowCopy" @click="allowCopy = !allowCopy"><span class="cb" :class="{ on: allowCopy }"><Check v-if="allowCopy" class="ic xs" aria-hidden="true" /></span>{{ t("export.allowCopy") }}</button>
              </div>
            </template>
          </div>
        </template>
      </div>
      <div class="mft">
        <button class="btn gh print" @click="(emit('close'), printTab(tab))"><Printer class="ic s" aria-hidden="true" />{{ t("export.print") }}</button>
        <span class="muted grow fname">{{ fileName }}</span>
        <button class="btn gh" @click="emit('close')">{{ t("save.cancel") }}</button>
        <button class="btn pri" :disabled="!valid || busy" @click="run">{{ t("export.export") }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scrim { position: fixed; inset: 0; z-index: 100; background: var(--scrim); display: flex; align-items: center; justify-content: center; padding: 16px; }
.modal { width: 560px; max-width: 100%; max-height: 100%; overflow: auto; background: var(--surface); border-radius: 14px; box-shadow: 0 0 0 1px var(--line), var(--shadow-lg); display: flex; flex-direction: column; }
.mhd { display: flex; align-items: center; gap: 10px; padding: 16px 14px 2px 22px; }
.mhd h3 { margin: 0; font-size: 15px; font-weight: 600; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mbd { padding: 12px 22px 18px; display: flex; flex-direction: column; gap: 16px; }
.mft { display: flex; align-items: center; gap: 8px; padding: 12px 16px 12px 22px; border-top: 1px solid var(--line); background: var(--surface-2); }
:root.dark .mft { background: rgba(0, 0, 0, .12); }
.fs { display: flex; flex-direction: column; gap: 8px; }
.lbl { display: flex; align-items: center; justify-content: space-between; font-size: 12px; font-weight: 600; color: var(--text-2); }
.lbl span { font-weight: 400; color: var(--text-3); }
.rc { display: flex; align-items: flex-start; gap: 12px; padding: 10px 12px; border-radius: 10px; border: 0; background: transparent; box-shadow: inset 0 0 0 1px var(--line-2); text-align: left; font: inherit; color: var(--text); }
.rc.on { box-shadow: inset 0 0 0 2px var(--accent); background: var(--accent-soft); }
.rc:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.rc b { display: block; font-size: 13px; font-weight: 600; }
.rc small { display: block; font-size: 12px; color: var(--text-2); margin-top: 1px; line-height: 1.4; }
.radio { width: 16px; height: 16px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--line-2); background: var(--surface); flex: none; box-sizing: border-box; margin-top: 1px; }
.radio.on { box-shadow: inset 0 0 0 5px var(--accent); background: #fff; }
.grow { flex: 1; }
.row { display: flex; align-items: center; gap: 10px; }
.dimmed { opacity: .5; }
.seg .sb, .seg > button { display: flex; align-items: center; height: 26px; padding: 0 10px; border-radius: 7px; border: 0; background: transparent; color: var(--text-2); font: inherit; font-size: 12.5px; font-weight: 500; cursor: pointer; }
.seg .sb.on, .seg > button.on { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--line), 0 1px 2px rgba(0, 0, 0, .08); }
.dpi { height: 26px; border-radius: 7px; border: 0; background: var(--hover); color: var(--text); font: inherit; font-size: 12.5px; padding: 0 6px; }
.range { width: calc(100% - 12px); margin: 4px 6px 0; accent-color: var(--accent); }
.sl3 { display: flex; justify-content: space-between; font-size: 11.5px; color: var(--text-3); }
.sl3 .cur { color: var(--text); font-weight: 500; }
.rinp { height: 30px; flex: 1; min-width: 0; border-radius: 7px; border: 0; box-shadow: inset 0 0 0 1px var(--line-2); background: var(--bg); color: var(--text); padding: 0 10px; font: inherit; font-size: 13px; }
.rinp:focus { outline: none; box-shadow: inset 0 0 0 1px var(--accent), var(--ring); }
.rinp.bad { box-shadow: inset 0 0 0 1.5px var(--bad); }
.hr { height: 1px; background: var(--line); }
.swrow, .ckrow { border: 0; background: transparent; padding: 0; font: inherit; color: var(--text); text-align: left; cursor: pointer; }
.swrow { font-size: 13px; font-weight: 500; }
.ckrow { gap: 8px; font-size: 12.5px; }
.swrow:focus-visible, .ckrow:focus-visible { box-shadow: var(--ring); border-radius: 6px; }
.sw-t { position: relative; width: 30px; height: 18px; border-radius: 9px; background: var(--line-2); flex: none; }
.sw-t::after { content: ""; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: #fff; box-shadow: 0 1px 2px rgba(0, 0, 0, .25); transition: left .15s; }
.sw-t.on { background: var(--accent); }
.sw-t.on::after { left: 14px; }
.g2 { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.inp { display: flex; align-items: center; gap: 8px; height: 32px; border-radius: 7px; box-shadow: inset 0 0 0 1px var(--line-2); background: var(--bg); padding: 0 6px 0 10px; box-sizing: border-box; }
.inp:focus-within { box-shadow: inset 0 0 0 1px var(--accent), var(--ring); }
.inp input { flex: 1; min-width: 0; border: 0; background: transparent; color: var(--text); font: inherit; font-size: 13px; outline: none; }
.inp button { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; border: 0; background: transparent; color: var(--text-3); padding: 0; }
.okc { color: var(--ok); }
.strength { font-size: 12px; color: var(--text-2); }
.meter { display: flex; gap: 4px; align-items: center; }
.meter i { display: block; width: 44px; height: 4px; border-radius: 2px; background: var(--line-2); }
.meter i.f { background: var(--ok); }
.perms { gap: 20px; }
.cb { display: grid; place-items: center; width: 16px; height: 16px; border-radius: 4px; box-shadow: inset 0 0 0 1.5px var(--line-2); background: var(--surface); flex: none; color: #fff; }
.cb.on { background: var(--accent); box-shadow: none; }
.muted { color: var(--text-2); font-size: 12.5px; }
.fname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.print { padding: 0 10px; }
.btn:disabled { opacity: .5; }
</style>
