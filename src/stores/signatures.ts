import { defineStore } from "pinia";
import { ref } from "vue";
import type { Rect, SavedSignature } from "../bindings";
import { commands, unwrap } from "../lib/api";

/** Emplacement visé : un champ de signature (sinon, centre de la page courante). */
export interface SigTarget {
  page: number;
  rect: Rect;
}

/** Signatures manuscrites enregistrées (planche 07), menu de l'outil S et dialogue de création. */
export const useSignatures = defineStore("signatures", () => {
  const list = ref<SavedSignature[]>([]);
  const loaded = ref(false);
  /** Menu ouvert : onglet, position d'ancrage (px, fenêtre), cible éventuelle. */
  const picker = ref<{ tabKey: string; x: number; y: number; target: SigTarget | null } | null>(null);
  /** Dialogue « Nouvelle signature » ouvert. */
  const dialog = ref<{ tabKey: string; target: SigTarget | null } | null>(null);

  async function load() {
    list.value = await commands.listSignatures();
    loaded.value = true;
  }

  async function save(png: Uint8Array, width: number, height: number) {
    const s = await unwrap(commands.saveSignature(Array.from(png), width, height));
    await load();
    return s;
  }

  async function remove(id: string) {
    await unwrap(commands.deleteSignature(id));
    list.value = list.value.filter((s) => s.id !== id);
  }

  return { list, loaded, picker, dialog, load, save, remove };
});
