// Chargé avant l'interface (index.html) : journal de démarrage et erreurs, recopiés dans le
// terminal même si le module principal échoue à se charger. Aucune dépendance.
type Internals = { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
const internals = (window as unknown as { __TAURI_INTERNALS__?: Internals }).__TAURI_INTERNALS__;

export function bootLog(message: string) {
  if (!internals || "__FEUILLET_E2E__" in window) return;
  void internals.invoke("log_frontend_error", { message }).catch(() => {});
}

bootLog(`démarrage : page chargée (${navigator.userAgent.match(/AppleWebKit\/[\d.]+/)?.[0] ?? "?"})`);
window.addEventListener(
  "error",
  (e) => {
    // Échec de chargement d'une ressource (script, style) : pas de message, mais une cible.
    const el = e.target as (HTMLElement & { src?: string; href?: string }) | null;
    if (el && el !== (window as unknown) && (el.src || el.href)) bootLog(`ressource introuvable : ${el.src || el.href}`);
    else bootLog(`erreur : ${e.message} (${e.filename}:${e.lineno}:${e.colno})`);
  },
  true,
);
window.addEventListener("unhandledrejection", (e) => bootLog(`promesse rejetée : ${String((e.reason as Error)?.stack ?? e.reason)}`));
