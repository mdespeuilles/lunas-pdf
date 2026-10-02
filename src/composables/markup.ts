// Surligner / souligner / barrer la sélection de texte courante (outil V puis H, U ou K).
import type { AnnotBody } from "../bindings";
import { commands, unwrap } from "../lib/api";
import { colorFor, familyOf, type Tool, useAnnotTools } from "../stores/annotTools";
import { type DocTab, useTabs } from "../stores/tabs";

export async function markupFromSelection(tab: DocTab, tool: Tool): Promise<boolean> {
  const sel = window.getSelection();
  if (!tab.info || !sel || sel.isCollapsed || !sel.rangeCount) return false;
  const range = sel.getRangeAt(0);
  const rects = [...range.getClientRects()].filter((r) => r.width > 0 && r.height > 0);
  const pageEl = range.startContainer.parentElement?.closest<HTMLElement>("[data-page]");
  if (!rects.length || !pageEl) return false;
  const page = Number(pageEl.dataset.page);
  const pr = pageEl.getBoundingClientRect();
  const k = pr.width / tab.info.pages[page].width;
  const first = rects[0];
  const last = rects[rects.length - 1];
  const from = { x: (first.left - pr.left) / k + 0.5, y: (first.top + first.height / 2 - pr.top) / k };
  const to = { x: (last.right - pr.left) / k - 0.5, y: (last.top + last.height / 2 - pr.top) / k };
  const r = await unwrap(commands.textRange(tab.info.id, page, from, to));
  if (!r.rects.length) return false;
  const tools = useAnnotTools();
  const type = tool === "highlight" ? "highlight" : tool === "underline" ? "underline" : "strikeOut";
  const id = crypto.randomUUID();
  await useTabs().addAnnot(tab, {
    id, page, rect: r.rects[0], color: colorFor(tools.colors[familyOf(tool)] ?? "yellow", tool), opacity: 1, width: 1,
    contents: null, author: null, modified: null, excerpt: null, body: { type, quads: r.rects } as AnnotBody, hidden: false,
  });
  sel.removeAllRanges();
  tab.selected = id;
  return true;
}
