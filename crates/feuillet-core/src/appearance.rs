//! Génération des flux d'apparence (`/AP /N`) des annotations.
//!
//! Le contenu est dessiné dans un repère local aligné sur la page *affichée* (origine en bas
//! à gauche du rectangle, y vers le haut). La `/Matrix` du formulaire compense la rotation de la
//! page, si bien que les textes, notes et tampons restent droits même sur une page tournée.

use std::io::Write;

use lopdf::{Dictionary, Object, ObjectId, Stream, dictionary};

use crate::annot::{Annot, AnnotBody, CheckStyle, FontFamily, Point, parse_color};
use crate::fonts;
use crate::geom::Affine;
use crate::pdfwrite::{write_real, write_string};
use crate::types::Rect;

/// Allocateur d'objets indirects (polices, images) fourni par l'écrivain.
pub type Alloc<'a> = dyn FnMut(Object) -> ObjectId + 'a;

/// Couleur d'une note par défaut (planche 03).
pub const NOTE_ICON_INK: [f32; 3] = [0.361, 0.267, 0.0];
pub const NOTE_SIZE: f32 = 22.0;

struct Ops(Vec<u8>);

impl Ops {
    fn n(&mut self, v: f32) -> &mut Self {
        write_real(&mut self.0, v);
        self.0.push(b' ');
        self
    }
    fn op(&mut self, o: &str) -> &mut Self {
        self.0.extend_from_slice(o.as_bytes());
        self.0.push(b'\n');
        self
    }
    fn rgb(&mut self, c: [f32; 3], stroke: bool) -> &mut Self {
        self.n(c[0]).n(c[1]).n(c[2]).op(if stroke { "RG" } else { "rg" })
    }
    fn m(&mut self, p: (f32, f32)) -> &mut Self {
        self.n(p.0).n(p.1).op("m")
    }
    fn l(&mut self, p: (f32, f32)) -> &mut Self {
        self.n(p.0).n(p.1).op("l")
    }
    fn re(&mut self, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.n(x).n(y).n(w).n(h).op("re")
    }
    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) -> &mut Self {
        const K: f32 = 0.552_284_8;
        let (kx, ky) = (rx * K, ry * K);
        self.m((cx + rx, cy));
        self.n(cx + rx).n(cy + ky).n(cx + kx).n(cy + ry).n(cx).n(cy + ry).op("c");
        self.n(cx - kx).n(cy + ry).n(cx - rx).n(cy + ky).n(cx - rx).n(cy).op("c");
        self.n(cx - rx).n(cy - ky).n(cx - kx).n(cy - ry).n(cx).n(cy - ry).op("c");
        self.n(cx + kx).n(cy - ry).n(cx + rx).n(cy - ky).n(cx + rx).n(cy).op("c")
    }
}

/// Repère local d'un rectangle d'affichage.
struct Local {
    r: Rect,
}
impl Local {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        (x - self.r.x, self.r.y + self.r.h - y)
    }
    fn rect(&self, q: &Rect) -> (f32, f32, f32, f32) {
        let (x, y) = self.p(q.x, q.y + q.h);
        (x, y, q.w, q.h)
    }
}

pub struct FontSpec {
    pub res: &'static str,
    pub base: &'static str,
    pub widths: &'static [u16; 256],
    pub ascent: f32,
    pub descent: f32,
}

pub fn font_spec(f: FontFamily) -> FontSpec {
    match f {
        FontFamily::Sans => FontSpec {
            res: "Helv",
            base: "Helvetica",
            widths: &fonts::HELVETICA_WIDTHS,
            ascent: fonts::HELVETICA_METRICS.0 as f32,
            descent: fonts::HELVETICA_METRICS.1 as f32,
        },
        FontFamily::Serif => FontSpec {
            res: "TiRo",
            base: "Times-Roman",
            widths: &fonts::TIMES_ROMAN_WIDTHS,
            ascent: fonts::TIMES_ROMAN_METRICS.0 as f32,
            descent: fonts::TIMES_ROMAN_METRICS.1 as f32,
        },
        FontFamily::Mono => FontSpec {
            res: "Cour",
            base: "Courier",
            widths: &fonts::COURIER_WIDTHS,
            ascent: fonts::COURIER_METRICS.0 as f32,
            descent: fonts::COURIER_METRICS.1 as f32,
        },
    }
}

