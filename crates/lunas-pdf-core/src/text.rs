//! Regroupement des glyphes en fragments de texte et recherche plein texte.
//! Fonctions pures : testables sans PDFium.

use crate::annot::Point;
use crate::types::{Rect, SearchHit, TextRun, TextSelection};

/// Un caractère de la page. Les caractères générés par PDFium (espaces et fins de ligne
/// déduits de la mise en page) ont une boîte vide.
#[derive(Debug, Clone, Copy)]
pub struct Glyph {
    pub c: char,
    pub rect: Rect,
}

impl Glyph {
    fn is_break(&self) -> bool {
        matches!(self.c, '\r' | '\n')
    }
    fn is_space(&self) -> bool {
        self.c.is_whitespace() && !self.is_break()
    }
    fn has_box(&self) -> bool {
        self.rect.w > 0.0 && self.rect.h > 0.0
    }
    fn cy(&self) -> f32 {
        self.rect.y + self.rect.h / 2.0
    }
}

/// Construit les fragments (mots) de la couche texte.
pub fn build_runs(glyphs: &[Glyph]) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut cur: Option<(String, Rect)> = None;
    let mut last: Option<Glyph> = None;

    fn flush(runs: &mut Vec<TextRun>, cur: &mut Option<(String, Rect)>, eol: bool) {
        if let Some((text, rect)) = cur.take() {
            runs.push(TextRun { text, rect, eol });
        } else if eol && let Some(r) = runs.last_mut() {
            r.eol = true;
        }
    }

    for g in glyphs {
        if g.is_break() {
            flush(&mut runs, &mut cur, true);
            last = None;
            continue;
        }
        if g.is_space() {
            if let Some((text, _)) = cur.as_mut() {
                text.push(' ');
            }
            flush(&mut runs, &mut cur, false);
            continue;
        }
        if !g.has_box() {
            continue;
        }
        if let (Some(p), Some(_)) = (last, cur.as_ref()) {
            let h = p.rect.h.max(g.rect.h);
            let gap = g.rect.x - (p.rect.x + p.rect.w);
            let new_line = (g.cy() - p.cy()).abs() > h * 0.5;
            if new_line {
                flush(&mut runs, &mut cur, true);
            } else if gap > h * 0.25 || gap < -h {
                if let Some((text, _)) = cur.as_mut() {
                    text.push(' ');
                }
                flush(&mut runs, &mut cur, false);
            }
        }
        match cur.as_mut() {
            Some((text, rect)) => {
                text.push(g.c);
                *rect = rect.union(g.rect);
            }
            None => cur = Some((g.c.to_string(), g.rect)),
        }
        last = Some(*g);
    }
    flush(&mut runs, &mut cur, true);
    runs
}

