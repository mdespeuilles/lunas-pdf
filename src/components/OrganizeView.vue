<script setup lang="ts">
// « Organiser les pages » (planche 08) : barre d'actions, grille, côte à côte, glisser-déposer
// (déplacement dans un document, copie d'un document à l'autre).
import { ArrowLeft, ChevronDown, Columns2, Copy, FilePlus2, FileOutput, FileText, FolderOpen, ImageMinus, ImagePlus, RotateCcw, RotateCw, Square, Trash2 } from "lucide-vue-next";
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import Dropdown from "./Dropdown.vue";
import PageGrid from "./PageGrid.vue";
import ThumbCanvas from "./ThumbCanvas.vue";
import { useOrganize } from "../composables/organize";
import { openBeside } from "../composables/open";
import { sideDrop } from "../composables/page-drag";
import { isNoop } from "../lib/page-order";
import { pickPdfFiles } from "../lib/window";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const tabs = useTabs();
const org = useOrganize();

const size = ref(116);
const side = computed(() => (props.tab.orgSide ? tabs.tabs.find((x) => x.key === props.tab.orgSide && x.status === "ready") ?? null : null));
/** Volet visé par les actions de la barre (dernier volet utilisé). */
const activeKey = ref(props.tab.key);
const active = computed(() => (side.value && activeKey.value === side.value.key ? side.value : props.tab));
const others = computed(() => tabs.tabs.filter((x) => x.key !== props.tab.key && x.status === "ready"));
watch(side, (s) => {
  if (!s) activeKey.value = props.tab.key;
});

function done() {
  tabs.toggleOrganizing(props.tab, false);
}

function openPage(tab: DocTab, i: number) {
  if (tab.key !== props.tab.key) return;
  props.tab.orgSel = [i];
  done();
}

async function showSide(key: string | null) {
  props.tab.orgSide = key;
  if (key) {
    const s = tabs.tabs.find((x) => x.key === key);
    if (s && !s.orgSel.length) s.orgSel = [];
  }
}

async function openSide() {
  const e2e = (window as unknown as { __LUNAS_PDF_E2E__?: { sidePath?: string } }).__LUNAS_PDF_E2E__;
  const paths = e2e ? (e2e.sidePath ? [e2e.sidePath] : []) : await pickPdfFiles(t("organize.openSide"));
  if (paths.length) await openBeside(props.tab, paths);
}

// --- Sélection et glisser-déposer -------------------------------------------------------------
interface Press {
  tab: DocTab;
  index: number;
  x: number;
  y: number;
  ctrl: boolean;
  shift: boolean;
}
interface Drag {
  src: DocTab;
  pages: number[];
  x: number;
  y: number;
  target: { tab: DocTab; at: number } | null;
}
const press = ref<Press | null>(null);
const drag = ref<Drag | null>(null);
const anchors = new Map<string, number>();

function onPress(tab: DocTab, index: number, e: PointerEvent) {
  if (e.button !== 0) return;
  activeKey.value = tab.key;
  press.value = { tab, index, x: e.clientX, y: e.clientY, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey };
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp, { once: true });
}

/** Clic sans glisser : sélection simple, Ctrl (basculer), Maj (plage). */
function click(p: Press) {
  const tab = p.tab;
  if (p.shift) {
    const a = anchors.get(tab.key) ?? p.index;
    const [lo, hi] = [Math.min(a, p.index), Math.max(a, p.index)];
    tab.orgSel = Array.from({ length: hi - lo + 1 }, (_, k) => lo + k);
    return;
  }
  anchors.set(tab.key, p.index);
  if (p.ctrl) tab.orgSel = tab.orgSel.includes(p.index) ? tab.orgSel.filter((i) => i !== p.index) : [...tab.orgSel, p.index];
  else tab.orgSel = [p.index];
}

function hit(x: number, y: number): Drag["target"] {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  const pane = el?.closest<HTMLElement>("[data-pane]");
  const tab = pane ? tabs.tabs.find((t) => t.key === pane.dataset.pane) : undefined;
  if (!pane || !tab?.info) return null;
  const cell = el!.closest<HTMLElement>(".pt[data-index]");
  if (!cell) return { tab, at: tab.info.pages.length };
  const r = cell.getBoundingClientRect();
  const i = Number(cell.dataset.index);
  return { tab, at: x < r.left + r.width / 2 ? i : i + 1 };
}