pub fn font_from_res(name: &str) -> FontFamily {
    match name {
        "TiRo" | "Times-Roman" | "Times" => FontFamily::Serif,
        "Cour" | "Courier" => FontFamily::Mono,
        _ => FontFamily::Sans,
    }
}

/// Encodage WinAnsi (les caractères absents deviennent « ? »).
pub fn encode_winansi(s: &str) -> Vec<u8> {
    s.chars().map(|c| fonts::winansi(c).unwrap_or(b'?')).collect()
}

pub fn text_width(bytes: &[u8], font: &FontSpec, size: f32) -> f32 {
    bytes.iter().map(|&b| font.widths[b as usize] as f32).sum::<f32>() * size / 1000.0
}

/// Découpe un texte en lignes tenant dans `width` points.
pub fn wrap(text: &str, font: &FontSpec, size: f32, width: f32) -> Vec<Vec<u8>> {
    let mut lines = vec![];
    for para in text.split('\n') {
        let mut line: Vec<u8> = vec![];
        for word in para.split(' ') {
            let w = encode_winansi(word);
            let candidate: Vec<u8> = if line.is_empty() {
                w.clone()
            } else {
                [line.as_slice(), b" ", &w].concat()
            };
            if text_width(&candidate, font, size) <= width || line.is_empty() && text_width(&w, font, size) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // Mot plus long que la ligne : coupure par caractère.
            for b in w {
                let mut c = line.clone();
                c.push(b);
                if text_width(&c, font, size) > width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(b);
            }
        }
        lines.push(line);
    }
    lines
}

pub const FREETEXT_PAD: f32 = 2.0;

/// `/Matrix` du formulaire d'apparence : repère local → espace utilisateur.
pub fn form_matrix(to_display: &Affine) -> [f32; 6] {
    let [a, b, c, d] = to_display.snapped_linear();
    // repère local = [[a, c], [-b, -d]] · utilisateur ; on prend l'inverse.
    let (p, q, r, s) = (a, c, -b, -d);
    let det = p * s - q * r;
    [s / det, -r / det, -q / det, p / det, 0.0, 0.0]
}

fn font_dict(f: &FontSpec) -> Object {
    Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => f.base, "Encoding" => "WinAnsiEncoding",
    })
}

