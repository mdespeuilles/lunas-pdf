//! Écrivain Feuillet : modèle d'annotations d'un document et production des révisions.
//!
//! Le fichier d'origine n'est jamais réécrit : chaque état du modèle est sérialisé sous la
//! forme d'**une** révision incrémentale ajoutée aux octets enregistrés. Seuls les objets
//! réellement modifiés y figurent : annotations ajoutées ou modifiées, leurs apparences, et le
//! tableau `/Annots` (ou le dictionnaire de page) des pages touchées.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary, xref::XrefType};

use crate::annot::{Annot, AnnotBody, AnnotOp, CheckStyle, EditState, FontFamily, History, Point, format_color, parse_color};
use crate::appearance::{self, NOTE_SIZE};
use crate::error::{Error, Result};
use crate::geom::Affine;
use crate::pdfwrite::{write_indirect, write_object};
use crate::types::Rect;

/// Entrée d'origine du tableau `/Annots` d'une page.
struct Entry {
    obj: Object,
    /// Identifiant dans le modèle ; `None` = conservée telle quelle (lien, champ, popup…).
    model: Option<String>,
    /// Pour une popup : annotation parente.
    parent: Option<ObjectId>,
}

struct PageData {
    id: ObjectId,
    to_display: Affine,
    /// Tableau `/Annots` indirect (sinon il est réécrit dans le dictionnaire de page).
    annots_ref: Option<ObjectId>,
    entries: Vec<Entry>,
}

/// Origine d'une annotation existante.
#[derive(Clone)]
struct Origin {
    id: Option<ObjectId>,
    dict: Dictionary,
}

/// Image importée (octets du fichier PNG ou JPEG).
#[derive(Clone)]
pub struct ImportedImage {
    pub bytes: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

pub struct Editor {
    base: Arc<Vec<u8>>,
    doc: Document,
    pages: Vec<PageData>,
    saved: Vec<Annot>,
    pub annots: Vec<Annot>,
    origin: HashMap<String, Origin>,
    history: History,
    images: HashMap<String, ImportedImage>,
    pub author: String,
}

impl Editor {
    pub fn open(base: Arc<Vec<u8>>, password: Option<&str>, author: String) -> Result<Editor> {
        let opts = lopdf::LoadOptions {
            password: password.map(str::to_owned),
            ..Default::default()
        };
        let doc = Document::load_mem_with_options(&base, opts).map_err(|e| Error::Invalid(e.to_string()))?;
        let mut ed = Editor {
            base,
            doc,
            pages: vec![],
            saved: vec![],
            annots: vec![],
            origin: HashMap::new(),
            history: History::default(),
            images: HashMap::new(),
            author,
        };
        ed.parse()?;
        ed.saved = ed.annots.clone();
        Ok(ed)
    }

    pub fn base(&self) -> &Arc<Vec<u8>> {
        &self.base
    }

    pub fn page_transform(&self, page: u32) -> Option<Affine> {
        self.pages.get(page as usize).map(|p| p.to_display)
    }

    pub fn state(&self, changed_pages: Vec<u32>) -> EditState {
        EditState {
            annots: self.annots.clone(),
            changed_pages,
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            dirty: self.is_dirty(),
            pending_redactions: self.annots.iter().any(|a| matches!(a.body, AnnotBody::Redact { .. })),
            unsaved_count: self.unsaved_count(),
            author: self.author.clone(),
        }
    }

    fn unsaved_count(&self) -> u32 {
        let same = |a: &Annot, b: &Annot| {
            let mut a = a.clone();
            a.hidden = false;
            a.excerpt = b.excerpt.clone();
            &a == b
        };
        let changed = self
            .annots
            .iter()
            .filter(|a| !self.saved.iter().any(|s| s.id == a.id && same(a, s)))
            .count();
        let removed = self
            .saved
            .iter()
            .filter(|s| !self.annots.iter().any(|a| a.id == s.id))
            .count();
        (changed + removed) as u32
    }

    pub fn is_dirty(&self) -> bool {
        self.annots.len() != self.saved.len()
            || self.annots.iter().zip(&self.saved).any(|(a, b)| {
                let mut a = a.clone();
                a.hidden = false;
                a.excerpt = b.excerpt.clone();
                &a != b
            })
    }

