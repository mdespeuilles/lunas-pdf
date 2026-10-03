<script setup lang="ts">
// Panneau IA (volet de droite) : questions et actions sur le document de l'onglet. Les
// modifications de l'agent apparaissent tout de suite dans le document, annulables.
import { BookmarkPlus, Check, Sparkles, X } from "lucide-vue-next";
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { renderMarkdown } from "../lib/markdown";
import { useAi } from "../stores/ai";
import { useSettings } from "../stores/settings";
import { type DocTab, useTabs } from "../stores/tabs";

const props = defineProps<{ tab: DocTab }>();
const { t } = useI18n();
const ai = useAi();
const tabs = useTabs();
const settings = useSettings();

const input = ref("");
const inputEl = ref<HTMLTextAreaElement>();
const scroller = ref<HTMLElement>();
const chat = computed(() => ai.chats[props.tab.key] ?? []);
const busy = computed(() => ai.running(props.tab.key));
const textOnly = computed(() => settings.settings.aiAgent === "codex");

const SUGGESTIONS = ["summary", "fill", "dates", "check"] as const;

function send(q = input.value) {
  if (!q.trim() || busy.value) return;
  ai.ask(props.tab, q);
  input.value = "";
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
    e.preventDefault();
    send();
  } else if (e.key === "Escape") {
    e.stopPropagation();
    ai.toggle(props.tab, false);
  }
}

/** Lien vers une page dans une réponse : affiche la page. */
function onClick(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest<HTMLElement>("a.page-link");
  if (!a?.dataset.page) return;
  e.preventDefault();
  tabs.goto(props.tab, Number(a.dataset.page) - 1);
}

/** Résumé lisible d'un appel d'outil. */
function toolLabel(tool: { name: string; input: Record<string, unknown> }): string {
  const page = typeof tool.input.page === "number" ? tool.input.page : "?";
  const count = (k: string) => (Array.isArray(tool.input[k]) ? (tool.input[k] as unknown[]).length : 0);
  switch (tool.name) {
    case "get_page_text":
      return t("ai.tools.text", { page });
    case "view_page":
      return t("ai.tools.view", { page });
    case "list_form_fields":
      return t("ai.tools.fields");
    case "fill_form_fields":
      return t("ai.tools.fill", { n: count("fields") });
    case "list_annotations":
      return t("ai.tools.annots");
    case "add_annotations":
      return t("ai.tools.add", { n: count("items") });
    case "remove_annotations":
      return t("ai.tools.remove", { n: count("ids") });
    default:
      return tool.name;
  }
}

watch(
  () => chat.value.map((e) => e.text.length + e.tools.length).join(),
  () => nextTick(() => scroller.value?.scrollTo({ top: scroller.value.scrollHeight })),
);
onMounted(() => nextTick(() => inputEl.value?.focus()));
</script>

<template>
  <aside class="insp" :aria-label="t('ai.title')">
    <div class="inh">
      <Sparkles class="ic s spark" aria-hidden="true" />
      <span>{{ t("ai.title") }}</span>
      <div class="grow" />
      <button v-if="chat.length" class="btn gh sm" :disabled="busy" @click="ai.clear(tab.key)">{{ t("ai.new") }}</button>
      <button class="cl" style="width: 26px; height: 26px" :aria-label="t('ai.close')" :title="t('ai.close')" @click="ai.toggle(tab, false)">
        <X class="ic s" aria-hidden="true" />
      </button>
    </div>

    <div ref="scroller" class="scroll" @click="onClick">
      <div v-if="!chat.length" class="intro">
        <p>{{ t("ai.intro") }}</p>
        <div class="chips">
          <!-- Clés : ai.suggestions.summary, fill, dates, check -->
          <button v-for="s in SUGGESTIONS" :key="s" class="chip" @click="send(t(`ai.suggestions.${s}`))">{{ t(`ai.suggestions.${s}`) }}</button>
        </div>
        <p class="hint">{{ textOnly ? t("ai.hintTextOnly") : t("ai.hint") }}</p>
        <p v-if="ai.memory.length" class="hint">{{ t("ai.memory.count", { n: ai.memory.length }) }}</p>
      </div>

      <div v-for="(e, i) in chat" :key="i" class="entry" :class="e.role">
        <div v-if="e.role === 'user'" class="bubble">{{ e.text }}</div>
        <template v-else>
          <div v-if="e.tools.length" class="tools">
            <div v-for="(tool, j) in e.tools" :key="j" class="tool-line">↳ {{ toolLabel(tool) }}</div>
          </div>
          <div v-if="e.state === 'error'" class="error" role="alert">{{ e.text }}</div>
          <!-- eslint-disable-next-line vue/no-v-html -- HTML échappé par renderMarkdown -->
          <div v-else-if="e.text" class="answer" v-html="renderMarkdown(e.text)" />
          <div v-if="e.memory" class="mem" role="group" :aria-label="t('ai.memory.title')">
            <div class="mem-h"><BookmarkPlus class="ic xs" aria-hidden="true" /> {{ t("ai.memory.title") }}</div>
            <template v-if="e.memory.state === 'pending'">
              <label v-for="(f, j) in e.memory.add" :key="j" class="mem-f">
                <input v-model="e.memory.keep[j]" type="checkbox" />
                <span>{{ f }}</span>
              </label>
              <p class="mem-hint">{{ t("ai.memory.hint") }}</p>
              <div class="mem-a">
                <button class="btn gh sm" @click="e.memory.state = 'dismissed'">{{ t("ai.memory.dismiss") }}</button>
                <button class="btn pri sm" :disabled="!e.memory.keep.some(Boolean)" @click="ai.acceptMemory(e.memory)">{{ t("ai.memory.save") }}</button>
              </div>
            </template>
            <p v-else class="mem-done">
              <template v-if="e.memory.state === 'saved'"><Check class="ic xs" aria-hidden="true" /> {{ t("ai.memory.saved") }}</template>
              <template v-else>{{ t("ai.memory.dismissed") }}</template>
            </p>
          </div>
          <div v-if="e.state === 'running'" class="thinking">
            <span class="spinner" aria-hidden="true" /> {{ t("ai.thinking") }}
            <button class="link" @click="ai.stop(tab.key)">{{ t("ai.stop") }}</button>
          </div>
        </template>
      </div>
    </div>

    <form class="composer" @submit.prevent="send()">
      <textarea ref="inputEl" v-model="input" class="input" rows="3" :placeholder="t('ai.placeholder')" :aria-label="t('ai.placeholder')" @keydown="onKey" />
      <button type="submit" class="btn pri" :disabled="!input.trim() || busy">{{ t("ai.send") }}</button>
    </form>
  </aside>