let scrollTimer = 0;
function autoScroll(x: number, y: number) {
  window.clearInterval(scrollTimer);
  const pane = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>("[data-pane]");
  if (!pane) return;
  const r = pane.getBoundingClientRect();
  const dir = y < r.top + 48 ? -1 : y > r.bottom - 48 ? 1 : 0;
  if (dir) scrollTimer = window.setInterval(() => (pane.scrollTop += dir * 14), 16);
}

function onMove(e: PointerEvent) {
  const p = press.value;
  if (!p) return;
  if (!drag.value) {
    if (Math.hypot(e.clientX - p.x, e.clientY - p.y) < 6) return;
    // Une page non sélectionnée se déplace seule.
    if (!p.tab.orgSel.includes(p.index)) p.tab.orgSel = [p.index];
    drag.value = { src: p.tab, pages: [...p.tab.orgSel].sort((a, b) => a - b), x: e.clientX, y: e.clientY, target: null };
  }
  const d = drag.value;
  d.x = e.clientX;
  d.y = e.clientY;
  d.target = hit(e.clientX, e.clientY);
  autoScroll(e.clientX, e.clientY);
}

async function onUp() {
  window.removeEventListener("pointermove", onMove);
  window.clearInterval(scrollTimer);
  const p = press.value;
  const d = drag.value;
  press.value = null;
  drag.value = null;
  if (!p) return;
  if (!d) return click(p);
  const target = d.target;
  if (!target || !d.src.info) return;
  if (target.tab.key === d.src.key) {
    if (!isNoop(d.src.info.pages.length, d.pages, target.at)) await org.move(d.src, d.pages, target.at);
  } else {
    activeKey.value = target.tab.key;
    await org.copyPages(target.tab, d.src, d.pages, target.at);
  }
}
onBeforeUnmount(() => {
  window.removeEventListener("pointermove", onMove);
  window.clearInterval(scrollTimer);
});

const pill = computed(() => {
  const d = drag.value;
  if (!d?.target) return "";
  const copy = d.target.tab.key !== d.src.key;
  const n = d.target.tab.info?.pages.length ?? 0;
  if (d.target.at >= n) return t(copy ? "organize.copyEnd" : "organize.moveEnd");
  return t(copy ? "organize.copyBefore" : "organize.moveBefore", { n: d.target.at + 1 });
});

function dropAt(tab: DocTab) {
  const d = drag.value;
  return d?.target?.tab.key === tab.key ? d.target.at : null;
}

function draggingIn(tab: DocTab) {
  const d = drag.value;
  return d && d.src.key === tab.key ? d.pages : [];
}

const selCount = computed(() => active.value.orgSel.length);
</script>