    pub fn add_image(&mut self, key: String, img: ImportedImage) {
        self.images.insert(key, img);
    }

    pub fn apply(&mut self, ops: Vec<AnnotOp>) -> Vec<u32> {
        let ops = ops.into_iter().map(|op| self.normalize_op(op)).collect();
        self.history.apply(&mut self.annots, ops)
    }
    pub fn undo(&mut self) -> Vec<u32> {
        self.history.undo(&mut self.annots)
    }
    pub fn redo(&mut self) -> Vec<u32> {
        self.history.redo(&mut self.annots)
    }

    /// Masque une annotation le temps d'une édition en place (hors historique).
    pub fn set_hidden(&mut self, id: &str, hidden: bool) -> Vec<u32> {
        match self.annots.iter_mut().find(|a| a.id == id) {
            Some(a) if a.hidden != hidden => {
                a.hidden = hidden;
                vec![a.page]
            }
            _ => vec![],
        }
    }

    /// Complète une opération venue de l'interface : auteur, date, rectangle des lignes.
    fn normalize_op(&self, op: AnnotOp) -> AnnotOp {
        let fix = |mut a: Annot| {
            a.modified = Some(pdf_date_now());
            if a.author.is_none() {
                a.author = Some(self.author.clone());
            }
            if let AnnotBody::Line { from, to, arrow } = &a.body {
                a.rect = appearance::line_rect(*from, *to, a.width, *arrow);
            }
            if let Some(q) = a.body.quads()
                && let Some(first) = q.first()
            {
                a.rect = q.iter().skip(1).fold(*first, |acc, r| acc.union(*r));
            }
            a
        };
        match op {
            AnnotOp::Add { annot, index } => AnnotOp::Add {
                annot: fix(annot),
                index,
            },
            AnnotOp::Update { annot } => AnnotOp::Update { annot: fix(annot) },
            r => r,
        }
    }

    // --- Lecture des annotations existantes ---------------------------------------------------

    fn parse(&mut self) -> Result<()> {
        let page_ids: Vec<ObjectId> = self.doc.get_pages().into_values().collect();
        let mut used_ids: HashMap<String, ()> = HashMap::new();
        for (pi, pid) in page_ids.iter().enumerate() {
            let to_display = page_transform(&self.doc, *pid);
            let page_dict = self
                .doc
                .get_dictionary(*pid)
                .map_err(|e| Error::Invalid(e.to_string()))?
                .clone();
            let (annots_ref, arr) = match page_dict.get(b"Annots") {
                Ok(Object::Reference(r)) => (
                    Some(*r),
                    self.doc
                        .get_object(*r)
                        .and_then(|o| o.as_array())
                        .cloned()
                        .unwrap_or_default(),
                ),
                Ok(Object::Array(a)) => (None, a.clone()),
                _ => (None, vec![]),
            };
            let mut entries = vec![];
            for (idx, obj) in arr.into_iter().enumerate() {
                let (oid, dict) = match &obj {
                    Object::Reference(r) => (Some(*r), self.doc.get_dictionary(*r).cloned().unwrap_or_default()),
                    Object::Dictionary(d) => (None, d.clone()),
                    _ => (None, Dictionary::new()),
                };
                let subtype = name(&dict, b"Subtype").unwrap_or_default();
                let flags = dict.get(b"F").and_then(|f| f.as_i64()).unwrap_or(0);
                let passthrough = matches!(subtype.as_str(), "Link" | "Widget" | "Popup" | "") || flags & 2 != 0;
                if passthrough {
                    let parent = dict.get(b"Parent").and_then(|p| p.as_reference()).ok();
                    entries.push(Entry {
                        obj,
                        model: None,
                        parent: if subtype == "Popup" { parent } else { None },
                    });
                    continue;
                }
                let mut id = string(&dict, b"NM").filter(|s| !s.is_empty()).unwrap_or_default();
                if id.is_empty() || used_ids.contains_key(&id) {
                    id = match oid {
                        Some((n, g)) => format!("obj{n}_{g}"),
                        None => format!("p{pi}i{idx}"),
                    };
                }
                used_ids.insert(id.clone(), ());
                let annot = parse_annot(&self.doc, &dict, &id, pi as u32, &to_display, &subtype);
                self.origin.insert(id.clone(), Origin { id: oid, dict });
                self.annots.push(annot);
                entries.push(Entry {
                    obj,
                    model: Some(id),
                    parent: None,
                });
            }
            self.pages.push(PageData {
                id: *pid,
                to_display,
                annots_ref,
                entries,
            });
        }
        Ok(())
    }