/// Construit le flux d'apparence. `image` : XObject image déjà alloué pour les tampons image.
pub fn build(annot: &Annot, to_display: &Affine, alloc: &mut Alloc, image: Option<ObjectId>) -> Stream {
    let r = annot.rect;
    let lc = Local { r };
    let color = parse_color(&annot.color);
    let mut o = Ops(Vec::with_capacity(256));
    let mut res = Dictionary::new();
    let w = annot.width.max(0.1);

    match &annot.body {
        AnnotBody::Highlight { quads } => {
            res.set(
                "ExtGState",
                dictionary! { "GS0" => dictionary! { "BM" => "Multiply", "CA" => annot.opacity, "ca" => annot.opacity } },
            );
            o.op("/GS0 gs").rgb(color, false);
            for q in quads {
                let (x, y, qw, qh) = lc.rect(q);
                o.re(x, y, qw, qh);
            }
            o.op("f");
        }
        AnnotBody::Underline { quads } | AnnotBody::StrikeOut { quads } => {
            let strike = matches!(annot.body, AnnotBody::StrikeOut { .. });
            o.rgb(color, true);
            for q in quads {
                let (x, y, qw, qh) = lc.rect(q);
                let lw = (qh * if strike { 0.06 } else { 0.07 }).max(0.75);
                let ly = if strike { y + qh * 0.45 } else { y + qh * 0.1 };
                o.n(lw).op("w").m((x, ly)).l((x + qw, ly)).op("S");
            }
        }
        AnnotBody::Square => {
            o.rgb(color, true).n(w).op("w");
            o.re(w / 2.0, w / 2.0, (r.w - w).max(0.0), (r.h - w).max(0.0)).op("S");
        }
        AnnotBody::Circle => {
            o.rgb(color, true).n(w).op("w");
            o.ellipse(r.w / 2.0, r.h / 2.0, ((r.w - w) / 2.0).max(0.0), ((r.h - w) / 2.0).max(0.0))
                .op("S");
        }
        AnnotBody::Line { from, to, arrow } => {
            let a = lc.p(from.x, from.y);
            let b = lc.p(to.x, to.y);
            o.rgb(color, true).n(w).op("w").op("1 J 1 j").m(a).l(b).op("S");
            if *arrow {
                for (x, y) in arrow_head(a, b, w) {
                    o.m((x, y)).l(b);
                }
                o.op("S");
            }
        }
        AnnotBody::FreeText { text, font, size } => {
            let f = font_spec(*font);
            res.set("Font", dictionary! { f.res => alloc(font_dict(&f)) });
            let lines = wrap(text, &f, *size, (r.w - 2.0 * FREETEXT_PAD).max(1.0));
            let leading = size * 1.2;
            let first = r.h - FREETEXT_PAD - f.ascent / 1000.0 * size;
            o.op("/Tx BMC").op("q").re(0.0, 0.0, r.w, r.h).op("W n").op("BT");
            let _ = write!(o.0, "/{} ", f.res);
            o.n(*size)
                .op("Tf")
                .rgb(color, false)
                .n(leading)
                .op("TL")
                .n(FREETEXT_PAD)
                .n(first)
                .op("Td");
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    o.op("T*");
                }
                write_string(&mut o.0, line, lopdf::StringFormat::Literal);
                o.op(" Tj");
            }
            o.op("ET").op("Q").op("EMC");
        }
        AnnotBody::Note => {
            // Pastille arrondie + bulle (planche 03).
            let s = r.w.min(r.h);
            let k = s / NOTE_SIZE;
            o.rgb(color, false);
            rounded_rect(&mut o, 0.0, r.h - s, s, s, 4.0 * k).op("f");
            o.rgb(NOTE_ICON_INK, true).n(1.4 * k).op("w").op("1 J 1 j");
            let p = |x: f32, y: f32| (x * k, r.h - s + y * k);
            o.m(p(5.5, 15.5))
                .l(p(16.5, 15.5))
                .l(p(16.5, 8.0))
                .l(p(10.0, 8.0))
                .l(p(7.0, 5.5))
                .l(p(7.0, 8.0))
                .l(p(5.5, 8.0))
                .op("h S");
        }
        AnnotBody::Check { style } => {
            let s = r.w.min(r.h);
            let (ox, oy) = ((r.w - s) / 2.0, (r.h - s) / 2.0);
            let p = |x: f32, y: f32| (ox + x / 24.0 * s, oy + (24.0 - y) / 24.0 * s);
            match style {
                CheckStyle::Check => {
                    o.rgb(color, true)
                        .n(w)
                        .op("w")
                        .op("1 J 1 j")
                        .m(p(20.0, 6.0))
                        .l(p(9.0, 17.0))
                        .l(p(4.0, 12.0))
                        .op("S");
                }
                CheckStyle::Cross => {
                    o.rgb(color, true).n(w).op("w").op("1 J 1 j");
                    o.m(p(18.0, 6.0)).l(p(6.0, 18.0)).m(p(6.0, 6.0)).l(p(18.0, 18.0)).op("S");
                }
                CheckStyle::Dot => {
                    o.rgb(color, false).ellipse(r.w / 2.0, r.h / 2.0, s * 0.3, s * 0.3).op("f");
                }
            }
        }
        AnnotBody::Image { .. } => {
            if let Some(img) = image {
                res.set("XObject", dictionary! { "Im0" => img });
                o.op("q").n(r.w).n(0.0).n(0.0).n(r.h).n(0.0).n(0.0).op("cm /Im0 Do Q");
            }
        }
        AnnotBody::Redact { quads } => {
            // Apparence de marquage (avant application) : contour rouge, intérieur hachuré léger.
            o.rgb([0.88, 0.19, 0.19], true).n(1.0).op("w");
            for q in quads {
                let (x, y, qw, qh) = lc.rect(q);
                o.re(x + 0.5, y + 0.5, (qw - 1.0).max(0.0), (qh - 1.0).max(0.0));
            }
            o.op("S");
        }
        AnnotBody::Other { .. } => {}
    }

    let m = form_matrix(to_display);
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1,
        "BBox" => vec![0.into(), 0.into(), Object::Real(r.w), Object::Real(r.h)],
        "Matrix" => m.iter().map(|v| Object::Real(*v)).collect::<Vec<_>>(),
        "Resources" => res,
    };
    let mut s = Stream::new(dict, o.0);
    let _ = s.compress();
    s
}