<template>
  <div class="org">
    <div class="toolbar" role="toolbar" :aria-label="t('organize.title')">
      <button class="btn gh back" @click="done"><ArrowLeft class="ic s" aria-hidden="true" />{{ t("organize.reading") }}</button>
      <div class="sep" />
      <span class="ttl">{{ t("organize.title") }}</span>
      <span v-if="selCount" class="selc" aria-live="polite">{{ t("organize.selected", { n: selCount }, selCount) }}</span>
      <div class="sep" />
      <button class="tb" :disabled="!selCount" :aria-label="t('organize.rotateLeft')" :title="t('organize.rotateLeft')" @click="org.rotate(active, -90)"><RotateCcw class="ic" aria-hidden="true" /></button>
      <button class="tb" :disabled="!selCount" :aria-label="t('organize.rotateRight')" :title="t('organize.rotateRight')" @click="org.rotate(active, 90)"><RotateCw class="ic" aria-hidden="true" /></button>
      <button class="tb" :disabled="!selCount" :aria-label="t('organize.duplicate')" :title="t('organize.duplicate')" @click="org.duplicate(active)"><Copy class="ic" aria-hidden="true" /></button>
      <button class="tb" :aria-label="t('organize.blank')" :title="t('organize.blank')" @click="org.insertBlank(active)"><FilePlus2 class="ic" aria-hidden="true" /></button>
      <button class="tb" :disabled="!selCount" :aria-label="t('organize.extract')" :title="t('organize.extract')" @click="org.extract(active)"><FileOutput class="ic" aria-hidden="true" /></button>
      <button class="tb" :disabled="!selCount" :aria-label="t('organize.delete')" :title="t('organize.delete')" @click="org.remove(active)"><Trash2 class="ic" aria-hidden="true" /></button>
      <div class="grow" />
      <ImageMinus class="ic s dim" aria-hidden="true" />
      <input v-model.number="size" class="range" type="range" min="72" max="200" step="4" :aria-label="t('organize.thumbSize')" />
      <ImagePlus class="ic s dim" aria-hidden="true" />
      <div class="sep" />
      <div class="seg">
        <button :class="{ on: !side }" :aria-pressed="!side" :aria-label="t('organize.single')" :title="t('organize.single')" @click="showSide(null)"><Square class="ic s" aria-hidden="true" /></button>
        <Dropdown align="right" :width="260">
          <template #trigger="{ toggle }">
            <button class="sidebtn" :class="{ on: !!side }" :aria-pressed="!!side" aria-haspopup="menu" @click="others.length ? toggle(true) : openSide()">
              <Columns2 class="ic s" aria-hidden="true" />{{ t("organize.sideBySide") }}<ChevronDown class="ic xs chev" aria-hidden="true" />
            </button>
          </template>
          <button v-for="o in others" :key="o.key" class="mi" role="menuitem" :class="{ on: side?.key === o.key }" @click="showSide(o.key)">{{ o.name }}</button>
          <div class="msep" />
          <button class="mi" role="menuitem" @click="openSide">{{ t("organize.openSide") }}</button>
        </Dropdown>
      </div>
      <button class="btn pri done" @click="done">{{ t("organize.done") }}</button>
    </div>
    <div class="body">
      <section class="pane" :class="{ act: side && activeKey === tab.key }" :aria-label="tab.name">
        <div class="paneh"><b>{{ tab.name }}</b><span>· {{ t("organize.pageCount", { n: tab.info?.pages.length ?? 0 }, tab.info?.pages.length ?? 0) }}</span></div>
        <PageGrid :tab="tab" :size="size" :drop-at="dropAt(tab)" :dragging="draggingIn(tab)" @press="(i, e) => onPress(tab, i, e)" @open="(i) => openPage(tab, i)" @focus-pane="activeKey = tab.key" />
      </section>
      <!-- Invitation au côte à côte, dans l'espace libre (disparaît dès qu'un document est à côté). -->
      <aside v-if="!side" class="invite" :class="{ over: sideDrop === tab.key }" :data-side-drop="tab.key" :aria-label="t('organize.inviteTitle')">
        <div class="card">
          <span class="lic"><Columns2 class="ic l" aria-hidden="true" /></span>
          <b>{{ t("organize.inviteTitle") }}</b>
          <p>{{ t("organize.inviteText") }}</p>
          <div class="choices">
            <button v-for="o in others" :key="o.key" class="btn out" :title="o.path" @click="showSide(o.key)">
              <FileText class="ic s" aria-hidden="true" /><span class="nm">{{ o.name }}</span>
            </button>
            <button class="btn pri" @click="openSide"><FolderOpen class="ic s" aria-hidden="true" />{{ t("organize.inviteOpen") }}</button>
          </div>
          <span class="drop">{{ t("organize.inviteDrop") }}</span>
        </div>
      </aside>
      <template v-if="side">
        <div class="divider"><span /></div>
        <section class="pane r" :class="{ act: activeKey === side.key }" :aria-label="side.name">
          <div class="paneh">
            <b>{{ side.name }}</b><span>· {{ t("organize.pageCount", { n: side.info?.pages.length ?? 0 }, side.info?.pages.length ?? 0) }}</span>
            <span class="grow" />
            <button class="btn gh small" @click="showSide(null)">{{ t("organize.closeSide") }}</button>
          </div>
          <PageGrid :tab="side" :size="size" :drop-at="dropAt(side)" :dragging="draggingIn(side)" @press="(i, e) => onPress(side!, i, e)" @focus-pane="activeKey = side!.key" />
        </section>
      </template>
      <div v-if="drag" class="ghost" :style="{ left: drag.x + 12 + 'px', top: drag.y + 12 + 'px' }" aria-hidden="true">
        <div v-for="(p, k) in drag.pages.slice(0, 2).reverse()" :key="p" class="gp" :style="{ transform: `rotate(${k === 0 && drag.pages.length > 1 ? 5 : -3}deg)` }">
          <ThumbCanvas v-if="drag.src.info" :doc="drag.src.info.id" :page="p" :width="72" :height="Math.round((72 * drag.src.info.pages[p].height) / drag.src.info.pages[p].width)" :rev="drag.src.pageRev[p] ?? 0" />
        </div>
        <span v-if="drag.pages.length > 1" class="cnt">{{ drag.pages.length }}</span>
        <span v-if="pill" class="dpill">{{ pill }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.org { flex: 1; display: flex; flex-direction: column; min-height: 0; }
.toolbar { display: flex; align-items: center; gap: 4px; height: 46px; padding: 0 10px; background: var(--chrome); border-bottom: 1px solid var(--line); flex: none; }
.back { padding: 0 10px 0 6px; }
.ttl { font-size: 13px; font-weight: 600; padding: 0 4px; white-space: nowrap; }
.selc { display: inline-flex; align-items: center; height: 24px; padding: 0 9px; border-radius: 12px; background: var(--accent-soft); color: var(--accent-text); font-size: 12px; font-weight: 500; white-space: nowrap; }
.tb:disabled { opacity: .4; }
.grow { flex: 1; }
.dim { color: var(--text-3); }
.range { width: 110px; accent-color: var(--accent); }
.seg .on { background: var(--surface); color: var(--text); box-shadow: 0 0 0 1px var(--line), 0 1px 2px rgba(0, 0, 0, .08); }
.done { margin-left: 8px; }
.body { flex: 1; display: flex; min-height: 0; position: relative; }
.pane { flex: 1; display: flex; flex-direction: column; min-width: 0; background: var(--canvas); overflow: hidden; }
.pane.r { background: var(--canvas-2); }
.paneh { display: flex; align-items: center; gap: 8px; height: 44px; padding: 0 20px 0 28px; font-size: 12.5px; color: var(--text-2); flex: none; }
.paneh b { color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.pane.act .paneh b { color: var(--accent-text); }
.btn.small { height: 26px; padding: 0 8px; font-size: 12px; }
.divider { position: relative; width: 1px; background: var(--line-2); flex: none; }
.divider span { position: absolute; top: 50%; left: -3px; width: 7px; height: 40px; margin-top: -20px; border-radius: 4px; background: var(--surface); box-shadow: 0 0 0 1px var(--line-2); }
.ghost { position: fixed; z-index: 50; width: 110px; height: 130px; pointer-events: none; }
.gp { position: absolute; left: 0; top: 0; background: #fff; border-radius: 3px; overflow: hidden; box-shadow: 0 0 0 1px rgba(0, 0, 0, .1), 0 18px 40px rgba(0, 0, 0, .28); }
.cnt { position: absolute; top: -10px; left: 60px; display: grid; place-items: center; min-width: 24px; height: 24px; padding: 0 6px; box-sizing: border-box; border-radius: 12px; background: var(--accent); color: #fff; font-size: 12px; font-weight: 600; box-shadow: 0 0 0 2px #fff; }
.dpill { position: absolute; left: -6px; top: 118px; display: flex; align-items: center; gap: 6px; padding: 5px 10px; border-radius: 8px; background: var(--tip-bg); color: var(--tip-fg); font-size: 12px; white-space: nowrap; box-shadow: 0 6px 18px rgba(0, 0, 0, .25); }
/* Bouton dans le menu déroulant : hors de `.seg > button`, styles repris ici. */
.sidebtn { display: flex; align-items: center; gap: 6px; height: 26px; padding: 0 8px 0 9px; border-radius: 7px; border: 0; background: transparent; color: var(--text-2); font: inherit; font-size: 12.5px; font-weight: 500; white-space: nowrap; }
.sidebtn:hover { color: var(--text); }
.chev { opacity: .7; }
.invite { width: 320px; flex: none; display: flex; overflow-y: auto; padding: 24px; background: var(--canvas); }
/* Centrée verticalement sans jamais déborder (fenêtre basse : la zone défile). */
.invite .card { margin: auto 0; width: 100%; display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 28px 22px; border-radius: 14px; border: 1.5px dashed var(--line-2); text-align: center; transition: border-color .15s, background .15s; }
.invite.over .card { border-color: var(--accent); background: var(--accent-soft); }
.invite .lic { display: grid; place-items: center; width: 48px; height: 48px; border-radius: 14px; background: var(--accent-soft); color: var(--accent-text); margin-bottom: 4px; }
.invite b { font-size: 14px; font-weight: 600; color: var(--text); }
.invite p { margin: 0 0 6px; font-size: 12.5px; line-height: 1.5; color: var(--text-2); }
.invite .choices { display: flex; flex-direction: column; gap: 6px; width: 100%; }
.invite .choices .btn { width: 100%; justify-content: flex-start; min-width: 0; }
.invite .choices .btn.pri { justify-content: center; }
.invite .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.invite .drop { font-size: 11.5px; color: var(--text-3); margin-top: 2px; }
@media (max-width: 900px) { .invite { display: none; } }
.mi { display: flex; align-items: center; width: 100%; height: 30px; padding: 0 10px; border: 0; border-radius: 6px; background: transparent; font: inherit; font-size: 13px; color: var(--text); text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.mi:hover, .mi:focus-visible, .mi.on { background: var(--hover); outline: none; }
.msep { height: 1px; background: var(--line); margin: 5px 4px; }
</style>
