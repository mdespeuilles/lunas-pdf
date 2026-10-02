import { defineStore } from "pinia";
import { ref } from "vue";
import type { RecentDoc } from "../bindings";
import { commands, unwrap } from "../lib/api";

export const useRecents = defineStore("recents", () => {
  const items = ref<RecentDoc[]>([]);
  /** Change à chaque rechargement pour invalider le cache des miniatures. */
  const version = ref(0);

  async function refresh() {
    items.value = await commands.getRecents();
    version.value++;
  }
  async function remove(path: string) {
    await unwrap(commands.removeRecent(path));
    await refresh();
  }
  async function clear() {
    await unwrap(commands.clearRecents());
    await refresh();
  }
  return { items, version, refresh, remove, clear };
});
