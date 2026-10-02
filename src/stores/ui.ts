import { defineStore } from "pinia";
import { ref } from "vue";

export type AskKind = "unsaved" | "redact" | "signedAnnotate" | "signedFill" | "signedOrganize";
export type AskAnswer = "save" | "discard" | "cancel" | "confirm" | "copy";

/** État d'interface global : dialogues et messages éphémères. */
export const useUi = defineStore("ui", () => {
  const prefsOpen = ref(false);
  const aboutOpen = ref(false);
  const toast = ref<{ text: string; seq: number } | null>(null);
  const ask = ref<{ kind: AskKind; tabKey: string; resolve: (a: AskAnswer) => void } | null>(null);
  /** Case « Ne plus demander pour ce document » du dernier dialogue. */
  const askRemember = ref(false);
  let seq = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function notify(text: string) {
    toast.value = { text, seq: ++seq };
    clearTimeout(timer);
    timer = setTimeout(() => (toast.value = null), 4000);
  }

  /** Ouvre un dialogue de confirmation et attend la réponse. */
  function askUser(kind: AskKind, tabKey: string): Promise<AskAnswer> {
    ask.value?.resolve("cancel");
    askRemember.value = false;
    return new Promise((resolve) => {
      ask.value = {
        kind,
        tabKey,
        resolve: (a) => {
          ask.value = null;
          resolve(a);
        },
      };
    });
  }

  return { prefsOpen, aboutOpen, toast, ask, askRemember, notify, askUser };
});
