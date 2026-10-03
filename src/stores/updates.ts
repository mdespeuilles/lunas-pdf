// Mises à jour de l'app installée (reprises de Lunas Mail) : publiées sur les versions GitHub
// du dépôt, signées avec la clé du projet (vérifiée par le module de mise à jour de Tauri).
// Jamais en développement (`tauri dev`).
import { defineStore } from "pinia";
import { markRaw, ref, shallowRef } from "vue";
import type { Update } from "@tauri-apps/plugin-updater";
import { inTauri } from "../lib/api";
import { confirmQuit } from "../composables/save";

/** Vérification périodique ; la première au lancement. */
const EVERY_MS = 6 * 3600 * 1000;

export const useUpdates = defineStore("updates", () => {
  // Objet du module de mise à jour tel quel, sans proxy réactif : ses champs privés (#…)
  // sont illisibles à travers un proxy (« Cannot read private member »).
  const available = shallowRef<Update | null>(null);
  const dismissed = ref(false);
  const checking = ref(false);
  const installing = ref(false);
  const progress = ref<number | null>(null);
  const error = ref("");
  /** Dernière vérification manuelle sans nouveauté. */
  const upToDate = ref(false);
  let started = false;

  const isDev = () => import.meta.env.DEV;

  async function checkNow(manual = false) {
    if (!inTauri || checking.value || installing.value) return;
    if (isDev()) {
      if (manual) error.value = "dev";
      return;
    }
    checking.value = true;
    error.value = "";
    upToDate.value = false;
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const u = await check();
      available.value = u ? markRaw(u) : null;
      if (u) dismissed.value = false;
      else upToDate.value = manual;
    } catch (e) {
      // Hors ligne au lancement : nouvel essai plus tard, sans bruit.
      if (manual) error.value = String(e);
    } finally {
      checking.value = false;
    }
  }

  function start() {
    if (started || !inTauri) return;
    started = true;
    void checkNow();
    setInterval(() => void checkNow(), EVERY_MS);
  }

  async function install() {
    const u = available.value;
    if (!u || installing.value) return;
    // Documents modifiés : proposer de les enregistrer avant (sous Windows, l'installeur
    // quitte l'app dès la fin du téléchargement).
    if (!(await confirmQuit())) return;
    installing.value = true;
    error.value = "";
    let total = 0;
    let done = 0;
    try {
      await u.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        else if (e.event === "Progress") {
          done += e.data.chunkLength;
          progress.value = total ? Math.round((done / total) * 100) : null;
        }
      });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (e) {
      error.value = String(e);
      installing.value = false;
    }
  }

  return { available, dismissed, checking, installing, progress, error, upToDate, start, checkNow, install };
});