fn rounded_rect(o: &mut Ops, x: f32, y: f32, w: f32, h: f32, rad: f32) -> &mut Ops {
    const K: f32 = 0.552_284_8 * 1.0;
    let r = rad.min(w / 2.0).min(h / 2.0);
    let c = r * (1.0 - K);
    o.m((x + r, y)).l((x + w - r, y));
    o.n(x + w - c).n(y).n(x + w).n(y + c).n(x + w).n(y + r).op("c");
    o.l((x + w, y + h - r));
    o.n(x + w).n(y + h - c).n(x + w - c).n(y + h).n(x + w - r).n(y + h).op("c");
    o.l((x + r, y + h));
    o.n(x + c).n(y + h).n(x).n(y + h - c).n(x).n(y + h - r).op("c");
    o.l((x, y + r));
    o.n(x).n(y + c).n(x + c).n(y).n(x + r).n(y).op("c");
    o
}

/// Longueur de la pointe de flèche pour une épaisseur donnée.
pub fn arrow_len(width: f32) -> f32 {
    (width * 3.0 + 6.0).max(8.0)
}

/// Les deux extrémités d'une pointe ouverte (30°) en `b`, pour un segment `a → b`.
pub fn arrow_head(a: (f32, f32), b: (f32, f32), width: f32) -> [(f32, f32); 2] {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / len, dy / len);
    let l = arrow_len(width);
    let (c, s) = (30f32.to_radians().cos(), 30f32.to_radians().sin());
    [
        (b.0 - l * (ux * c - uy * s), b.1 - l * (uy * c + ux * s)),
        (b.0 - l * (ux * c + uy * s), b.1 - l * (uy * c - ux * s)),
    ]
}

/// Rectangle d'une ligne (pointe et épaisseur comprises).
pub fn line_rect(from: Point, to: Point, width: f32, arrow: bool) -> Rect {
    let pad = width / 2.0 + if arrow { arrow_len(width) } else { 0.0 } + 2.0;
    let (x0, y0) = (from.x.min(to.x) - pad, from.y.min(to.y) - pad);
    let (x1, y1) = (from.x.max(to.x) + pad, from.y.max(to.y) + pad);
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_text_to_width() {
        let f = font_spec(FontFamily::Sans);
        let lines = wrap("Montant à confirmer avec la direction financière", &f, 12.0, 120.0);
        assert!(lines.len() >= 2);
        for l in &lines {
            assert!(text_width(l, &f, 12.0) <= 120.0, "{:?}", String::from_utf8_lossy(l));
        }
        assert_eq!(wrap("a\n\nb", &f, 12.0, 100.0).len(), 3);
        // « à » et « è » existent en WinAnsi ; « 漢 » devient « ? ».
        assert_eq!(encode_winansi("àè漢"), vec![0xe0, 0xe8, b'?']);
    }

    #[test]
    fn matrix_compensates_page_rotation() {
        // Page non tournée : y d'affichage = hauteur − y utilisateur.
        let flip = Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: 0.0,
            f: 842.0,
        };
        assert_eq!(form_matrix(&flip), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        // Page /Rotate 90 : x d'affichage = y utilisateur, y d'affichage = x utilisateur.
        let rot = Affine {
            a: 0.0,
            b: 1.0,
            c: 1.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        };
        let m = form_matrix(&rot);
        // Le vecteur local (1, 0) doit pointer vers +x d'affichage, donc +y utilisateur.
        assert_eq!((m[0], m[1]), (0.0, 1.0));
    }
}
