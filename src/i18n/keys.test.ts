/// <reference types="node" />
// Chaque clé de traduction écrite en toutes lettres dans le code (`t("a.b")`) existe en
// français et en anglais. Les clés construites (`t(\`ai.suggestions.${s}\`)`) ne sont pas
// vérifiées ici.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import en from "./en";
import fr from "./fr";

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((n) => {
    const p = join(dir, n);
    if (statSync(p).isDirectory()) return files(p);
    return /\.(vue|ts)$/.test(n) && !n.endsWith(".test.ts") && n !== "bindings.ts" ? [p] : [];
  });
}

function has(messages: unknown, key: string): boolean {
  let cur: unknown = messages;
  for (const part of key.split(".")) {
    if (typeof cur !== "object" || cur === null || !(part in cur)) return false;
    cur = (cur as Record<string, unknown>)[part];
  }
  return typeof cur === "string";
}

const used = new Map<string, string>();
for (const f of files(join(__dirname, ".."))) {
  for (const m of readFileSync(f, "utf8").matchAll(/\bt\(\s*["'`]([a-zA-Z][\w]*(?:\.[\w]+)+)["'`]/g)) used.set(m[1], f);
}

describe("clés de traduction", () => {
  it("le code en utilise", () => expect(used.size).toBeGreaterThan(100));
  for (const [lang, messages] of [["fr", fr], ["en", en]] as const) {
    it(`toutes existent (${lang})`, () => {
      const missing = [...used].filter(([k]) => !has(messages, k)).map(([k, f]) => `${k} (${f.split("/src/")[1]})`);
      expect(missing).toEqual([]);
    });
  }
});