fn fold(c: char, case_sensitive: bool) -> char {
    if c.is_whitespace() {
        ' '
    } else if case_sensitive {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Recherche toutes les occurrences de `query` dans la page. Les blancs consécutifs
/// (espaces, fins de ligne) sont équivalents à une espace.
pub fn search_page(page: u32, glyphs: &[Glyph], query: &str, case_sensitive: bool) -> Vec<SearchHit> {
    let q: Vec<char> = {
        let mut v: Vec<char> = Vec::new();
        for c in query.trim().chars().map(|c| fold(c, case_sensitive)) {
            if !(c == ' ' && v.last() == Some(&' ')) {
                v.push(c);
            }
        }
        v
    };
    if q.is_empty() {
        return vec![];
    }
    // Texte normalisé et correspondance vers l'index du glyphe d'origine.
    let mut norm: Vec<char> = Vec::with_capacity(glyphs.len());
    let mut map: Vec<usize> = Vec::with_capacity(glyphs.len());
    for (i, g) in glyphs.iter().enumerate() {
        let c = fold(g.c, case_sensitive);
        if c == ' ' && norm.last() == Some(&' ') {
            continue;
        }
        norm.push(c);
        map.push(i);
    }

    let mut hits = vec![];
    let mut i = 0;
    while i + q.len() <= norm.len() {
        if norm[i..i + q.len()] == q[..] {
            let (a, b) = (map[i], map[i + q.len() - 1]);
            hits.push(SearchHit {
                page,
                rects: line_rects(&glyphs[a..=b]),
                before: context(&norm_display(glyphs, &map, i.saturating_sub(40), i), true),
                matched: norm_display(glyphs, &map, i, i + q.len()),
                after: context(
                    &norm_display(glyphs, &map, i + q.len(), (i + q.len() + 40).min(norm.len())),
                    false,
                ),
            });
            i += q.len();
        } else {
            i += 1;
        }
    }
    hits
}

/// Texte d'origine (casse conservée) pour une plage de l'index normalisé.
fn norm_display(glyphs: &[Glyph], map: &[usize], from: usize, to: usize) -> String {
    map[from..to]
        .iter()
        .map(|&i| if glyphs[i].c.is_whitespace() { ' ' } else { glyphs[i].c })
        .collect()
}

/// Coupe le contexte au mot entier le plus proche.
fn context(s: &str, before: bool) -> String {
    let s = s.to_string();
    if before {
        match s.find(' ') {
            Some(i) if s.chars().count() >= 40 => s[i..].to_string(),
            _ => s,
        }
    } else {
        match s.rfind(' ') {
            Some(i) if s.chars().count() >= 40 => s[..i].to_string(),
            _ => s,
        }
    }
}

/// Une boîte par ligne pour une suite de glyphes.
fn line_rects(glyphs: &[Glyph]) -> Vec<Rect> {
    let mut out: Vec<Rect> = vec![];
    let mut last_cy: Option<f32> = None;
    for g in glyphs.iter().filter(|g| g.has_box()) {
        match (out.last_mut(), last_cy) {
            (Some(r), Some(cy)) if (g.cy() - cy).abs() <= g.rect.h.max(r.h) * 0.5 => *r = r.union(g.rect),
            _ => out.push(g.rect),
        }
        last_cy = Some(g.cy());
    }
    out
}

/// Glyphe le plus proche d'un point, en privilégiant la même ligne.
fn nearest(glyphs: &[Glyph], p: Point) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (i, g) in glyphs.iter().enumerate().filter(|(_, g)| g.has_box()) {
        let r = g.rect;
        let dx = if p.x < r.x {
            r.x - p.x
        } else if p.x > r.x + r.w {
            p.x - r.x - r.w
        } else {
            0.0
        };
        let dy = if p.y < r.y {
            r.y - p.y
        } else if p.y > r.y + r.h {
            p.y - r.y - r.h
        } else {
            0.0
        };
        let score = dy * 4.0 + dx;
        if best.is_none_or(|(s, _)| score < s) {
            best = Some((score, i));
        }
    }
    best.map(|(_, i)| i)
}

fn clean(glyphs: &[Glyph]) -> String {
    let mut out = String::new();
    for g in glyphs {
        let c = if g.c.is_whitespace() { ' ' } else { g.c };
        if !(c == ' ' && out.ends_with(' ')) {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Texte entre deux points (glisser de l'outil surligner) : une boîte par ligne.
pub fn range(glyphs: &[Glyph], from: Point, to: Point) -> TextSelection {
    let (Some(a), Some(b)) = (nearest(glyphs, from), nearest(glyphs, to)) else {
        return TextSelection {
            rects: vec![],
            text: String::new(),
        };
    };
    let (a, b) = (a.min(b), a.max(b));
    TextSelection {
        rects: line_rects(&glyphs[a..=b]),
        text: clean(&glyphs[a..=b]),
    }
}

/// Texte dont les glyphes ont leur centre dans l'une des zones.
pub fn text_in(glyphs: &[Glyph], zones: &[Rect]) -> String {
    let inside: Vec<Glyph> = glyphs
        .iter()
        .filter(|g| {
            if !g.has_box() {
                return g.c.is_whitespace();
            }
            let (cx, cy) = (g.rect.x + g.rect.w / 2.0, g.cy());
            zones
                .iter()
                .any(|z| cx >= z.x - 0.5 && cx <= z.x + z.w + 0.5 && cy >= z.y - 0.5 && cy <= z.y + z.h + 0.5)
        })
        .copied()
        .collect();
    clean(&inside)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, y: f32) -> Vec<Glyph> {
        text.chars()
            .enumerate()
            .map(|(i, c)| Glyph {
                c,
                rect: if c == ' ' {
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    }
                } else {
                    Rect {
                        x: 10.0 + i as f32 * 6.0,
                        y,
                        w: 5.0,
                        h: 10.0,
                    }
                },
            })
            .collect()
    }

    fn page() -> Vec<Glyph> {
        let mut g = line("Chaque échéance donne", 10.0);
        g.push(Glyph {
            c: '\r',
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
        });
        g.push(Glyph {
            c: '\n',
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
        });
        g.extend(line("lieu à une Échéance.", 30.0));
        g
    }

    #[test]
    fn runs_are_words_with_eol() {
        let runs = build_runs(&page());
        let texts: Vec<&str> = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["Chaque ", "échéance ", "donne", "lieu ", "à ", "une ", "Échéance."]);
        assert!(runs[2].eol && runs[6].eol && !runs[0].eol);
    }

    #[test]
    fn search_case_insensitive_and_sensitive() {
        let g = page();
        assert_eq!(search_page(0, &g, "échéance", false).len(), 2);
        assert_eq!(search_page(0, &g, "échéance", true).len(), 1);
        assert_eq!(search_page(0, &g, "Échéance", true).len(), 1);
    }

    #[test]
    fn search_across_line_break() {
        let hits = search_page(4, &page(), "donne lieu", false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page, 4);
        assert_eq!(hits[0].rects.len(), 2, "une boîte par ligne");
        assert_eq!(hits[0].matched, "donne lieu");
    }

    #[test]
    fn range_between_points() {
        let g = page();
        // De « échéance » (ligne 1) à « lieu » (ligne 2).
        let sel = range(
            &g,
            Point {
                x: 10.0 + 7.0 * 6.0 + 1.0,
                y: 15.0,
            },
            Point {
                x: 10.0 + 3.0 * 6.0,
                y: 35.0,
            },
        );
        assert_eq!(sel.text, "échéance donne lieu");
        assert_eq!(sel.rects.len(), 2);
        assert_eq!(text_in(&g, &sel.rects), "échéance donne lieu");
        assert!(
            range(&[], Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 })
                .rects
                .is_empty()
        );
    }

    #[test]
    fn search_context() {
        let hits = search_page(0, &page(), "une", false);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].before.ends_with("lieu à "));
        assert_eq!(hits[0].after, " Échéance.");
    }
}
