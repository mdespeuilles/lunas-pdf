// Panneau IA (repris de Lunas Mail) : questions et actions sur le document de l'onglet,
// confiées à l'agent choisi dans les préférences. Une conversation par onglet ; les
// modifications faites par l'agent arrivent par `aiEditedEvent` et s'appliquent comme celles
// de l'interface (annulables, non enregistrées).
import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { events, type AiError, type AiTask, type ChatTurn } from "../bindings";
import { commands, inTauri } from "../lib/api";
import { i18n } from "../i18n";
import { useSettings } from "./settings";
import { type DocTab, useTabs } from "./tabs";

/** Une demande en cours : le texte arrive par morceaux, puis le résultat complet. */
export interface AiJob {
  cancel: () => void;
  result: Promise<string>;
}

interface Handlers {
  delta?: (delta: string) => void;
  tool?: (name: string, input: Record<string, unknown>) => void;
  memory?: (add: string[], replace: string[]) => void;
}

/** Informations que l'agent propose de mémoriser ; `keep` : cases cochées par l'utilisateur. */
export interface MemoryProposal {
  add: string[];
  replace: string[];
  keep: boolean[];
  state: "pending" | "saved" | "dismissed";
}

/** Tour de la conversation. */
export interface ChatEntry {
  role: "user" | "assistant";
  text: string;
  tools: { name: string; input: Record<string, unknown> }[];
  memory: MemoryProposal | null;
  state: "running" | "done" | "error";
}

let seq = 0;

/** Message d'erreur traduit. */
export function aiErrorText(e: unknown): string {
  const t = i18n.global.t;
  const err = e as Partial<AiError> | undefined;
  if (!err?.kind) return t("ai.errors.failed", { detail: String((e as Error)?.message ?? e) });
  return t(`ai.errors.${err.kind}`, { detail: err.detail ?? "" });
}

export const useAi = defineStore("ai", () => {
  const settings = useSettings();
  const tabs = useTabs();
  /** Un agent est choisi : le panneau est proposé. */
  const enabled = computed(() => (settings.settings.aiAgent ?? "none") !== "none");

  const handlers = new Map<string, Handlers>();
  let listening = false;

  async function init() {
    if (listening || (!inTauri && !("__LUNAS_PDF_E2E__" in window))) return;
    listening = true;
    await events.aiChunkEvent.listen((e) => handlers.get(e.payload.requestId)?.delta?.(e.payload.delta));
    await events.aiToolEvent.listen((e) => {
      let input: Record<string, unknown> = {};
      try {
        input = JSON.parse(e.payload.input) ?? {};
      } catch {
        /* arguments illisibles : nom seul */
      }
      handlers.get(e.payload.requestId)?.tool?.(e.payload.name, input);
    });
    await events.aiEditedEvent.listen((e) => tabs.applyExternal(e.payload.doc, e.payload.state));
    await events.aiMemoryEvent.listen((e) => handlers.get(e.payload.requestId)?.memory?.(e.payload.add, e.payload.replace));
    await loadMemory();
  }

  // --- Informations mémorisées (remplissage des documents) -------------------------------

  const memory = ref<string[]>([]);

  async function loadMemory() {
    memory.value = await commands.getAiMemory();
  }

  async function setMemory(facts: string[]) {
    const r = await commands.setAiMemory(facts);
    if (r.status === "ok") memory.value = r.data;
  }

  /** Mémorise les informations cochées d'une proposition (remplace celles qu'elles mettent à jour). */
  async function acceptMemory(p: MemoryProposal) {
    const kept = p.add.filter((_, i) => p.keep[i]);
    await setMemory([...memory.value.filter((f) => !p.replace.includes(f)), ...kept]);
    p.state = "saved";
  }

  /** Lance une tâche ; le texte et les appels d'outils arrivent au fil de l'eau. */
  function run(task: AiTask, on: Handlers = {}): AiJob {
    const id = `ai-${Date.now()}-${++seq}`;
    handlers.set(id, on);
    const result = commands
      .aiRun(id, task)
      .then((r) => (r.status === "ok" ? r.data : Promise.reject(r.error)))
      .finally(() => handlers.delete(id));
    return { result, cancel: () => void commands.aiCancel(id) };
  }

  // --- Conversation sur le document de chaque onglet ------------------------------------

  const chats = ref<Record<string, ChatEntry[]>>({});
  const jobs = new Map<string, AiJob>();
  const running = (key: string) => (chats.value[key] ?? []).some((e) => e.state === "running");

  function ask(tab: DocTab, question: string) {
    const q = question.trim();
    if (!q || !tab.info || running(tab.key)) return;
    chats.value[tab.key] ??= [];
    // Le tableau réactif (et non le tableau brut affecté) : ses modifications s'affichent.
    const chat = chats.value[tab.key];
    const history: ChatTurn[] = chat.filter((e) => e.state === "done" && e.text).map((e) => ({ role: e.role, text: e.text }));
    chat.push({ role: "user", text: q, tools: [], memory: null, state: "done" });
    chat.push({ role: "assistant", text: "", tools: [], memory: null, state: "running" });
    // L'entrée réactive (et non l'objet brut) : ses modifications s'affichent.
    const entry = chat[chat.length - 1];
    const job = run(
      {
        kind: "chat",
        doc: tab.info.id,
        name: tab.name,
        pages: tab.info.pages.map((p) => [p.width, p.height] as [number, number]),
        lang: settings.locale,
        history,
        question: q,
      },
      {
        delta: (d) => (entry.text += d),
        tool: (name, input) => entry.tools.push({ name, input }),
        memory: (add, replace) => {
          // Déjà mémorisées : rien à proposer.
          const fresh = add.filter((f) => !memory.value.includes(f));
          if (fresh.length) entry.memory = { add: fresh, replace, keep: fresh.map(() => true), state: "pending" };
        },
      },
    );
    jobs.set(tab.key, job);
    job.result
      .then(
        (text) => {
          entry.text = text;
          entry.state = "done";
        },
        (e) => {
          entry.text = aiErrorText(e);
          entry.state = "error";
        },
      )
      .finally(() => jobs.delete(tab.key));
  }

  function stop(key: string) {
    jobs.get(key)?.cancel();
  }

  function clear(key: string) {
    stop(key);
    delete chats.value[key];
  }

  function toggle(tab: DocTab, open = !tab.aiOpen) {
    if (!enabled.value) return;
    tab.aiOpen = open;
    if (open) tab.sigPanel = false;
  }

  // Onglet fermé : sa conversation s'arrête.
  tabs.$onAction(({ name, args }) => {
    if (name === "close") clear(args[0] as string);
  });

  return { enabled, init, run, chats, running, ask, stop, clear, toggle, memory, loadMemory, setMemory, acceptMemory };
});
