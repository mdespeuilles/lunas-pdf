//! Caviardage réel : suppression du contenu sous les annotations `/Redact`.
//!
//! - Texte : les glyphes recouverts sont retirés des opérateurs `Tj`/`TJ`/`'`/`"` et remplacés
//!   par un décalage équivalent (le reste de la ligne ne bouge pas).
//! - Tracés : segments retirés pour les traits, sous-chemins entiers pour les remplissages.
//! - Images : pixels recouverts noircis (DCT, Flate, sans filtre ; 8 bits ou masque 1 bit),
//!   sinon image entière retirée. Images en ligne recouvertes : retirées.
//! - XObjects formulaires : traités récursivement (copie propre à la page).
//! - Annotations recouvertes et `/ActualText`, `/Alt` du contenu marqué : supprimés.
//! - Rectangles opaques peints par-dessus, puis **réécriture complète** du fichier : aucune
//!   révision antérieure ni objet orphelin ne subsiste.

use std::collections::HashMap;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

use crate::error::{Error, Result};
use crate::fonts;
use crate::pdfwrite::{write_name, write_object};

/// Zone à caviarder en espace utilisateur.
#[derive(Debug, Clone, Copy)]
struct Area {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [f32; 3],
}

impl Area {
    fn intersects(&self, b: BBox) -> bool {
        b.0 < self.x1 && b.2 > self.x0 && b.1 < self.y1 && b.3 > self.y0
    }
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

type M = [f32; 6];
/// Boîte englobante (x0, y0, x1, y1) en espace utilisateur.
type BBox = (f32, f32, f32, f32);
/// Segment de chemin : opération, boîte, début de sous-chemin, point de départ.
type Seg = (Operation, Option<BBox>, bool, (f32, f32));
const ID: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Produit matriciel PDF (vecteurs ligne) : a puis b.
fn mul(a: &M, b: &M) -> M {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn tp(m: &M, x: f32, y: f32) -> (f32, f32) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

fn bbox(m: &M, x0: f32, y0: f32, x1: f32, y1: f32) -> BBox {
    let pts = [tp(m, x0, y0), tp(m, x1, y0), tp(m, x0, y1), tp(m, x1, y1)];
    pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| {
        (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
    })
}

fn num(o: &Object) -> f32 {
    o.as_float().unwrap_or(0.0)
}

fn matrix(ops: &[Object]) -> Option<M> {
    (ops.len() == 6).then(|| {
        [
            num(&ops[0]),
            num(&ops[1]),
            num(&ops[2]),
            num(&ops[3]),
            num(&ops[4]),
            num(&ops[5]),
        ]
    })
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(o),
        _ => o,
    }
}

fn dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    resolve(doc, o).as_dict().ok()
}

// --- Polices ------------------------------------------------------------------------------------

#[derive(Clone)]
struct FontInfo {
    two_byte: bool,
    /// Largeurs en millièmes d'em.
    widths: HashMap<u32, f32>,
    default_w: f32,
    ascent: f32,
    descent: f32,
}

impl FontInfo {
    fn width(&self, code: u32) -> f32 {
        *self.widths.get(&code).unwrap_or(&self.default_w)
    }
    fn codes(&self, s: &[u8]) -> Vec<(u32, Vec<u8>)> {
        if self.two_byte {
            s.chunks(2)
                .map(|c| {
                    (
                        if c.len() == 2 {
                            (c[0] as u32) << 8 | c[1] as u32
                        } else {
                            c[0] as u32
                        },
                        c.to_vec(),
                    )
                })
                .collect()
        } else {
            s.iter().map(|&b| (b as u32, vec![b])).collect()
        }
    }
}