    // --- Écriture ------------------------------------------------------------------------------

    /// Octets du document dans l'état courant : base + une révision incrémentale.
    pub fn build(&self, for_save: bool) -> Result<Vec<u8>> {
        if !self.is_dirty() && !self.annots.iter().any(|a| a.hidden) {
            return Ok(self.base.to_vec());
        }
        let mut objects: Vec<(ObjectId, Object)> = vec![];
        let mut next = self.doc.max_id + 1;

        for (pi, page) in self.pages.iter().enumerate() {
            let pi = pi as u32;
            let current: Vec<&Annot> = self.annots.iter().filter(|a| a.page == pi).collect();
            let saved: Vec<&Annot> = self.saved.iter().filter(|a| a.page == pi).collect();
            if current == saved {
                continue;
            }
            let removed: Vec<ObjectId> = page
                .entries
                .iter()
                .filter_map(|e| e.model.as_ref().filter(|id| !current.iter().any(|a| &a.id == *id)))
                .filter_map(|id| self.origin.get(id).and_then(|o| o.id))
                .collect();
            let mut arr: Vec<Object> = vec![];
            for e in &page.entries {
                match &e.model {
                    None => {
                        if e.parent.is_some_and(|p| removed.contains(&p)) {
                            continue; // popup d'une annotation supprimée
                        }
                        arr.push(e.obj.clone());
                    }
                    Some(id) => {
                        let Some(a) = current.iter().find(|a| &a.id == id) else {
                            continue;
                        };
                        let unchanged = self.saved.iter().any(|s| s == *a);
                        if unchanged && !a.hidden {
                            arr.push(e.obj.clone());
                            continue;
                        }
                        let origin = self.origin.get(id);
                        let oid = origin.and_then(|o| o.id).unwrap_or_else(|| {
                            let id = (next, 0);
                            next += 1;
                            id
                        });
                        let dict = self.annot_dict(a, origin, page, &mut objects, &mut next, for_save)?;
                        objects.push((oid, Object::Dictionary(dict)));
                        arr.push(Object::Reference(oid));
                    }
                }
            }
            for a in current.iter().filter(|a| !self.origin.contains_key(&a.id)) {
                let oid = (next, 0);
                next += 1;
                let dict = self.annot_dict(a, None, page, &mut objects, &mut next, for_save)?;
                objects.push((oid, Object::Dictionary(dict)));
                arr.push(Object::Reference(oid));
            }
            match page.annots_ref {
                Some(r) => objects.push((r, Object::Array(arr))),
                None => {
                    let mut pd = self
                        .doc
                        .get_dictionary(page.id)
                        .map_err(|e| Error::Invalid(e.to_string()))?
                        .clone();
                    if arr.is_empty() {
                        pd.remove(b"Annots");
                    } else {
                        pd.set("Annots", Object::Array(arr));
                    }
                    objects.push((page.id, Object::Dictionary(pd)));
                }
            }
        }
        self.serialize(objects, next)
    }