</template>

<style scoped>
.insp { width: 400px; flex: none; background: var(--sidebar); border-left: 1px solid var(--line); display: flex; flex-direction: column; overflow: hidden; }
.inh { display: flex; align-items: center; gap: 8px; padding: 12px 12px 8px 16px; font-size: 13.5px; font-weight: 600; }
.spark { color: var(--accent); }
.grow { flex: 1; }
.btn.sm { height: 26px; padding: 0 10px; font-size: 12px; }
.scroll { flex: 1; min-height: 0; overflow: auto; padding: 8px 16px 16px; display: flex; flex-direction: column; gap: 14px; }
.intro { color: var(--text-2); font-size: 12.5px; line-height: 1.5; }
.intro p { margin: 0 0 10px; }
.hint { color: var(--text-3); font-size: 11.5px; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 12px; }
.chip { border: 0; box-shadow: 0 0 0 1px var(--line-2); background: var(--surface); color: var(--text-2); border-radius: 14px; padding: 4px 10px; font: inherit; font-size: 12px; text-align: left; }
.chip:hover { box-shadow: 0 0 0 1px var(--accent); color: var(--accent-text); }
.entry.user { align-self: flex-end; max-width: 85%; }
.bubble { background: var(--accent-soft); color: var(--text); border-radius: 10px; padding: 7px 11px; font-size: 12.5px; white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
.entry.assistant { display: flex; flex-direction: column; gap: 8px; }
.tools { display: flex; flex-direction: column; gap: 2px; }
.tool-line { font-size: 11.5px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.answer { font-size: 12.5px; line-height: 1.55; color: var(--text); overflow-wrap: anywhere; user-select: text; }
.answer :deep(p) { margin: 0 0 7px; }
.answer :deep(ul), .answer :deep(ol) { margin: 0 0 7px; padding-left: 18px; }
.answer :deep(li) { margin: 2px 0; }
.answer :deep(h4) { margin: 8px 0 4px; font-size: 12.5px; }
.answer :deep(code) { font: 11.5px ui-monospace, Menlo, monospace; background: var(--hover); padding: 0 3px; border-radius: 3px; }
.answer :deep(pre) { margin: 6px 0; padding: 8px 10px; background: var(--hover); border-radius: 6px; overflow-x: auto; }
.answer :deep(pre code) { padding: 0; background: none; white-space: pre; }
.answer :deep(table) { border-collapse: collapse; margin: 6px 0; font-size: 12px; max-width: 100%; display: block; overflow-x: auto; }
.answer :deep(th), .answer :deep(td) { border: 1px solid var(--line-2); padding: 4px 8px; text-align: left; vertical-align: top; }
.answer :deep(th) { background: var(--hover); font-weight: 600; }
.answer :deep(a.page-link) { color: var(--accent-text); cursor: pointer; text-decoration: underline; text-decoration-color: color-mix(in oklab, var(--accent) 40%, transparent); }
.thinking { display: flex; align-items: center; gap: 7px; font-size: 12px; color: var(--text-3); }
.spinner { width: 11px; height: 11px; border-radius: 50%; border: 2px solid var(--line-2); border-top-color: var(--accent); animation: spin .8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
.link { border: 0; background: none; color: var(--accent-text); font: inherit; font-size: 12px; padding: 0; text-decoration: underline; }
.mem { border-radius: 10px; background: var(--surface); box-shadow: 0 0 0 1px var(--line); padding: 10px 12px; display: flex; flex-direction: column; gap: 6px; font-size: 12.5px; }
.mem-h { display: flex; align-items: center; gap: 6px; font-weight: 600; }
.mem-f { display: flex; align-items: flex-start; gap: 8px; line-height: 1.4; user-select: text; }
.mem-f input { margin: 2px 0 0; accent-color: var(--accent); flex: none; }
.mem-hint { margin: 2px 0 0; color: var(--text-3); font-size: 11.5px; }
.mem-a { display: flex; justify-content: flex-end; gap: 6px; }
.mem-done { margin: 0; color: var(--text-2); display: flex; align-items: center; gap: 6px; }
.error { font-size: 12.5px; color: var(--bad); overflow-wrap: anywhere; user-select: text; }
.composer { flex: none; display: flex; flex-direction: column; gap: 8px; padding: 10px 12px 12px; border-top: 1px solid var(--line); }
.composer textarea { height: auto; resize: none; padding: 7px 10px; font-size: 12.5px; line-height: 1.45; user-select: text; }
.composer .btn { align-self: flex-end; }
</style>
