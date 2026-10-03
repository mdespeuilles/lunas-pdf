<script setup lang="ts">
// Préférences de l'IA (reprises de Lunas Mail) : agent installé sur le poste, chemin,
// modèle, essai.
import { X } from "lucide-vue-next";
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import type { Agent, Detected } from "../bindings";
import { commands, inTauri } from "../lib/api";
import { aiErrorText, useAi } from "../stores/ai";
import { useSettings } from "../stores/settings";
import { useTabs } from "../stores/tabs";

const { t } = useI18n();
const settings = useSettings();
const ai = useAi();
const tabs = useTabs();

/** Installation trouvée automatiquement ; `undefined` : recherche en cours. */
const detected = ref<Detected | undefined>(undefined);
const test = ref<{ state: "idle" | "running" | "ok" | "error"; text: string }>({ state: "idle", text: "" });
const NONE: Detected = { path: null, version: null, outdated: null };

async function detect() {
  detected.value = undefined;
  const agent = settings.settings.aiAgent;
  if (agent === "none" || !inTauri) {
    detected.value = NONE;
    return;
  }
  const r = await commands.aiDetect(agent).catch(() => null);
  detected.value = r?.status === "ok" ? r.data : NONE;
}

async function choose(agent: Agent) {
  test.value = { state: "idle", text: "" };
  await settings.update({ aiAgent: agent });
  // IA désactivée : les panneaux ouverts se ferment.
  if (agent === "none") for (const tab of tabs.tabs) tab.aiOpen = false;
  await detect();
}

async function runTest() {
  test.value = { state: "running", text: "" };
  const started = Date.now();
  try {
    const answer = await ai.run({ kind: "test" }).result;
    test.value = { state: "ok", text: t("ai.prefs.testOk", { answer: answer.slice(0, 60), s: ((Date.now() - started) / 1000).toFixed(1) }) };
  } catch (e) {
    test.value = { state: "error", text: aiErrorText(e) };
  }
}

onMounted(() => {
  void detect();
  if (inTauri || "__LUNAS_PDF_E2E__" in window) void ai.loadMemory();
});
</script>

<template>
  <div class="field col">
    <div class="row">
      <span>
        <span class="ttl" id="p-ai">{{ t("ai.prefs.agent") }}</span>
        <span class="hint">{{ t("ai.prefs.agentHint") }}</span>
      </span>
      <div class="seg" role="radiogroup" aria-labelledby="p-ai">
        <button
          v-for="a in (['none', 'claude', 'codex'] as const)"
          :key="a"
          role="radio"
          :aria-checked="settings.settings.aiAgent === a"
          :class="{ on: settings.settings.aiAgent === a }"
          @click="choose(a)"
        >
          {{ a === "none" ? t("ai.prefs.none") : a === "claude" ? "Claude Code" : "Codex" }}
        </button>
      </div>
    </div>
    <template v-if="settings.settings.aiAgent !== 'none'">
      <div class="row">
        <label class="sub" for="p-ai-path">
          <template v-if="detected === undefined">{{ t("ai.prefs.detecting") }}</template>
          <template v-else-if="detected.path">{{ t("ai.prefs.detected", { path: detected.path, version: detected.version ?? "?" }) }}</template>
          <span v-else-if="detected.outdated" class="warn">{{ t("ai.prefs.outdated", { path: detected.outdated.path, version: detected.outdated.version ?? "?" }) }}</span>
          <template v-else>{{ t("ai.prefs.notFound") }}</template>
        </label>
        <input
          id="p-ai-path"
          class="input mono"
          spellcheck="false"
          :value="settings.settings.aiPath"
          :placeholder="detected?.path || t('ai.prefs.pathPlaceholder')"
          :aria-label="t('ai.prefs.path')"
          @change="settings.update({ aiPath: ($event.target as HTMLInputElement).value.trim() })"
        />
      </div>
      <div class="row">
        <label class="sub" for="p-ai-model">{{ t("ai.prefs.modelHint") }}</label>
        <input
          id="p-ai-model"
          class="input mono small"
          spellcheck="false"
          :value="settings.settings.aiModel"
          :placeholder="t('ai.prefs.modelPlaceholder')"
          :aria-label="t('ai.prefs.model')"
          @change="settings.update({ aiModel: ($event.target as HTMLInputElement).value.trim() })"
        />
      </div>
      <div class="row">
        <span class="sub">
          <template v-if="test.state === 'idle'">{{ t("ai.prefs.testHint") }}</template>
          <template v-else-if="test.state === 'running'">{{ t("ai.prefs.testing") }}</template>
          <span v-else :class="test.state === 'ok' ? 'ok' : 'bad'">{{ test.text }}</span>
        </span>
        <button class="btn out" :disabled="test.state === 'running'" @click="runTest">{{ t("ai.prefs.test") }}</button>
      </div>
      <div class="memlist">
        <div class="row">
          <span>
            <span class="ttl">{{ t("ai.prefs.memory") }}</span>
            <span class="hint">{{ ai.memory.length ? t("ai.prefs.memoryHint") : t("ai.prefs.memoryEmpty") }}</span>
          </span>
          <button v-if="ai.memory.length" class="btn gh" @click="ai.setMemory([])">{{ t("ai.prefs.memoryClear") }}</button>
        </div>
        <ul v-if="ai.memory.length">
          <li v-for="f in ai.memory" :key="f">
            <span class="nm">{{ f }}</span>
            <button class="cl" :aria-label="t('ai.prefs.memoryRemove', { f })" :title="t('ai.prefs.memoryRemove', { f })" @click="ai.setMemory(ai.memory.filter((x) => x !== f))">
              <X class="ic xs" aria-hidden="true" />
            </button>
          </li>
        </ul>
      </div>
      <p class="hint">
        {{ t("ai.prefs.privacy", { vendor: settings.settings.aiAgent === "claude" ? "Anthropic" : "OpenAI" }) }}
        {{ settings.settings.aiAgent === "claude" ? t("ai.prefs.safetyClaude") : t("ai.prefs.safetyCodex") }}
      </p>
    </template>
  </div>
</template>

<style scoped>
.field { display: flex; padding: 10px 0; border-top: 1px solid var(--line); }
.field.col { flex-direction: column; align-items: stretch; justify-content: flex-start; gap: 10px; }
.row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
.ttl { display: block; font-weight: 500; }
.seg > button { white-space: nowrap; }
.hint { display: block; color: var(--text-2); font-size: 12px; margin: 2px 0 0; line-height: 1.45; }
.sub { flex: 1; color: var(--text-2); font-size: 12px; min-width: 0; overflow-wrap: anywhere; }
.input.mono { width: 240px; flex: none; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }
.input.small { width: 140px; }
.memlist { display: flex; flex-direction: column; gap: 8px; }
.memlist ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; max-height: 180px; overflow: auto; }
.memlist li { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 4px 4px 10px; border-radius: 7px; background: var(--hover); font-size: 12.5px; }
.memlist .nm { overflow-wrap: anywhere; user-select: text; }
.warn { color: var(--warn); }
.ok { color: var(--ok); }
.bad { color: var(--bad); }
</style>