    fn annot_dict(
        &self,
        a: &Annot,
        origin: Option<&Origin>,
        page: &PageData,
        objects: &mut Vec<(ObjectId, Object)>,
        next: &mut u32,
        for_save: bool,
    ) -> Result<Dictionary> {
        let to_user = page.to_display.invert();
        let mut alloc = |o: Object| -> ObjectId {
            let id = (*next, 0);
            *next += 1;
            objects.push((id, o));
            id
        };
        let user_rect = |r: &Rect| -> Vec<Object> {
            let (x0, y0, x1, y1) = to_user.bbox(r.x, r.y, r.x + r.w, r.y + r.h);
            vec![Object::Real(x0), Object::Real(y0), Object::Real(x1), Object::Real(y1)]
        };
        let mut d = origin.map(|o| o.dict.clone()).unwrap_or_else(|| {
            dictionary! {
                "Type" => "Annot",
                "Subtype" => Object::Name(a.body.subtype().as_bytes().to_vec()),
                "NM" => Object::string_literal(a.id.clone()),
                "P" => page.id,
            }
        });
        let saved = self.saved.iter().find(|s| s.id == a.id);
        let mut flags = d.get(b"F").and_then(|f| f.as_i64()).unwrap_or(0) | 4; // imprimable
        flags = if a.hidden && !for_save { flags | 2 } else { flags & !2 };
        d.set("F", flags);
        d.set("Rect", user_rect(&a.rect));

        // Annotations externes non éditables : seul le déplacement est pris en charge.
        if let AnnotBody::Other { .. } = a.body {
            if let Some(s) = saved {
                translate_geometry(&mut d, &to_user, s.rect, a.rect);
            }
            return Ok(d);
        }
        // Tampon image existant : on garde son apparence (mise à l'échelle par le lecteur).
        if let AnnotBody::Image { image } = &a.body
            && image == "ap"
        {
            return Ok(d);
        }

        let rgb = parse_color(&a.color);
        let color_arr = |c: [f32; 3]| Object::Array(c.iter().map(|v| Object::Real(*v)).collect());
        d.set("C", color_arr(rgb));
        d.set("CA", a.opacity);
        d.set("M", Object::string_literal(a.modified.clone().unwrap_or_else(pdf_date_now)));
        if let Some(t) = &a.author {
            d.set("T", lopdf::text_string(t));
        }
        match &a.contents {
            Some(c) if !c.is_empty() => d.set("Contents", lopdf::text_string(c)),
            _ => {
                d.remove(b"Contents");
            }
        }
        d.remove(b"AS");
        d.remove(b"Border");
        d.set("BS", dictionary! { "W" => a.width, "S" => "S" });

        if let Some(quads) = a.body.quads() {
            let mut qp = vec![];
            for q in quads {
                // Ordre Acrobat : haut-gauche, haut-droit, bas-gauche, bas-droit (repère affiché).
                for (x, y) in [(q.x, q.y), (q.x + q.w, q.y), (q.x, q.y + q.h), (q.x + q.w, q.y + q.h)] {
                    let (ux, uy) = to_user.apply(x, y);
                    qp.push(Object::Real(ux));
                    qp.push(Object::Real(uy));
                }
            }
            d.set("QuadPoints", qp);
        }
        match &a.body {
            AnnotBody::Line { from, to, arrow } => {
                let (x0, y0) = to_user.apply(from.x, from.y);
                let (x1, y1) = to_user.apply(to.x, to.y);
                d.set(
                    "L",
                    vec![Object::Real(x0), Object::Real(y0), Object::Real(x1), Object::Real(y1)],
                );
                d.set(
                    "LE",
                    vec![
                        Object::Name(b"None".to_vec()),
                        Object::Name(if *arrow { b"OpenArrow".to_vec() } else { b"None".to_vec() }),
                    ],
                );
            }
            AnnotBody::FreeText { text, font, size } => {
                let f = appearance::font_spec(*font);
                d.set("Contents", lopdf::text_string(text));
                d.set(
                    "DA",
                    Object::string_literal(format!(
                        "/{} {} Tf {} {} {} rg",
                        f.res,
                        fmt(*size),
                        fmt(rgb[0]),
                        fmt(rgb[1]),
                        fmt(rgb[2])
                    )),
                );
                d.remove(b"C"); // pas de bordure ni de fond
                d.set("BS", dictionary! { "W" => 0, "S" => "S" });
                d.remove(b"RC");
            }
            AnnotBody::Note => {
                d.set("Name", "Comment");
            }
            AnnotBody::Check { style } => {
                d.set(
                    "Name",
                    match style {
                        CheckStyle::Check => "FeuilletCheck",
                        CheckStyle::Cross => "FeuilletCross",
                        CheckStyle::Dot => "FeuilletDot",
                    },
                );
            }
            AnnotBody::Image { .. } => {
                d.set("Name", "FeuilletImage");
            }
            AnnotBody::Redact { .. } => {
                d.set("IC", color_arr([0.0, 0.0, 0.0]));
            }
            _ => {}
        }

        let image = match &a.body {
            AnnotBody::Image { image } => match self.images.get(image) {
                Some(img) => Some(image_xobject(img, &mut alloc)?),
                None => return Err(Error::Engine(format!("image inconnue : {image}"))),
            },
            _ => None,
        };
        let ap = appearance::build(a, &page.to_display, &mut alloc, image);
        let ap_id = alloc(Object::Stream(ap));
        d.set("AP", dictionary! { "N" => ap_id });
        Ok(d)
    }

