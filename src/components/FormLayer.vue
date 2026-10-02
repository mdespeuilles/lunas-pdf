<script setup lang="ts">
// Champs de formulaire d'une page (planche 04) : saisie au-dessus du rendu PDFium. Hors
// focus, les contrôles sont transparents et laissent voir l'apparence générée par le moteur.
import { computed, nextTick, onMounted, reactive, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { FormField, Widget } from "../bindings";
import { fillable, stops, useForm } from "../composables/form";
import { type Parsed, allowInput, editText, parseInput } from "../lib/field-format";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import type { DocTab } from "../stores/tabs";

const props = defineProps<{ tab: DocTab; page: number; k: number }>();
const settings = useSettings();
const form = useForm();
const ui = useUi();
const { t, locale } = useI18n();

const FONT_CSS = { sans: "Helvetica, Arial, 'Liberation Sans', sans-serif", serif: "'Times New Roman', Times, 'Liberation Serif', serif", mono: "'Courier New', Courier, 'Liberation Mono', monospace" } as const;
/** Mêmes valeurs que `form.rs` (interligne, taille automatique). */
const LINE_HEIGHT = 1.15;
const PAD = 3;

type Control = "text" | "multiline" | "check" | "select" | "combo";
interface Item {
  key: string;
  field: FormField;
  widget: Widget;
  control: Control;
  stop: boolean;
}

const fields = computed(() => props.tab.edit?.fields ?? []);
const stopOf = computed(() => new Map(stops(fields.value).map((s) => [s.field.id, s.widget])));
const items = computed<Item[]>(() => {
  const out: Item[] = [];
  for (const field of fields.value) {
    if (!fillable(field)) continue;
    const kind = field.kind;
    const control: Control =
      kind.type === "text" ? (kind.multiline ? "multiline" : "text")
      : kind.type === "checkbox" || kind.type === "radio" ? "check"
      : kind.type === "combo" && kind.editable ? "combo"
      : "select";
    field.widgets.forEach((widget, i) => {
      if (widget.page !== props.page) return;
      out.push({ key: `${field.id}#${i}`, field, widget, control, stop: stopOf.value.get(field.id) === widget });
    });
  }
  return out;
});

/** Saisie en cours (non encore validée) par champ, et champs dont le rendu se met à jour. */
const drafts = reactive<Record<string, string>>({});
const pending = reactive(new Set<string>());

function textOf(f: FormField) {
  return drafts[f.id] ?? editText(f, f.value[0] ?? "");
}

function box(w: Widget) {
  const k = props.k;
  return { left: `${w.rect.x * k}px`, top: `${w.rect.y * k}px`, width: `${w.rect.w * k}px`, height: `${w.rect.h * k}px` };
}

function textStyle(it: Item) {
  const f = it.field;
  const h = it.widget.rect.h;
  const size = f.fontSize > 0 ? f.fontSize : it.control === "multiline" ? 12 : Math.min(12, Math.max(4, (h - 4) / LINE_HEIGHT));
  return {
    ...box(it.widget),
    fontFamily: FONT_CSS[f.font],
    fontSize: `${size * props.k}px`,
    lineHeight: String(LINE_HEIGHT),
    textAlign: (["left", "center", "right"] as const)[f.align] ?? "left",
    padding: it.control === "multiline" ? `${PAD * props.k}px ${PAD * props.k}px` : `0 ${PAD * props.k}px`,
  };
}

async function commit(f: FormField, value: string[]) {
  pending.add(f.id);
  try {
    await form.setValue(props.tab, f.id, value);
  } finally {
    delete drafts[f.id];
    // Laisse le temps au nouveau rendu d'arriver avant de redevenir transparent.
    setTimeout(() => pending.delete(f.id), 400);
  }
}

function errorText(f: FormField, r: Exclude<Parsed, { ok: true }>) {
  const num = (n: string | number) => (n === "" ? "" : new Intl.NumberFormat(locale.value).format(Number(n)));
  const name = label(f);
  if (r.error !== "range") return t(`form.errors.${r.error}`, { field: name, ...r.params });
  const { min, max } = r.params;
  const key = min !== "" && max !== "" ? "rangeBoth" : min !== "" ? "rangeMin" : "rangeMax";
  return t(`form.errors.${key}`, { field: name, min: num(min), max: num(max) });
}

/** Valide la saisie en cours (format, plage) ; une saisie refusée est annulée et signalée. */
function commitText(f: FormField, el?: EventTarget | null) {
  const v = drafts[f.id];
  if (v === undefined) return;
  const input = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement ? el : null;
  const restore = () => {
    delete drafts[f.id];
    if (input) input.value = editText(f, f.value[0] ?? "");
  };
  const r = parseInput(f, v);
  if (!r.ok) {
    ui.notify(errorText(f, r));
    restore();
  } else if (r.value === (f.value[0] ?? "")) {
    restore();
  } else {
    void commit(f, [r.value]);
  }
}

function onBeforeInput(e: InputEvent, f: FormField) {
  if (e.data && !allowInput(f, e.data)) e.preventDefault();
}

function toggle(it: Item) {
  const on = it.widget.onState;
  if (!on) return;
  const checked = it.field.value.includes(on);
  if (it.field.kind.type === "radio") {
    if (!checked) void commit(it.field, [on]);
  } else {
    void commit(it.field, checked ? [] : [on]);
  }
}

function onSelect(f: FormField, e: Event) {
  const el = e.target as HTMLSelectElement;
  const v = Array.from(el.selectedOptions, (o) => o.value).filter((x) => x !== "");
  void commit(f, v);
}

function options(f: FormField) {
  return f.kind.type === "combo" || f.kind.type === "list" ? f.kind.options : [];
}

function onKey(e: KeyboardEvent, it: Item) {
  if (e.key === "Tab" && !e.ctrlKey && !e.altKey) {
    e.preventDefault();
    if (it.control === "text" || it.control === "multiline" || it.control === "combo") commitText(it.field, e.target);
    form.step(props.tab, e.shiftKey ? -1 : 1);
  } else if (e.key === "Escape") {
    delete drafts[it.field.id];
    const el = e.target as HTMLInputElement;
    if ("value" in el && it.control !== "select") el.value = editText(it.field, it.field.value[0] ?? "");
    el.blur();
  }
}

function label(f: FormField) {
  return f.label ?? f.id;
}

// Focus demandé par la navigation (Tab, pastille) une fois la page affichée.
function applyFocus() {
  const req = props.tab.formFocus;
  if (!req || !items.value.some((it) => it.stop && it.field.id === req.id)) return;
  void nextTick(() => {
    document.querySelector<HTMLElement>(`[data-field-id="${CSS.escape(req.id)}"][data-stop]`)?.focus();
    props.tab.formFocus = null;
  });
}
watch(() => props.tab.formFocus?.seq, applyFocus);
onMounted(applyFocus);
</script>

<template>
  <div class="flayer" :class="{ hlf: settings.settings.highlightFields }">
    <template v-for="it in items" :key="it.key">
      <textarea
        v-if="it.control === 'multiline'"
        class="ff"
        :class="{ pending: pending.has(it.field.id) }"
        :style="textStyle(it)"
        :value="textOf(it.field)"
        :aria-label="label(it.field)"
        :aria-required="it.field.required || undefined"
        :maxlength="it.field.kind.type === 'text' && it.field.kind.maxLen ? it.field.kind.maxLen : undefined"
        :data-field-id="it.field.id"
        :data-stop="it.stop || undefined"
        spellcheck="false"
        @input="drafts[it.field.id] = ($event.target as HTMLTextAreaElement).value"
        @beforeinput="onBeforeInput($event as InputEvent, it.field)"
        @change="commitText(it.field, $event.target)"
        @focus="tab.focusedField = it.field.id"
        @keydown="onKey($event, it)"
      />
      <template v-else-if="it.control === 'text' || it.control === 'combo'">
        <input
          class="ff"
          :class="{ pending: pending.has(it.field.id), comb: it.field.kind.type === 'text' && it.field.kind.comb }"
          :style="textStyle(it)"
          :type="it.field.kind.type === 'text' && it.field.kind.password ? 'password' : 'text'"
          :value="textOf(it.field)"
          :aria-label="label(it.field)"
          :aria-required="it.field.required || undefined"
          :maxlength="it.field.kind.type === 'text' && it.field.kind.maxLen ? it.field.kind.maxLen : undefined"
          :list="it.control === 'combo' ? `dl-${it.key}` : undefined"
          :data-field-id="it.field.id"
          :data-stop="it.stop || undefined"
          spellcheck="false"
          autocomplete="off"
          @input="drafts[it.field.id] = ($event.target as HTMLInputElement).value"
          @beforeinput="onBeforeInput($event as InputEvent, it.field)"
        @change="commitText(it.field, $event.target)"
          @focus="tab.focusedField = it.field.id"
          @keydown="onKey($event, it)"
        />
        <datalist v-if="it.control === 'combo'" :id="`dl-${it.key}`">
          <option v-for="o in options(it.field)" :key="o.value" :value="o.value">{{ o.label }}</option>
        </datalist>
      </template>
      <button
        v-else-if="it.control === 'check'"
        class="ff btn"
        :style="box(it.widget)"
        :role="it.field.kind.type === 'radio' ? 'radio' : 'checkbox'"
        :aria-checked="!!it.widget.onState && it.field.value.includes(it.widget.onState)"
        :aria-label="it.field.kind.type === 'radio' ? `${label(it.field)} : ${it.widget.onState}` : label(it.field)"
        :data-field-id="it.field.id"
        :data-stop="it.stop || undefined"
        @click="toggle(it)"
        @focus="tab.focusedField = it.field.id"
        @keydown="onKey($event, it)"
      />
      <span v-else class="ff sel" :style="box(it.widget)">
        <select
          :multiple="it.field.kind.type === 'list' && it.field.kind.multi"
          :size="it.field.kind.type === 'list' ? Math.max(2, options(it.field).length) : undefined"
          :aria-label="label(it.field)"
          :aria-required="it.field.required || undefined"
          :data-field-id="it.field.id"
          :data-stop="it.stop || undefined"
          @change="onSelect(it.field, $event)"
          @focus="tab.focusedField = it.field.id"
          @keydown="onKey($event, it)"
        >
          <option v-if="it.field.kind.type === 'combo'" value="" :selected="!it.field.value.length" />
          <option v-for="o in options(it.field)" :key="o.value" :value="o.value" :selected="it.field.value.includes(o.value)">{{ o.label }}</option>
        </select>
      </span>
    </template>
  </div>
</template>

<style scoped>
.flayer { position: absolute; inset: 0; z-index: 3; pointer-events: none; }
.ff {
  position: absolute; box-sizing: border-box; margin: 0; border: 0; border-radius: 2px; pointer-events: auto;
  background: transparent; color: transparent; caret-color: transparent; outline: none; resize: none; overflow: hidden;
}
.hlf .ff { background: color-mix(in oklab, var(--accent) 16%, transparent); }
.ff:hover { box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent) 60%, transparent); }
input.ff:focus, textarea.ff:focus, .ff.pending { background: #fff; color: #1b1b20; caret-color: #1b1b20; }
.ff:focus, .ff:focus-within {
  z-index: 1; border-radius: 3px;
  box-shadow: 0 0 0 2px var(--accent), 0 0 0 6px color-mix(in oklab, var(--accent) 22%, transparent);
}
.ff.btn { padding: 0; cursor: pointer; }
.ff.btn:focus { background: transparent; }
.ff.comb { letter-spacing: .3em; }
.sel select { position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0; cursor: pointer; }
.sel option { color: #1b1b20; background: #fff; }
</style>