fn font_info(doc: &Document, fd: &Dictionary) -> FontInfo {
    let name = |d: &Dictionary, k: &[u8]| {
        d.get(k)
            .and_then(|o| o.as_name())
            .ok()
            .map(|n| String::from_utf8_lossy(n).into_owned())
    };
    let arr = |d: &Dictionary, k: &[u8]| -> Vec<Object> {
        d.get(k)
            .ok()
            .map(|o| resolve(doc, o))
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default()
    };
    let descriptor = |d: &Dictionary| d.get(b"FontDescriptor").ok().and_then(|o| dict(doc, o)).cloned();
    let metrics = |desc: Option<&Dictionary>| {
        let g = |k: &[u8], dflt: f32| {
            desc.and_then(|d| d.get(k).ok())
                .map(|o| num(resolve(doc, o)))
                .filter(|v| *v != 0.0)
                .unwrap_or(dflt)
        };
        (
            g(b"Ascent", 900.0).max(700.0) / 1000.0,
            g(b"Descent", -250.0).min(-150.0) / 1000.0,
        )
    };
    let subtype = name(fd, b"Subtype").unwrap_or_default();
    if subtype == "Type0" {
        let desc_font = arr(fd, b"DescendantFonts")
            .first()
            .and_then(|o| dict(doc, o))
            .cloned()
            .unwrap_or_default();
        let mut widths = HashMap::new();
        let w = arr(&desc_font, b"W");
        let mut i = 0;
        while i < w.len() {
            let first = num(&w[i]) as u32;
            match w.get(i + 1).map(|o| resolve(doc, o)) {
                Some(Object::Array(list)) => {
                    for (k, v) in list.iter().enumerate() {
                        widths.insert(first + k as u32, num(v));
                    }
                    i += 2;
                }
                Some(last) if i + 2 < w.len() => {
                    let (last, width) = (num(last) as u32, num(&w[i + 2]));
                    for c in first..=last.min(first + 65535) {
                        widths.insert(c, width);
                    }
                    i += 3;
                }
                _ => break,
            }
        }
        let default_w = desc_font.get(b"DW").map(|o| num(resolve(doc, o))).unwrap_or(1000.0);
        let (ascent, descent) = metrics(descriptor(&desc_font).as_ref());
        return FontInfo {
            two_byte: true,
            widths,
            default_w,
            ascent,
            descent,
        };
    }
    let first = fd.get(b"FirstChar").map(|o| num(resolve(doc, o)) as u32).unwrap_or(0);
    let mut scale = 1.0;
    if subtype == "Type3" {
        scale = arr(fd, b"FontMatrix").first().map(num).unwrap_or(0.001) * 1000.0;
    }
    let mut widths: HashMap<u32, f32> = arr(fd, b"Widths")
        .iter()
        .enumerate()
        .map(|(i, w)| (first + i as u32, num(resolve(doc, w)) * scale))
        .collect();
    let desc = descriptor(fd);
    let mut default_w = desc
        .as_ref()
        .and_then(|d| d.get(b"MissingWidth").ok())
        .map(|o| num(resolve(doc, o)))
        .unwrap_or(0.0);
    if widths.is_empty() {
        // Police standard 14 sans /Widths.
        let base = name(fd, b"BaseFont").unwrap_or_default();
        let table = if base.starts_with("Times") {
            Some(&fonts::TIMES_ROMAN_WIDTHS)
        } else if base.starts_with("Courier") {
            Some(&fonts::COURIER_WIDTHS)
        } else if base.starts_with("Helvetica") || base.starts_with("Arial") {
            Some(&fonts::HELVETICA_WIDTHS)
        } else {
            None
        };
        match table {
            Some(t) => widths = t.iter().enumerate().map(|(i, w)| (i as u32, *w as f32)).collect(),
            None => default_w = 600.0,
        }
    }
    if default_w == 0.0 {
        default_w = 500.0;
    }
    let (ascent, descent) = metrics(desc.as_ref());
    FontInfo {
        two_byte: false,
        widths,
        default_w,
        ascent,
        descent,
    }
}

// --- Traitement d'un flux de contenu ---------------------------------------------------------------

#[derive(Clone)]
struct GState {
    ctm: M,
    tc: f32,
    tw: f32,
    th: f32,
    tl: f32,
    tfs: f32,
    rise: f32,
    font: Option<FontInfo>,
}

struct Ctx<'a> {
    doc: &'a mut Document,
    areas: &'a [Area],
    fonts: HashMap<ObjectId, FontInfo>,
    counter: u32,
}

/// Retire des ressources les XObjects remplacés qui ne sont plus appelés par le contenu.
fn drop_unused_xobjects(doc: &Document, res: &mut Dictionary, ops: &[Operation], replaced: &[Vec<u8>]) {
    if replaced.is_empty() {
        return;
    }
    let used: Vec<&[u8]> = ops
        .iter()
        .filter(|o| o.operator == "Do")
        .filter_map(|o| o.operands.first()?.as_name().ok())
        .collect();
    let mut x = res
        .get(b"XObject")
        .ok()
        .and_then(|x| dict(doc, x))
        .cloned()
        .unwrap_or_default();
    for name in replaced {
        if !used.contains(&name.as_slice()) {
            x.remove(name);
        }
    }
    res.set("XObject", x);
}

/// Résultat d'un passage : opérations réécrites et ressources (modifiées ou non).
struct Processed {
    ops: Vec<Operation>,
    resources: Dictionary,
    changed: bool,
}

fn op(name: &str, operands: Vec<Object>) -> Operation {
    Operation::new(name, operands)
}