    fn serialize(&self, mut objects: Vec<(ObjectId, Object)>, mut next: u32) -> Result<Vec<u8>> {
        if let Some(state) = &self.doc.encryption_state {
            for (id, obj) in objects.iter_mut() {
                lopdf::encryption::encrypt_object(state, *id, obj).map_err(|e| Error::Engine(format!("chiffrement : {e:?}")))?;
            }
        }
        objects.sort_by_key(|(id, _)| *id);
        let mut out = Vec::with_capacity(self.base.len() + 8192);
        out.extend_from_slice(&self.base);
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        let mut offsets: Vec<(u32, u16, usize)> = vec![];
        for (id, obj) in &objects {
            offsets.push((id.0, id.1, write_indirect(&mut out, *id, obj)));
        }

        let mut trailer = Dictionary::new();
        for key in [&b"Root"[..], b"Info", b"ID", b"Encrypt"] {
            if let Ok(v) = self.doc.trailer.get(key) {
                trailer.set(key.to_vec(), v.clone());
            }
        }
        // lopdf retire /Encrypt du trailer après déchiffrement : on le rétablit.
        if let Some(state) = &self.doc.encryption_state
            && !trailer.has(b"Encrypt")
            && let Some(eid) = state.encrypt_object_id()
        {
            trailer.set("Encrypt", eid);
        }
        trailer.set("Prev", self.doc.xref_start as i64);
        let xref_stream = matches!(self.doc.reference_table.cross_reference_type, XrefType::CrossReferenceStream);
        let xref_id = if xref_stream {
            let id = next;
            next += 1;
            Some(id)
        } else {
            None
        };
        let size = next.max(self.doc.reference_table.size).max(self.doc.max_id + 1);
        trailer.set("Size", size as i64);

        let start = out.len();
        if let Some(xid) = xref_id {
            offsets.push((xid, 0, start));
            offsets.sort_by_key(|o| o.0);
            let mut data = vec![];
            let mut index = vec![];
            for group in contiguous(&offsets) {
                index.push(Object::Integer(group[0].0 as i64));
                index.push(Object::Integer(group.len() as i64));
                for (_, generation, off) in group {
                    data.push(1u8);
                    data.extend_from_slice(&(*off as u32).to_be_bytes());
                    data.extend_from_slice(&generation.to_be_bytes());
                }
            }
            trailer.set("Type", "XRef");
            trailer.set("W", vec![1.into(), 4.into(), 2.into()]);
            trailer.set("Index", index);
            let mut s = Stream::new(trailer, data);
            let _ = s.compress();
            write_indirect(&mut out, (xid, 0), &Object::Stream(s));
        } else {
            out.extend_from_slice(b"xref\n");
            for group in contiguous(&offsets) {
                let _ = writeln!(out, "{} {}", group[0].0, group.len());
                for (_, generation, off) in group {
                    let _ = write!(out, "{off:010} {generation:05} n\r\n");
                }
            }
            out.extend_from_slice(b"trailer\n");
            write_object(&mut out, &Object::Dictionary(trailer));
            out.push(b'\n');
        }
        let _ = write!(out, "startxref\n{start}\n%%EOF\n");
        Ok(out)
    }

