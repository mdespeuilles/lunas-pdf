import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

describe("markdown des réponses de l'IA", () => {
  it("rend listes, gras et paragraphes", () => {
    expect(renderMarkdown("Résumé **court**.\n\n- un\n- *deux*\n\n1. a\n2. b")).toBe(
      "<p>Résumé <strong>court</strong>.</p><ul><li>un</li><li><em>deux</em></li></ul><ol><li>a</li><li>b</li></ol>",
    );
  });
  it("échappe tout le HTML", () => {
    const html = renderMarkdown('<img src=x onerror="alert(1)"> [lien](https://x)');
    expect(html).not.toContain("<img");
    expect(html).toContain("&lt;img");
    expect(html).not.toContain("<a ");
  });
  it("rend les liens vers une page du document, et seulement ceux-là", () => {
    expect(renderMarkdown("- Voir [page 3](lunas://page/3) et [site](https://x.fr)")).toBe(
      '<ul><li>Voir <a class="page-link" data-page="3">page 3</a> et site</li></ul>',
    );
    // Pas d'attribut injecté : le texte reste échappé.
    expect(renderMarkdown('[x](lunas://page/3" onclick="y)')).not.toContain('onclick="');
  });
  it("garde les retours à la ligne d'un paragraphe", () => {
    expect(renderMarkdown("ligne 1\nligne 2")).toBe("<p>ligne 1<br>ligne 2</p>");
  });

  it("rend les tableaux", () => {
    const html = renderMarkdown("| Événement | Date |\n|---|---|\n| **Départ** | 11 oct. |\n| Retour | 16 oct. |");
    expect(html).toBe("<table><thead><tr><th>Événement</th><th>Date</th></tr></thead><tbody><tr><td><strong>Départ</strong></td><td>11 oct.</td></tr><tr><td>Retour</td><td>16 oct.</td></tr></tbody></table>");
  });

  it("rend les blocs de code sans les interpréter", () => {
    expect(renderMarkdown("```\nBEGIN:VCALENDAR\n<b>x</b>\n```")).toBe("<pre><code>BEGIN:VCALENDAR\n&lt;b&gt;x&lt;/b&gt;</code></pre>");
  });

  it("une barre verticale seule ne fait pas un tableau", () => {
    expect(renderMarkdown("| a |\ntexte")).toBe("<p>| a |<br>texte</p>");
  });
});