impl Ctx<'_> {
    fn font(&mut self, res: &Dictionary, name: &[u8]) -> Option<FontInfo> {
        let fonts_obj = res.get(b"Font").ok()?;
        let fonts = dict(self.doc, fonts_obj)?;
        let entry = fonts.get(name).ok()?;
        let key = entry.as_reference().ok();
        if let Some(k) = key
            && let Some(f) = self.fonts.get(&k)
        {
            return Some(f.clone());
        }
        let fd = dict(self.doc, entry)?.clone();
        let info = font_info(self.doc, &fd);
        if let Some(k) = key {
            self.fonts.insert(k, info.clone());
        }
        Some(info)
    }

    fn hit_glyph(&self, b: BBox) -> bool {
        let (w, h) = ((b.2 - b.0).max(1e-3), (b.3 - b.1).max(1e-3));
        self.areas.iter().any(|a| {
            let ox = (b.2.min(a.x1) - b.0.max(a.x0)).max(0.0);
            let oy = (b.3.min(a.y1) - b.1.max(a.y0)).max(0.0);
            ox >= 0.5 * w && oy >= 0.4 * h
        })
    }

    fn hit(&self, b: BBox) -> bool {
        self.areas.iter().any(|a| a.intersects(b))
    }

    fn process(&mut self, ops: Vec<Operation>, res: &Dictionary, ctm0: M) -> Processed {
        let mut out: Vec<Operation> = Vec::with_capacity(ops.len());
        let mut res = res.clone();
        let mut changed = false;
        let mut replaced: Vec<Vec<u8>> = vec![];
        let mut gs = GState {
            ctm: ctm0,
            tc: 0.0,
            tw: 0.0,
            th: 1.0,
            tl: 0.0,
            tfs: 0.0,
            rise: 0.0,
            font: None,
        };
        let mut stack: Vec<GState> = vec![];
        let (mut tm, mut tlm) = (ID, ID);
        // Chemin en cours : (opération, boîte du segment, début de sous-chemin, point de départ).
        let mut path: Vec<Seg> = vec![];
        let mut cur = (0.0f32, 0.0f32);
        let mut start = (0.0f32, 0.0f32);

        for o in ops {
            let a = &o.operands;
            match o.operator.as_str() {
                "q" => stack.push(gs.clone()),
                "Q" => gs = stack.pop().unwrap_or(gs.clone()),
                "cm" => {
                    if let Some(m) = matrix(a) {
                        gs.ctm = mul(&m, &gs.ctm);
                    }
                }
                "BT" => {
                    tm = ID;
                    tlm = ID;
                }
                "Tf" if a.len() == 2 => {
                    gs.font = a[0].as_name().ok().and_then(|n| self.font(&res, n));
                    gs.tfs = num(&a[1]);
                }
                "Tc" if a.len() == 1 => gs.tc = num(&a[0]),
                "Tw" if a.len() == 1 => gs.tw = num(&a[0]),
                "Tz" if a.len() == 1 => gs.th = num(&a[0]) / 100.0,
                "TL" if a.len() == 1 => gs.tl = num(&a[0]),
                "Ts" if a.len() == 1 => gs.rise = num(&a[0]),
                "Td" | "TD" if a.len() == 2 => {
                    if o.operator == "TD" {
                        gs.tl = -num(&a[1]);
                    }
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, num(&a[0]), num(&a[1])], &tlm);
                    tm = tlm;
                }
                "Tm" => {
                    if let Some(m) = matrix(a) {
                        tlm = m;
                        tm = m;
                    }
                }
                "T*" => {
                    tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                    tm = tlm;
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    let mut pre = vec![];
                    let items: Vec<Object> = match o.operator.as_str() {
                        "Tj" => a.first().cloned().into_iter().collect(),
                        "TJ" => a.first().and_then(|x| x.as_array().ok()).cloned().unwrap_or_default(),
                        "'" => {
                            tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                            tm = tlm;
                            pre.push(op("T*", vec![]));
                            a.first().cloned().into_iter().collect()
                        }
                        _ => {
                            if a.len() == 3 {
                                gs.tw = num(&a[0]);
                                gs.tc = num(&a[1]);
                                pre.push(op("Tw", vec![a[0].clone()]));
                                pre.push(op("Tc", vec![a[1].clone()]));
                            }
                            tlm = mul(&[1.0, 0.0, 0.0, 1.0, 0.0, -gs.tl], &tlm);
                            tm = tlm;
                            pre.push(op("T*", vec![]));
                            a.get(2).cloned().into_iter().collect()
                        }
                    };
                    match self.show_text(&items, &gs, &mut tm) {
                        Some(new_items) => {
                            changed = true;
                            out.extend(pre);
                            out.push(op("TJ", vec![Object::Array(new_items)]));
                        }
                        None => out.push(o),
                    }
                    continue;
                }
                "BDC" => {
                    // Texte de remplacement susceptible de contenir le texte caviardé.
                    if let Some(Object::Dictionary(d)) = a.get(1)
                        && (d.has(b"ActualText") || d.has(b"Alt") || d.has(b"E"))
                    {
                        let mut d = d.clone();
                        d.remove(b"ActualText");
                        d.remove(b"Alt");
                        d.remove(b"E");
                        changed = true;
                        out.push(op("BDC", vec![a[0].clone(), Object::Dictionary(d)]));
                        continue;
                    }
                }
                // --- Tracés ---
                "m" if a.len() == 2 => {
                    cur = tp(&gs.ctm, num(&a[0]), num(&a[1]));
                    start = cur;
                    path.push((o, None, true, cur));
                    continue;
                }
                "l" | "c" | "v" | "y" => {
                    let pts: Vec<(f32, f32)> = a
                        .chunks(2)
                        .filter(|c| c.len() == 2)
                        .map(|c| tp(&gs.ctm, num(&c[0]), num(&c[1])))
                        .collect();
                    let from = cur;
                    let all: Vec<(f32, f32)> = std::iter::once(from).chain(pts.iter().copied()).collect();
                    let b = all.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| {
                        (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
                    });
                    cur = *pts.last().unwrap_or(&cur);
                    path.push((o, Some(b), false, from));
                    continue;
                }
                "h" => {
                    let from = cur;
                    let b = (
                        from.0.min(start.0),
                        from.1.min(start.1),
                        from.0.max(start.0),
                        from.1.max(start.1),
                    );
                    cur = start;
                    path.push((o, Some(b), false, from));
                    continue;
                }
                "re" if a.len() == 4 => {
                    let (x, y, w, h) = (num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3]));
                    let b = bbox(&gs.ctm, x, y, x + w, y + h);
                    cur = tp(&gs.ctm, x, y);
                    start = cur;
                    path.push((o, Some(b), true, cur));
                    continue;
                }
                "W" | "W*" => {
                    path.push((o, None, false, cur));
                    continue;
                }
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    let segs = std::mem::take(&mut path);
                    let paint = o.operator != "n";
                    let stroke_only = matches!(o.operator.as_str(), "S" | "s");
                    if paint && segs.iter().any(|s| s.1.is_some_and(|b| self.hit(b))) {
                        changed = true;
                        let kept = if stroke_only {
                            self.cut_strokes(segs, &gs.ctm)
                        } else {
                            self.cut_fills(segs)
                        };
                        if kept.iter().any(|k| k.operator != "W" && k.operator != "W*") {
                            out.extend(kept);
                            out.push(o);
                        } else {
                            out.extend(kept.into_iter().filter(|k| k.operator == "W" || k.operator == "W*"));
                            if out.last().is_some_and(|k| k.operator.starts_with('W')) {
                                out.push(op("n", vec![]));
                            }
                        }
                    } else {
                        out.extend(segs.into_iter().map(|s| s.0));
                        out.push(o);
                    }
                    continue;
                }
                // --- Images et formulaires ---
                "BI" => {
                    if self.hit(bbox(&gs.ctm, 0.0, 0.0, 1.0, 1.0)) {
                        changed = true;
                        continue;
                    }
                }
                "Do" if a.len() == 1 => {
                    let Ok(name) = a[0].as_name() else {
                        out.push(o);
                        continue;
                    };
                    let name = name.to_vec();
                    let xobj = res
                        .get(b"XObject")
                        .ok()
                        .and_then(|x| dict(self.doc, x))
                        .and_then(|x| x.get(&name).ok())
                        .cloned();
                    let Some(xref) = xobj else {
                        out.push(o);
                        continue;
                    };
                    let Ok(Object::Stream(stream)) = resolve(self.doc, &xref)
                        .clone()
                        .as_stream()
                        .map(|s| Object::Stream(s.clone()))
                    else {
                        out.push(o);
                        continue;
                    };
                    let sub = stream.dict.get(b"Subtype").and_then(|s| s.as_name()).unwrap_or(b"");
                    if sub == b"Image" {
                        if !self.hit(bbox(&gs.ctm, 0.0, 0.0, 1.0, 1.0)) {
                            out.push(o);
                            continue;
                        }
                        changed = true;
                        replaced.push(name.clone());
                        // Image non décodable : retirée entièrement.
                        if let Some(new) = redact_image(&stream, &gs.ctm, self.areas) {
                            let id = self.doc.add_object(Object::Stream(new));
                            let nm = self.new_xobject(&mut res, id);
                            out.push(op("Do", vec![Object::Name(nm)]));
                        }
                        continue;
                    }
                    if sub == b"Form" {
                        let fm = stream
                            .dict
                            .get(b"Matrix")
                            .ok()
                            .and_then(|m| m.as_array().ok())
                            .and_then(|m| matrix(m))
                            .unwrap_or(ID);
                        let fbbox = stream
                            .dict
                            .get(b"BBox")
                            .ok()
                            .and_then(|b| b.as_array().ok())
                            .map(|b| b.iter().map(num).collect::<Vec<_>>());
                        let fctm = mul(&fm, &gs.ctm);
                        if let Some(b) = fbbox.filter(|b| b.len() == 4)
                            && !self.hit(bbox(&fctm, b[0].min(b[2]), b[1].min(b[3]), b[0].max(b[2]), b[1].max(b[3])))
                        {
                            out.push(o);
                            continue;
                        }
                        let content = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
                        let fres = stream
                            .dict
                            .get(b"Resources")
                            .ok()
                            .and_then(|r| dict(self.doc, r))
                            .cloned()
                            .unwrap_or_else(|| res.clone());
                        let Ok(parsed) = Content::decode(&content) else {
                            out.push(o);
                            continue;
                        };
                        let p = self.process(parsed.operations, &fres, fctm);
                        if !p.changed {
                            out.push(o);
                            continue;
                        }
                        changed = true;
                        replaced.push(name.clone());
                        let mut d = stream.dict.clone();
                        d.remove(b"Filter");
                        d.remove(b"DecodeParms");
                        d.set("Resources", p.resources);
                        let mut s = Stream::new(d, encode_ops(&p.ops));
                        let _ = s.compress();
                        let id = self.doc.add_object(Object::Stream(s));
                        let nm = self.new_xobject(&mut res, id);
                        out.push(op("Do", vec![Object::Name(nm)]));
                        continue;
                    }
                }
                _ => {}
            }
            out.push(o);
        }
        // Chemin non terminé (contenu malformé) : restitué tel quel.
        out.extend(path.into_iter().map(|s| s.0));
        drop_unused_xobjects(self.doc, &mut res, &out, &replaced);
        Processed {
            ops: out,
            resources: res,
            changed,
        }
    }

    /// Ajoute un XObject sous un nom inédit dans les ressources (copie propre à cette page).
    fn new_xobject(&mut self, res: &mut Dictionary, id: ObjectId) -> Vec<u8> {
        self.counter += 1;
        let name = format!("FeuilletR{}", self.counter).into_bytes();
        let mut x = res
            .get(b"XObject")
            .ok()
            .and_then(|x| dict(self.doc, x))
            .cloned()
            .unwrap_or_default();
        x.set(name.clone(), id);
        res.set("XObject", x);
        name
    }

    /// Glyphes d'un affichage de texte ; `None` si rien n'est caviardé.
    fn show_text(&self, items: &[Object], gs: &GState, tm: &mut M) -> Option<Vec<Object>> {
        let font = gs.font.as_ref()?;
        let mut out: Vec<Object> = vec![];
        let mut kept: Vec<u8> = vec![];
        let mut removed_any = false;
        let flush = |out: &mut Vec<Object>, kept: &mut Vec<u8>| {
            if !kept.is_empty() {
                out.push(Object::String(std::mem::take(kept), StringFormat::Hexadecimal));
            }
        };
        let push_adjust = |out: &mut Vec<Object>, v: f32| {
            if let Some(Object::Real(prev)) = out.last_mut() {
                *prev += v;
            } else {
                out.push(Object::Real(v));
            }
        };
        for item in items {
            match item {
                Object::String(s, _) => {
                    for (code, bytes) in font.codes(s) {
                        let w0 = font.width(code) / 1000.0;
                        let trm = mul(&[gs.tfs * gs.th, 0.0, 0.0, gs.tfs, 0.0, gs.rise], &mul(tm, &gs.ctm));
                        let b = bbox(&trm, 0.0, font.descent, w0.max(0.05), font.ascent);
                        let space = if !font.two_byte && code == 32 { gs.tw } else { 0.0 };
                        let adv = (w0 * gs.tfs + gs.tc + space) * gs.th;
                        if self.hit_glyph(b) {
                            removed_any = true;
                            flush(&mut out, &mut kept);
                            if gs.tfs != 0.0 && gs.th != 0.0 {
                                push_adjust(&mut out, -(adv / (gs.tfs * gs.th)) * 1000.0);
                            }
                        } else {
                            kept.extend_from_slice(&bytes);
                        }
                        *tm = mul(&[1.0, 0.0, 0.0, 1.0, adv, 0.0], tm);
                    }
                }
                n => {
                    let v = num(n);
                    flush(&mut out, &mut kept);
                    push_adjust(&mut out, v);
                    *tm = mul(&[1.0, 0.0, 0.0, 1.0, -v / 1000.0 * gs.tfs * gs.th, 0.0], tm);
                }
            }
        }
        flush(&mut out, &mut kept);
        removed_any.then_some(out)
    }

    /// Traits : retire les segments touchés, reprend par un `m` après chaque trou.
    fn cut_strokes(&self, segs: Vec<Seg>, ctm: &M) -> Vec<Operation> {
        let inv = invert(ctm);
        let mut out = vec![];
        let mut gap = false;
        for (o, b, _, from) in segs {
            match (o.operator.as_str(), b) {
                ("re", Some(_)) => {
                    // Rectangle tracé : décomposé en quatre segments.
                    let a = &o.operands;
                    let (x, y, w, h) = (num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3]));
                    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)];
                    let mut need_move = true;
                    for pair in corners.windows(2) {
                        let p0 = tp(ctm, pair[0].0, pair[0].1);
                        let p1 = tp(ctm, pair[1].0, pair[1].1);
                        let sb = (p0.0.min(p1.0), p0.1.min(p1.1), p0.0.max(p1.0), p0.1.max(p1.1));
                        if self.hit(sb) {
                            need_move = true;
                            continue;
                        }
                        if need_move {
                            out.push(op("m", vec![Object::Real(pair[0].0), Object::Real(pair[0].1)]));
                            need_move = false;
                        }
                        out.push(op("l", vec![Object::Real(pair[1].0), Object::Real(pair[1].1)]));
                    }
                    gap = true;
                }
                (_, Some(sb)) if self.hit(sb) => gap = true,
                (_, Some(_)) => {
                    if gap {
                        let (ux, uy) = tp(&inv, from.0, from.1);
                        out.push(op("m", vec![Object::Real(ux), Object::Real(uy)]));
                        gap = false;
                    }
                    if o.operator == "h" {
                        // La fermeture devient un segment explicite (le début a pu disparaître).
                        continue;
                    }
                    out.push(o);
                }
                _ => {
                    gap = false;
                    out.push(o);
                }
            }
        }
        out
    }

    /// Remplissages : retire chaque sous-chemin touché.
    fn cut_fills(&self, segs: Vec<Seg>) -> Vec<Operation> {
        let mut out = vec![];
        let mut sub: Vec<(Operation, Option<BBox>)> = vec![];
        let flush = |sub: &mut Vec<(Operation, Option<BBox>)>, out: &mut Vec<Operation>| {
            if !sub.iter().any(|s| s.1.is_some_and(|b| self.hit(b))) {
                out.extend(sub.drain(..).map(|s| s.0));
            } else {
                sub.clear();
            }
        };
        for (o, b, starts, _) in segs {
            if o.operator.starts_with('W') {
                flush(&mut sub, &mut out);
                out.push(o);
                continue;
            }
            if starts {
                flush(&mut sub, &mut out);
            }
            sub.push((o, b));
        }
        flush(&mut sub, &mut out);
        out
    }
}