    /// Repart d'octets enregistrés (après « Enregistrer ») : le modèle devient l'état de référence.
    pub fn rebase(&mut self, bytes: Arc<Vec<u8>>, password: Option<&str>) -> Result<()> {
        let images = std::mem::take(&mut self.images);
        let mut fresh = Editor::open(bytes, password, self.author.clone())?;
        fresh.images = images;
        *self = fresh;
        Ok(())
    }
}

fn contiguous(offsets: &[(u32, u16, usize)]) -> Vec<&[(u32, u16, usize)]> {
    let mut groups = vec![];
    let mut start = 0;
    for i in 1..=offsets.len() {
        if i == offsets.len() || offsets[i].0 != offsets[i - 1].0 + 1 {
            groups.push(&offsets[start..i]);
            start = i;
        }
    }
    groups
}

fn fmt(v: f32) -> String {
    let mut out = vec![];
    crate::pdfwrite::write_real(&mut out, v);
    String::from_utf8(out).unwrap_or_default()
}

/// XObject image d'un fichier PNG ou JPEG (JPEG intégré tel quel, PNG recompressé + masque alpha).
fn image_xobject(img: &ImportedImage, alloc: &mut dyn FnMut(Object) -> ObjectId) -> Result<ObjectId> {
    let bytes = img.bytes.as_slice();
    if bytes.starts_with(&[0xff, 0xd8]) {
        let decoded = image::load_from_memory(bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        let cs = if decoded.color().channel_count() == 1 {
            "DeviceGray"
        } else {
            "DeviceRGB"
        };
        let s = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => img.width as i64, "Height" => img.height as i64,
                "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "DCTDecode",
            },
            bytes.to_vec(),
        );
        return Ok(alloc(Object::Stream(s)));
    }
    let decoded = image::load_from_memory(bytes)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_rgba8();
    let (w, h) = decoded.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for p in decoded.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p.0[3]);
    }
    let mut mask_id = None;
    if alpha.iter().any(|&a| a != 255) {
        let mut m = Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
            alpha,
        );
        let _ = m.compress();
        mask_id = Some(alloc(Object::Stream(m)));
    }
    let mut dict = dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 };
    if let Some(m) = mask_id {
        dict.set("SMask", m);
    }
    let mut s = Stream::new(dict, rgb);
    let _ = s.compress();
    Ok(alloc(Object::Stream(s)))
}

/// Translation des tableaux de géométrie d'une annotation externe déplacée.
fn translate_geometry(d: &mut Dictionary, to_user: &Affine, from: Rect, to: Rect) {
    let (ax, ay) = to_user.apply(from.x, from.y);
    let (bx, by) = to_user.apply(to.x, to.y);
    let (dx, dy) = (bx - ax, by - ay);
    let shift = |arr: &Vec<Object>| -> Vec<Object> {
        arr.iter()
            .enumerate()
            .map(|(i, v)| Object::Real(v.as_float().unwrap_or(0.0) + if i % 2 == 0 { dx } else { dy }))
            .collect()
    };
    for key in [&b"QuadPoints"[..], b"L", b"Vertices", b"CL"] {
        if let Ok(Object::Array(a)) = d.get(key) {
            let s = shift(a);
            d.set(key.to_vec(), s);
        }
    }
    if let Ok(Object::Array(paths)) = d.get(b"InkList") {
        let p: Vec<Object> = paths
            .iter()
            .map(|p| p.as_array().map(|a| Object::Array(shift(a))).unwrap_or(Object::Null))
            .collect();
        d.set("InkList", p);
    }
}

// --- Lecture --------------------------------------------------------------------------------

fn name(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .and_then(|o| o.as_name())
        .ok()
        .map(|n| String::from_utf8_lossy(n).into_owned())
}

fn string(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key).ok().and_then(|o| lopdf::decode_text_string(o).ok())
}

fn numbers(doc: &Document, d: &Dictionary, key: &[u8]) -> Vec<f32> {
    let obj = match d.get(key) {
        Ok(Object::Reference(r)) => doc.get_object(*r).ok(),
        Ok(o) => Some(o),
        Err(_) => None,
    };
    obj.and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|v| v.as_float().ok()).collect())
        .unwrap_or_default()
}

fn color(doc: &Document, d: &Dictionary, key: &[u8]) -> Option<[f32; 3]> {
    let c = numbers(doc, d, key);
    match c.len() {
        1 => Some([c[0]; 3]),
        3 => Some([c[0], c[1], c[2]]),
        4 => Some([
            (1.0 - c[0]) * (1.0 - c[3]),
            (1.0 - c[1]) * (1.0 - c[3]),
            (1.0 - c[2]) * (1.0 - c[3]),
        ]),
        _ => None,
    }
}

