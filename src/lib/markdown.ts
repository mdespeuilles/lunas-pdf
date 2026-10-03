// Markdown minimal des réponses de l'IA → HTML sûr (repris de Lunas Mail) : tout est
// échappé, puis seuls titres, listes, tableaux, blocs de code, gras, italique et code en
// ligne sont rendus.
// Pas d'images ni de liens web : rien ne peut charger une ressource ou mener ailleurs.
// Seule exception, les liens vers une page du document (`lunas://page/<n>`), rendus en
// `<a class="page-link" data-page>` que le panneau suit lui-même ; les autres liens gardent
// leur seul texte.

const escape = (s: string) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function inline(s: string): string {
  return escape(s)
    .replace(/\[([^\]]+)\]\(lunas:\/\/page\/(\d+)\)/g, '<a class="page-link" data-page="$2">$1</a>')
    .replace(/\[([^\]]+)\]\([^)\s]*\)/g, "$1")
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*\w])\*([^*\s][^*]*)\*(?!\w)/g, "$1<em>$2</em>");
}

const isRow = (l: string) => /^\s*\|.*\|\s*$/.test(l);
const isSeparator = (l: string) => /^\s*\|?(\s*:?-{3,}:?\s*\|)+\s*:?-{0,}:?\s*\|?\s*$/.test(l.trim());
const cells = (l: string) => l.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim());

export function renderMarkdown(md: string): string {
  const out: string[] = [];
  let list: "ul" | "ol" | null = null;
  let para: string[] = [];
  const flushPara = () => {
    if (para.length) out.push(`<p>${para.map(inline).join("<br>")}</p>`);
    para = [];
  };
  const closeList = () => {
    if (list) out.push(`</${list}>`);
    list = null;
  };
  const lines = md.replace(/\r\n/g, "\n").split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i].trimEnd();
    // Bloc de code ``` … ``` : texte brut, espaces conservés.
    if (/^\s*```/.test(line)) {
      flushPara();
      closeList();
      const code: string[] = [];
      while (++i < lines.length && !/^\s*```/.test(lines[i])) code.push(lines[i]);
      out.push(`<pre><code>${escape(code.join("\n"))}</code></pre>`);
      continue;
    }
    // Tableau : ligne d'en-tête « | a | b | » suivie d'une ligne « |---|---| ».
    if (isRow(line) && i + 1 < lines.length && isSeparator(lines[i + 1])) {
      flushPara();
      closeList();
      const head = cells(line);
      const body: string[][] = [];
      i += 1;
      while (i + 1 < lines.length && isRow(lines[i + 1].trimEnd())) body.push(cells(lines[++i].trimEnd()));
      const th = head.map((c) => `<th>${inline(c)}</th>`).join("");
      const rows = body.map((r) => `<tr>${head.map((_, k) => `<td>${inline(r[k] ?? "")}</td>`).join("")}</tr>`).join("");
      out.push(`<table><thead><tr>${th}</tr></thead><tbody>${rows}</tbody></table>`);
      continue;
    }
    const bullet = /^\s*[-*•]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    const heading = /^#{1,6}\s+(.*)$/.exec(line);
    if (bullet || numbered) {
      flushPara();
      const kind = bullet ? "ul" : "ol";
      if (list !== kind) {
        closeList();
        out.push(`<${kind}>`);
        list = kind;
      }
      out.push(`<li>${inline((bullet ?? numbered)![1])}</li>`);
    } else if (heading) {
      flushPara();
      closeList();
      out.push(`<h4>${inline(heading[1])}</h4>`);
    } else if (!line.trim()) {
      flushPara();
      closeList();
    } else {
      closeList();
      para.push(line);
    }
  }
  flushPara();
  closeList();
  return out.join("");
}