fn invert(m: &M) -> M {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-9 {
        return ID;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    [a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)]
}

/// Encodeur de contenu (celui de lopdf ne sait pas réécrire les images en ligne).
fn encode_ops(ops: &[Operation]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ops.len() * 16);
    for o in ops {
        if o.operator == "BI" {
            if let Some(Object::Stream(s)) = o.operands.first() {
                out.extend_from_slice(b"BI ");
                for (k, v) in s.dict.iter() {
                    write_name(&mut out, k);
                    out.push(b' ');
                    write_object(&mut out, v);
                    out.push(b' ');
                }
                out.extend_from_slice(b"ID ");
                out.extend_from_slice(&s.content);
                out.extend_from_slice(b"\nEI\n");
            }
            continue;
        }
        for v in &o.operands {
            write_object(&mut out, v);
            out.push(b' ');
        }
        out.extend_from_slice(o.operator.as_bytes());
        out.push(b'\n');
    }
    out
}

// --- Images -------------------------------------------------------------------------------------

/// Noircit les pixels recouverts ; `None` si le format n'est pas pris en charge.
fn redact_image(s: &Stream, ctm: &M, areas: &[Area]) -> Option<Stream> {
    let d = &s.dict;
    let w = d.get(b"Width").ok()?.as_i64().ok()? as usize;
    let h = d.get(b"Height").ok()?.as_i64().ok()? as usize;
    let is_mask = d.get(b"ImageMask").and_then(|o| o.as_bool()).unwrap_or(false);
    let bpc = if is_mask {
        1
    } else {
        d.get(b"BitsPerComponent").ok()?.as_i64().ok()? as usize
    };
    let filters: Vec<Vec<u8>> = match d.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_name().ok().map(<[u8]>::to_vec)).collect(),
        _ => vec![],
    };
    let dct = filters.last().is_some_and(|f| f == b"DCTDecode");
    let (mut data, channels): (Vec<u8>, usize) = if dct {
        let mut raw = s.clone();
        if filters.len() > 1 {
            // Filtres préalables (ex. Flate puis DCT) : on ne garde que les données JPEG.
            raw.dict.set(
                "Filter",
                Object::Array(filters[..filters.len() - 1].iter().map(|f| Object::Name(f.clone())).collect()),
            );
            raw.content = raw.decompressed_content().ok()?;
        }
        let img = image::load_from_memory_with_format(&raw.content, image::ImageFormat::Jpeg).ok()?;
        if img.color().channel_count() == 1 {
            (img.to_luma8().into_raw(), 1)
        } else {
            (img.to_rgb8().into_raw(), 3)
        }
    } else {
        let raw = if filters.is_empty() {
            s.content.clone()
        } else {
            s.decompressed_content().ok()?
        };
        let channels = if is_mask {
            1
        } else {
            match d.get(b"ColorSpace").ok()?.as_name().ok() {
                Some(b"DeviceGray") => 1,
                Some(b"DeviceRGB") => 3,
                Some(b"DeviceCMYK") => 4,
                _ => return None,
            }
        };
        if !(bpc == 8 || (bpc == 1 && channels == 1)) {
            return None;
        }
        (raw, channels)
    };
    let row = if bpc == 1 { w.div_ceil(8) } else { w * channels };
    if data.len() < row * h {
        return None;
    }
    // Valeur « noire » (ou « transparente » pour un masque : 1 = non peint par défaut).
    let decode_inverted = d
        .get(b"Decode")
        .ok()
        .and_then(|o| o.as_array().ok())
        .and_then(|a| a.first())
        .is_some_and(|v| num(v) == 1.0);
    let black: u8 = match (is_mask, channels) {
        (true, _) => u8::from(!decode_inverted),
        (false, 4) => 255,
        _ => 0,
    };
    let inv = invert(ctm);
    for a in areas {
        // Rectangle des pixels potentiellement concernés.
        let b = bbox(&inv, a.x0, a.y0, a.x1, a.y1);
        let (px0, px1) = (
            ((b.0.max(0.0)) * w as f32).floor() as usize,
            ((b.2.min(1.0)) * w as f32).ceil() as usize,
        );
        let (py0, py1) = (
            ((1.0 - b.3.min(1.0)) * h as f32).floor() as usize,
            ((1.0 - b.1.max(0.0)) * h as f32).ceil() as usize,
        );
        for py in py0..py1.min(h) {
            for px in px0..px1.min(w) {
                let (ux, uy) = tp(ctm, (px as f32 + 0.5) / w as f32, 1.0 - (py as f32 + 0.5) / h as f32);
                if !a.contains(ux, uy) {
                    continue;
                }
                if bpc == 1 {
                    let byte = &mut data[py * row + px / 8];
                    let bit = 7 - (px % 8);
                    if black == 1 { *byte |= 1 << bit } else { *byte &= !(1 << bit) }
                } else {
                    let i = py * row + px * channels;
                    match channels {
                        4 => data[i..i + 4].copy_from_slice(&[0, 0, 0, 255]),
                        n => data[i..i + n].fill(black),
                    }
                }
            }
        }
    }
    let mut nd = d.clone();
    nd.remove(b"DecodeParms");
    if dct {
        let mut jpeg = vec![];
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90);
        let color = if channels == 1 {
            image::ExtendedColorType::L8
        } else {
            image::ExtendedColorType::Rgb8
        };
        image::ImageEncoder::write_image(enc, &data, w as u32, h as u32, color).ok()?;
        nd.set("Filter", "DCTDecode");
        nd.set("ColorSpace", if channels == 1 { "DeviceGray" } else { "DeviceRGB" });
        nd.set("BitsPerComponent", 8);
        nd.remove(b"Decode");
        return Some(Stream::new(nd, jpeg));
    }
    nd.remove(b"Filter");
    let mut ns = Stream::new(nd, data);
    let _ = ns.compress();
    Some(ns)
}

