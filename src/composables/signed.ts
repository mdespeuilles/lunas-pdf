// Document signé : avertissement avant la première modification (planche 06), avec la
// possibilité de travailler sur une copie.
import { type DocTab, useTabs } from "../stores/tabs";
import { useSettings } from "../stores/settings";
import { useUi } from "../stores/ui";
import { copyPath, saveTab } from "./save";

const acknowledged = new WeakSet<DocTab>();

/** Vrai si l'on peut modifier ce document (éventuellement devenu une copie). */
export async function guardSigned(tab: DocTab, action: "annotate" | "fill"): Promise<boolean> {
  if (!tab.signatures?.length || acknowledged.has(tab)) return true;
  const settings = useSettings();
  if (settings.settings.signedOk.includes(tab.path)) {
    acknowledged.add(tab);
    return true;
  }
  const ui = useUi();
  const answer = await ui.askUser(action === "annotate" ? "signedAnnotate" : "signedFill", tab.key);
  if (answer === "cancel") return false;
  if (ui.askRemember) void settings.update({ signedOk: [...settings.settings.signedOk, tab.path] });
  if (answer === "copy" && !(await saveTab(tab, true, copyPath(tab.path)))) return false;
  acknowledged.add(tab);
  return true;
}

/** Bouton Annoter, Ctrl Maj A : entre en mode annotation après l'avertissement éventuel. */
export async function toggleAnnotate(tab: DocTab) {
  const tabs = useTabs();
  if (!tab.annotating && !(await guardSigned(tab, "annotate"))) return;
  tabs.toggleAnnotating(tab);
}