fn parse_annot(doc: &Document, d: &Dictionary, id: &str, page: u32, t: &Affine, subtype: &str) -> Annot {
    let r = numbers(doc, d, b"Rect");
    let rect = if r.len() == 4 {
        t.rect(r[0], r[1], r[2], r[3])
    } else {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        }
    };
    let quads = || -> Vec<Rect> {
        numbers(doc, d, b"QuadPoints")
            .as_chunks::<8>()
            .0
            .iter()
            .map(|q| {
                let xs = [q[0], q[2], q[4], q[6]];
                let ys = [q[1], q[3], q[5], q[7]];
                let f = |v: &[f32; 4], m: fn(f32, f32) -> f32| v.iter().copied().fold(v[0], m);
                t.rect(f(&xs, f32::min), f(&ys, f32::min), f(&xs, f32::max), f(&ys, f32::max))
            })
            .collect()
    };
    let width = d
        .get(b"BS")
        .ok()
        .and_then(|bs| match bs {
            Object::Dictionary(bs) => bs.get(b"W").ok().and_then(|w| w.as_float().ok()),
            Object::Reference(r) => doc
                .get_dictionary(*r)
                .ok()
                .and_then(|bs| bs.get(b"W").ok().and_then(|w| w.as_float().ok())),
            _ => None,
        })
        .or_else(|| numbers(doc, d, b"Border").get(2).copied())
        .unwrap_or(1.0);
    let mut rgb = color(doc, d, b"C");
    let stamp_name = name(d, b"Name").unwrap_or_default();
    let body = match subtype {
        "Highlight" => AnnotBody::Highlight {
            quads: or_rect(quads(), rect),
        },
        "Underline" => AnnotBody::Underline {
            quads: or_rect(quads(), rect),
        },
        "StrikeOut" => AnnotBody::StrikeOut {
            quads: or_rect(quads(), rect),
        },
        "Redact" => AnnotBody::Redact {
            quads: or_rect(quads(), rect),
        },
        "Square" => AnnotBody::Square,
        "Circle" => AnnotBody::Circle,
        "Text" => AnnotBody::Note,
        "Line" => {
            let l = numbers(doc, d, b"L");
            if l.len() == 4 {
                let (x0, y0) = t.apply(l[0], l[1]);
                let (x1, y1) = t.apply(l[2], l[3]);
                let le = d
                    .get(b"LE")
                    .and_then(|o| o.as_array())
                    .ok()
                    .map(|a| a.iter().filter_map(|n| n.as_name().ok()).any(|n| n.ends_with(b"Arrow")))
                    .unwrap_or(false);
                AnnotBody::Line {
                    from: Point { x: x0, y: y0 },
                    to: Point { x: x1, y: y1 },
                    arrow: le,
                }
            } else {
                AnnotBody::Other { subtype: subtype.into() }
            }
        }
        "FreeText" => {
            let da = string(d, b"DA").unwrap_or_default();
            let (font, size, da_color) = parse_da(&da);
            rgb = da_color.or(rgb);
            AnnotBody::FreeText {
                text: string(d, b"Contents").unwrap_or_default(),
                font,
                size,
            }
        }
        "Stamp" => match stamp_name.as_str() {
            "FeuilletCheck" => AnnotBody::Check {
                style: CheckStyle::Check,
            },
            "FeuilletCross" => AnnotBody::Check {
                style: CheckStyle::Cross,
            },
            "FeuilletDot" => AnnotBody::Check { style: CheckStyle::Dot },
            _ => AnnotBody::Image { image: "ap".into() },
        },
        other => AnnotBody::Other { subtype: other.into() },
    };
    let contents = if matches!(body, AnnotBody::FreeText { .. }) {
        None
    } else {
        string(d, b"Contents").filter(|s| !s.is_empty())
    };
    Annot {
        id: id.into(),
        page,
        rect,
        color: format_color(rgb.unwrap_or([1.0, 0.83, 0.23])),
        opacity: d.get(b"CA").and_then(|v| v.as_float()).unwrap_or(1.0),
        width,
        contents,
        author: string(d, b"T").filter(|s| !s.is_empty()),
        modified: string(d, b"M"),
        excerpt: None,
        body,
        hidden: false,
    }
}

