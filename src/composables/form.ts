// Formulaires : ordre de tabulation, focus d'un champ (même hors écran), effacement.
import type { FormField, Widget } from "../bindings";
import { type DocTab, useTabs } from "../stores/tabs";
import { guardSigned } from "./signed";

export interface Stop {
  field: FormField;
  widget: Widget;
}

/** Le document a quelque chose à remplir : champ saisissable ou signature à apposer. */
export function hasFormContent(tab: DocTab): boolean {
  const signed = new Set((tab.signatures ?? []).map((s) => s.field));
  return (tab.edit?.fields ?? []).some((f) => fillable(f) || (f.kind.type === "signature" && !f.readOnly && !signed.has(f.id)));
}

/** Champ saisissable au clavier (les champs de signature s'ouvrent au clic). */
export function fillable(f: FormField): boolean {
  return !f.readOnly && f.widgets.length > 0 && f.kind.type !== "signature" && f.kind.type !== "button";
}

/** Arrêts de tabulation : un par champ (le bouton coché d'un groupe radio), par page puis
 * en lignes de haut en bas, de gauche à droite. */
export function stops(fields: FormField[]): Stop[] {
  const list = fields.filter(fillable).map((field) => {
    const on = field.kind.type === "radio" ? field.widgets.find((w) => w.onState && field.value.includes(w.onState)) : undefined;
    return { field, widget: on ?? field.widgets[0] };
  });
  // Tri par position du premier widget du champ (un groupe radio garde sa place).
  const first = (s: Stop) => s.field.widgets.reduce((a, w) => (w.page < a.page || (w.page === a.page && w.rect.y < a.rect.y) ? w : a));
  return list.sort((a, b) => {
    const wa = first(a);
    const wb = first(b);
    if (wa.page !== wb.page) return wa.page - wb.page;
    const row = Math.min(wa.rect.h, wb.rect.h) / 2;
    if (Math.abs(wa.rect.y - wb.rect.y) > row) return wa.rect.y - wb.rect.y;
    return wa.rect.x - wb.rect.x;
  });
}

export function useForm() {
  const tabs = useTabs();

  /** Donne le focus à un champ ; si sa page n'est pas affichée, y va d'abord. */
  function focusField(tab: DocTab, id: string) {
    const stop = stops(tab.edit?.fields ?? []).find((s) => s.field.id === id);
    if (!stop) return;
    tab.formFocus = { id, seq: Date.now() };
    const el = document.querySelector<HTMLElement>(`[data-field-id="${CSS.escape(id)}"][data-stop]`);
    if (el) el.focus();
    else tabs.goto(tab, stop.widget.page, Math.max(0, stop.widget.rect.y - 80));
  }

  /** Champ suivant (1) ou précédent (-1), en boucle. */
  function step(tab: DocTab, dir: 1 | -1) {
    const list = stops(tab.edit?.fields ?? []);
    if (!list.length) return;
    const i = list.findIndex((s) => s.field.id === tab.focusedField);
    const next = i < 0 ? (dir > 0 ? 0 : list.length - 1) : (i + dir + list.length) % list.length;
    focusField(tab, list[next].field.id);
  }

  /** Saisie d'un champ ; faux si l'utilisateur renonce (document signé). */
  async function setValue(tab: DocTab, id: string, value: string[]): Promise<boolean> {
    if (!(await guardSigned(tab, "fill"))) return false;
    await tabs.applyOps(tab, [{ op: "setField", id, value }]);
    return true;
  }

  /** Rétablit les valeurs par défaut (un seul pas d'annulation). */
  async function reset(tab: DocTab) {
    const ops = (tab.edit?.fields ?? [])
      .filter((f) => fillable(f) && JSON.stringify(f.value) !== JSON.stringify(f.defaultValue))
      .map((f) => ({ op: "setField" as const, id: f.id, value: f.defaultValue }));
    if (ops.length && (await guardSigned(tab, "fill"))) return tabs.applyOps(tab, ops);
  }

  const canReset = (tab: DocTab) => (tab.edit?.fields ?? []).some((f) => fillable(f) && JSON.stringify(f.value) !== JSON.stringify(f.defaultValue));

  return { focusField, step, setValue, reset, canReset };
}
