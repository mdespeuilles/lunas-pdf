// Texte et liens par page, mis en cache pour la durée de vie du document.
import type { LinkInfo, PageText } from "../bindings";
import { commands, unwrap } from "./api";

const texts = new Map<string, Promise<PageText>>();
const links = new Map<string, Promise<LinkInfo[]>>();

export function pageText(doc: number, page: number): Promise<PageText> {
  const key = `${doc}:${page}`;
  let p = texts.get(key);
  if (!p) {
    p = unwrap(commands.getPageText(doc, page));
    p.catch(() => texts.delete(key));
    texts.set(key, p);
  }
  return p;
}

export function pageLinks(doc: number, page: number): Promise<LinkInfo[]> {
  const key = `${doc}:${page}`;
  let p = links.get(key);
  if (!p) {
    p = unwrap(commands.getLinks(doc, page)).catch(() => []);
    links.set(key, p);
  }
  return p;
}

export function dropDocData(doc: number) {
  for (const m of [texts, links] as Map<string, unknown>[]) for (const k of [...m.keys()]) if (k.startsWith(`${doc}:`)) m.delete(k);
}