fn or_rect(q: Vec<Rect>, r: Rect) -> Vec<Rect> {
    if q.is_empty() { vec![r] } else { q }
}

/// `/Helv 12 Tf 1 0 0 rg` → police, taille, couleur.
fn parse_da(da: &str) -> (FontFamily, f32, Option<[f32; 3]>) {
    let toks: Vec<&str> = da.split_whitespace().collect();
    let mut font = FontFamily::Sans;
    let mut size = 12.0;
    let mut color = None;
    for (i, t) in toks.iter().enumerate() {
        match *t {
            "Tf" if i >= 2 => {
                font = appearance::font_from_res(toks[i - 2].trim_start_matches('/'));
                size = toks[i - 1].parse().unwrap_or(12.0);
                if size <= 0.0 {
                    size = 12.0;
                }
            }
            "rg" if i >= 3 => {
                let c: Vec<f32> = toks[i - 3..i].iter().filter_map(|v| v.parse().ok()).collect();
                if c.len() == 3 {
                    color = Some([c[0], c[1], c[2]]);
                }
            }
            "g" if i >= 1 => color = toks[i - 1].parse().ok().map(|g: f32| [g; 3]),
            _ => {}
        }
    }
    (font, size, color)
}

/// Transformation espace utilisateur → affichage déduite de `/CropBox` et `/Rotate` (hérités).
pub fn page_transform(doc: &Document, page: ObjectId) -> Affine {
    let inherited = |key: &[u8]| -> Option<Object> {
        let mut cur = doc.get_dictionary(page).ok()?;
        for _ in 0..32 {
            if let Ok(v) = cur.get(key) {
                return Some(match v {
                    Object::Reference(r) => doc.get_object(*r).ok()?.clone(),
                    o => o.clone(),
                });
            }
            cur = doc.get_dictionary(cur.get(b"Parent").ok()?.as_reference().ok()?).ok()?;
        }
        None
    };
    let boxv = |o: Option<Object>| -> Option<[f32; 4]> {
        let a = o?
            .as_array()
            .ok()?
            .iter()
            .filter_map(|v| v.as_float().ok())
            .collect::<Vec<_>>();
        (a.len() == 4).then(|| [a[0].min(a[2]), a[1].min(a[3]), a[0].max(a[2]), a[1].max(a[3])])
    };
    let media = boxv(inherited(b"MediaBox")).unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let crop = boxv(inherited(b"CropBox"))
        .map(|c| [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])])
        .filter(|c| c[2] > c[0] && c[3] > c[1])
        .unwrap_or(media);
    let rot = inherited(b"Rotate")
        .and_then(|r| r.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    let [x0, y0, x1, y1] = crop;
    match rot {
        90 => Affine {
            a: 0.0,
            b: 1.0,
            c: 1.0,
            d: 0.0,
            e: -y0,
            f: -x0,
        },
        180 => Affine {
            a: -1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: x1,
            f: -y0,
        },
        270 => Affine {
            a: 0.0,
            b: -1.0,
            c: -1.0,
            d: 0.0,
            e: y1,
            f: x1,
        },
        _ => Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: -x0,
            f: y1,
        },
    }
}

/// Date PDF courante en UTC : « D:AAAAMMJJHHmmSSZ ».
pub fn pdf_date_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86400) as i64, secs % 86400);
    // Algorithme « civil_from_days » (H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("D:{y:04}{m:02}{d:02}{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Taille par défaut d'une note (planche 03).
pub fn note_rect(at: Point) -> Rect {
    Rect {
        x: at.x,
        y: at.y,
        w: NOTE_SIZE,
        h: NOTE_SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_format() {
        let d = pdf_date_now();
        assert!(d.starts_with("D:20") && d.ends_with('Z') && d.len() == 17, "{d}");
    }

    #[test]
    fn default_appearance_string() {
        let (f, s, c) = parse_da("/TiRo 14 Tf 1 0 0 rg");
        assert_eq!((f, s, c), (FontFamily::Serif, 14.0, Some([1.0, 0.0, 0.0])));
        assert_eq!(parse_da("0 g /Helv 0 Tf").1, 12.0);
    }
}