// --- Document -----------------------------------------------------------------------------------

fn page_resources(doc: &Document, page: ObjectId) -> Dictionary {
    let mut cur = doc.get_dictionary(page).ok();
    for _ in 0..32 {
        let Some(d) = cur else { break };
        if let Ok(r) = d.get(b"Resources")
            && let Some(rd) = dict(doc, r)
        {
            return rd.clone();
        }
        cur = d
            .get(b"Parent")
            .and_then(|p| p.as_reference())
            .ok()
            .and_then(|p| doc.get_dictionary(p).ok());
    }
    Dictionary::new()
}

fn quads_or_rect(doc: &Document, d: &Dictionary) -> Vec<BBox> {
    let nums = |k: &[u8]| -> Vec<f32> {
        d.get(k)
            .ok()
            .map(|o| resolve(doc, o))
            .and_then(|o| o.as_array().ok())
            .map(|a| a.iter().map(num).collect())
            .unwrap_or_default()
    };
    let q = nums(b"QuadPoints");
    let mut out: Vec<BBox> = q
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| {
            let xs = [c[0], c[2], c[4], c[6]];
            let ys = [c[1], c[3], c[5], c[7]];
            (
                xs.iter().copied().fold(f32::MAX, f32::min),
                ys.iter().copied().fold(f32::MAX, f32::min),
                xs.iter().copied().fold(f32::MIN, f32::max),
                ys.iter().copied().fold(f32::MIN, f32::max),
            )
        })
        .collect();
    if out.is_empty() {
        let r = nums(b"Rect");
        if r.len() == 4 {
            out.push((r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])));
        }
    }
    out
}

