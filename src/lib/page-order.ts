// Ordre des pages après un déplacement (même règle que `pages::reorder` côté Rust).

/** Nouvel ordre (indices d'origine) quand `pages` passent avant la page d'indice `to`. */
export function reorder(count: number, pages: number[], to: number): number[] {
  const sel = new Set(pages.filter((p) => p >= 0 && p < count));
  const moving = [...sel].sort((a, b) => a - b);
  let anchor = -1;
  for (let i = to; i < count; i++) {
    if (!sel.has(i)) {
      anchor = i;
      break;
    }
  }
  const rest = Array.from({ length: count }, (_, i) => i).filter((i) => !sel.has(i));
  const at = anchor < 0 ? rest.length : rest.indexOf(anchor);
  rest.splice(at, 0, ...moving);
  return rest;
}

/** Indices des pages déplacées dans le nouvel ordre. */
export function movedIndices(count: number, pages: number[], to: number): number[] {
  const order = reorder(count, pages, to);
  const sel = new Set(pages);
  return order.map((p, i) => (sel.has(p) ? i : -1)).filter((i) => i >= 0);
}

/** Le déplacement ne change rien (pages déjà à cet endroit). */
export function isNoop(count: number, pages: number[], to: number): boolean {
  return reorder(count, pages, to).every((p, i) => p === i);
}
