// Pose d'une signature manuscrite : tampon image au centre de la page courante, ou ajusté
// dans un champ de signature.
import { i18n } from "../i18n";
import { commands, unwrap } from "../lib/api";
import { useAnnotTools } from "../stores/annotTools";
import type { SigTarget } from "../stores/signatures";
import { type DocTab, useTabs } from "../stores/tabs";
import { useUi } from "../stores/ui";
import { guardSigned } from "./signed";

/** Octets d'une image `data:…;base64,`. */
export function dataUrlBytes(url: string): Uint8Array {
  const bin = atob(url.slice(url.indexOf(",") + 1));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

export interface PlaceOptions {
  /** Champ visé : l'image y est ajustée, sans étape de pose. */
  target?: SigTarget | null;
  /** Taille maximale à la pose libre, en points. */
  maxW: number;
  maxH?: number;
  mime?: string;
}

/** Importe l'image dans le document puis la pose : dans le champ visé, ou sous le curseur
 * jusqu'au clic (Échap annule). */
export async function placeImageBytes(tab: DocTab, bytes: Uint8Array, opts: PlaceOptions) {
  const tabs = useTabs();
  const tools = useAnnotTools();
  if (!tab.info) return;
  if (!tab.annotating && !(await guardSigned(tab, "annotate"))) return;
  try {
    const img = await unwrap(commands.importImageBytes(tab.info.id, Array.from(bytes)));
    if (img.width <= 0 || img.height <= 0) return;
    const ratio = img.height / img.width;
    if (!tab.edit) await tabs.loadAnnotations(tab);
    tabs.toggleAnnotating(tab, true);
    tools.tool = "select";
    tools.cancelPlacing();
    const r = opts.target?.rect;
    if (opts.target && r) {
      // Dans le champ, marge de 2 pt, centrée.
      const w = Math.min(r.w - 4, (r.h - 4) / ratio);
      const h = w * ratio;
      const id = crypto.randomUUID();
      await tabs.addAnnot(tab, {
        id, page: opts.target.page, rect: { x: r.x + (r.w - w) / 2, y: r.y + (r.h - h) / 2, w, h }, color: "#000000", opacity: 1, width: 1,
        contents: null, author: null, modified: null, excerpt: null, body: { type: "image", image: img.key }, hidden: false,
      });
      tab.selected = id;
      return;
    }
    const g = tab.info.pages[tab.page];
    let w = Math.min(opts.maxW, g.width * 0.6);
    let h = w * ratio;
    if (opts.maxH && h > opts.maxH) {
      h = opts.maxH;
      w = h / ratio;
    }
    tab.selected = null;
    const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: opts.mime ?? "image/png" }));
    tools.placing = { tabKey: tab.key, key: img.key, url, w, h };
    useUi().notify(i18n.global.t("annot.placeHint"));
  } catch (e) {
    useUi().notify(String(e));
  }
}

export function placeSignature(tab: DocTab, png: Uint8Array, target: SigTarget | null) {
  return placeImageBytes(tab, png, { target, maxW: 150, maxH: 70 });
}
