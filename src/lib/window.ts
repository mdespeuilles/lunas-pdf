// Fenêtre, dialogues et ouverture de liens (inactifs hors Tauri).
import { inTauri } from "./api";

async function win() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export const appWindow = {
  async minimize() {
    if (inTauri) await (await win()).minimize();
  },
  async toggleMaximize() {
    if (inTauri) await (await win()).toggleMaximize();
  },
  async close() {
    if (inTauri) await (await win()).close();
  },
  async destroy() {
    if (inTauri) await (await win()).destroy();
  },
  /** Intercepte la fermeture (bouton du gestionnaire de fenêtres, Alt F4…). */
  async onCloseRequested(cb: () => Promise<boolean>) {
    if (!inTauri) return;
    const w = await win();
    await w.onCloseRequested(async (ev) => {
      ev.preventDefault();
      if (await cb()) await w.destroy();
    });
  },
  async show() {
    if (inTauri) {
      const w = await win();
      await w.show();
      await w.setFocus();
    }
  },
  async isMaximized() {
    return inTauri ? (await win()).isMaximized() : false;
  },
  async onResized(cb: () => void) {
    if (inTauri) await (await win()).onResized(cb);
  },
};

export async function pickPdfFiles(title: string): Promise<string[]> {
  if (!inTauri) return [];
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ multiple: true, title, filters: [{ name: "PDF", extensions: ["pdf", "PDF"] }] });
  return r ? (Array.isArray(r) ? r : [r]) : [];
}

export async function openUrl(url: string) {
  if (!/^(https?|mailto):/i.test(url)) return;
  if (inTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}

/** Position du pointeur en px CSS (la webview donne des px physiques). */
export type DropEvent =
  | { type: "enter" | "over"; paths?: string[]; x: number; y: number }
  | { type: "drop"; paths: string[]; x: number; y: number }
  | { type: "leave" };

/** Glisser-déposer de fichiers n'importe où dans la fenêtre. */
export async function onFileDrop(cb: (e: DropEvent) => void) {
  const e2e = (window as unknown as { __FEUILLET_E2E__?: Record<string, unknown> }).__FEUILLET_E2E__;
  if (e2e) e2e.emitFileDrop = cb;
  if (!inTauri) return;
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  await getCurrentWebview().onDragDropEvent((ev) => {
    const p = ev.payload;
    const k = window.devicePixelRatio || 1;
    const at = "position" in p ? { x: p.position.x / k, y: p.position.y / k } : { x: 0, y: 0 };
    if (p.type === "enter") cb({ type: "enter", paths: p.paths, ...at });
    else if (p.type === "over") cb({ type: "over", ...at });
    else if (p.type === "drop") cb({ type: "drop", paths: p.paths, ...at });
    else cb({ type: "leave" });
  });
}

export function isPdfPath(p: string) {
  return /\.pdf$/i.test(p);
}