pub fn apply(bytes: &[u8], password: Option<&str>) -> Result<Vec<u8>> {
    let opts = lopdf::LoadOptions {
        password: password.map(str::to_owned),
        ..Default::default()
    };
    let mut doc = Document::load_mem_with_options(bytes, opts).map_err(|e| Error::Invalid(e.to_string()))?;
    let state = doc.encryption_state.clone();
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let mut removed_widgets: Vec<ObjectId> = vec![];
    let mut counter = 0;

    for pid in pages {
        let page = doc.get_dictionary(pid).map_err(|e| Error::Invalid(e.to_string()))?.clone();
        let annots: Vec<Object> = page
            .get(b"Annots")
            .ok()
            .map(|o| resolve(&doc, o))
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        let mut areas: Vec<Area> = vec![];
        for a in &annots {
            let Some(d) = dict(&doc, a) else { continue };
            if d.get(b"Subtype").and_then(|s| s.as_name()).ok() != Some(b"Redact") {
                continue;
            }
            let ic = d
                .get(b"IC")
                .ok()
                .and_then(|o| o.as_array().ok())
                .map(|a| a.iter().map(num).collect::<Vec<_>>());
            let color = match ic.as_deref() {
                Some([r, g, b]) => [*r, *g, *b],
                Some([g]) => [*g; 3],
                _ => [0.0; 3],
            };
            for (x0, y0, x1, y1) in quads_or_rect(&doc, d) {
                areas.push(Area { x0, y0, x1, y1, color });
            }
        }
        if areas.is_empty() {
            continue;
        }

        // Contenu de la page.
        let content = doc.get_page_content(pid);
        let ops = Content::decode(&content)
            .map_err(|e| Error::Invalid(format!("contenu de page illisible : {e}")))?
            .operations;
        let res = page_resources(&doc, pid);
        let mut ctx = Ctx {
            doc: &mut doc,
            areas: &areas,
            fonts: HashMap::new(),
            counter,
        };
        let mut p = ctx.process(ops, &res, ID);
        counter = ctx.counter;
        p.ops.insert(0, op("q", vec![]));
        p.ops.push(op("Q", vec![]));
        for a in &areas {
            p.ops.push(op("q", vec![]));
            p.ops.push(op("rg", a.color.iter().map(|v| Object::Real(*v)).collect()));
            p.ops.push(op(
                "re",
                vec![
                    Object::Real(a.x0),
                    Object::Real(a.y0),
                    Object::Real(a.x1 - a.x0),
                    Object::Real(a.y1 - a.y0),
                ],
            ));
            p.ops.push(op("f", vec![]));
            p.ops.push(op("Q", vec![]));
        }
        let mut s = Stream::new(Dictionary::new(), encode_ops(&p.ops));
        let _ = s.compress();
        let cid = doc.add_object(Object::Stream(s));

        // Annotations : on retire les zones de caviardage et tout ce qu'elles recouvrent.
        let mut kept = vec![];
        let mut removed: Vec<ObjectId> = vec![];
        for a in &annots {
            let Some(d) = dict(&doc, a) else { continue };
            let sub = d.get(b"Subtype").and_then(|s| s.as_name()).unwrap_or(b"").to_vec();
            let r = quads_or_rect(&doc, d);
            let covered = sub == b"Redact" || r.iter().any(|b| areas.iter().any(|ar| ar.intersects(*b)));
            if covered && sub != b"Popup" {
                if let Ok(id) = a.as_reference() {
                    removed.push(id);
                    if sub == b"Widget" {
                        removed_widgets.push(id);
                    }
                }
                continue;
            }
            kept.push(a.clone());
        }
        kept.retain(|a| {
            dict(&doc, a)
                .and_then(|d| d.get(b"Parent").ok())
                .and_then(|p| p.as_reference().ok())
                .is_none_or(|p| !removed.contains(&p))
        });

        let pd = doc.get_dictionary_mut(pid).map_err(|e| Error::Invalid(e.to_string()))?;
        pd.set("Contents", cid);
        pd.set("Resources", p.resources);
        if kept.is_empty() {
            pd.remove(b"Annots");
        } else {
            pd.set("Annots", Object::Array(kept));
        }
    }

    // Champs de formulaire supprimés : retirés de l'AcroForm.
    if !removed_widgets.is_empty()
        && let Ok(root) = doc.trailer.get(b"Root").and_then(|r| r.as_reference())
        && let Ok(cat) = doc.get_dictionary(root)
        && let Ok(af) = cat.get(b"AcroForm").and_then(|a| a.as_reference())
        && let Ok(afd) = doc.get_dictionary_mut(af)
        && let Ok(Object::Array(fields)) = afd.get(b"Fields").cloned()
    {
        let f: Vec<Object> = fields
            .into_iter()
            .filter(|f| f.as_reference().is_ok_and(|r| !removed_widgets.contains(&r)))
            .collect();
        afd.set("Fields", f);
    }

    // Réécriture complète : ni révision antérieure, ni objet orphelin.
    if let Some(eid) = state.as_ref().and_then(|s| s.encrypt_object_id()) {
        doc.objects.remove(&eid);
    }
    doc.trailer.remove(b"Encrypt");
    doc.trailer.remove(b"Prev");
    doc.trailer.remove(b"XRefStm");
    doc.prune_objects();
    if let Some(state) = state {
        doc.encryption_state = None;
        doc.encrypt(&state)
            .map_err(|e| Error::Engine(format!("rechiffrement : {e}")))?;
    }
    let mut out = vec![];
    doc.save_to(&mut out).map_err(|e| Error::Engine(format!("écriture : {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn ctx_areas() -> Vec<Area> {
        vec![Area {
            x0: 100.0,
            y0: 100.0,
            x1: 200.0,
            y1: 120.0,
            color: [0.0; 3],
        }]
    }

    #[test]
    fn matrices() {
        let t = mul(&[2.0, 0.0, 0.0, 2.0, 0.0, 0.0], &[1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
        assert_eq!(tp(&t, 1.0, 1.0), (12.0, 22.0));
        let i = invert(&t);
        assert_eq!(tp(&i, 12.0, 22.0), (1.0, 1.0));
    }

    #[test]
    fn image_pixels_are_blackened() {
        // Image grise 10×10 posée sur [100,200]×[100,200] ; zone sur sa bande basse.
        let s = Stream::new(
            dictionary! { "Width" => 10, "Height" => 10, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
            vec![200u8; 100],
        );
        let ctm = [100.0, 0.0, 0.0, 100.0, 100.0, 100.0];
        let out = redact_image(&s, &ctm, &ctx_areas()).unwrap();
        let data = out.decompressed_content().unwrap();
        // Lignes du bas (y utilisateur 100–120 → 2 dernières lignes de l'image) noircies.
        assert!(data[80..100].iter().all(|&v| v == 0));
        assert!(data[0..80].iter().all(|&v| v == 200));
    }
}
