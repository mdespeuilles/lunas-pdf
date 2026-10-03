//! Organisation des pages : ordre, rotation, duplication, page blanche, copie depuis un autre
//! document (avec ses champs de formulaire et ses signets), extraction.
//!
//! Les opérations modifient un `lopdf::Document` en mémoire et notent les objets touchés ;
//! l'écrivain en fait une révision incrémentale (`writer::serialize_increment`).

use std::collections::{BTreeSet, HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{Error, Result};
use crate::types::DocId;

/// Opération sur les pages (indices de pages dans l'ordre courant, à partir de 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PageOp {
    /// Déplace les pages avant la page d'indice `to` (`to` = nombre de pages : à la fin).
    Move {
        pages: Vec<u32>,
        to: u32,
    },
    /// Rotation de ±90° (multiple de 90).
    Rotate {
        pages: Vec<u32>,
        delta: i32,
    },
    Delete {
        pages: Vec<u32>,
    },
    /// Chaque copie suit sa page d'origine.
    Duplicate {
        pages: Vec<u32>,
    },
    /// Page blanche au format de la page précédente, insérée à l'indice `at`.
    InsertBlank {
        at: u32,
    },
    /// Copie de pages d'un autre document (ou du même) à l'indice `at`.
    Import {
        from: DocId,
        pages: Vec<u32>,
        at: u32,
    },
}

const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const A4: [f32; 4] = [0.0, 0.0, 595.28, 841.89];

/// Édition de l'arbre des pages d'un document chargé.
pub struct PageEdit<'a> {
    pub doc: &'a mut Document,
    /// Objets ajoutés ou modifiés (à écrire dans la révision).
    pub changed: BTreeSet<ObjectId>,
    root: ObjectId,
}

fn invalid(e: impl std::fmt::Display) -> Error {
    Error::Invalid(e.to_string())
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(o),
        _ => o,
    }
}

fn name_of(d: &Dictionary, key: &[u8]) -> Option<Vec<u8>> {
    d.get(key).and_then(|o| o.as_name()).ok().map(<[u8]>::to_vec)
}

/// Attributs hérités d'une page (Resources, MediaBox, CropBox, Rotate) absents de son
/// dictionnaire, lus sur ses ancêtres.
fn inherited(doc: &Document, page: &Dictionary) -> Vec<(Vec<u8>, Object)> {
    let mut out: Vec<(Vec<u8>, Object)> = vec![];
    let mut parent = page.get(b"Parent").and_then(|p| p.as_reference()).ok();
    for _ in 0..64 {
        let Some(pid) = parent else { break };
        let Ok(pd) = doc.get_dictionary(pid) else { break };
        for k in INHERITED {
            if !page.has(k)
                && !out.iter().any(|(n, _)| n == k)
                && let Ok(v) = pd.get(k)
            {
                out.push((k.to_vec(), v.clone()));
            }
        }
        parent = pd.get(b"Parent").and_then(|p| p.as_reference()).ok();
    }
    out
}

fn annots_of(doc: &Document, page: &Dictionary) -> Vec<Object> {
    match page.get(b"Annots").map(|a| deref(doc, a)) {
        Ok(Object::Array(a)) => a.clone(),
        _ => vec![],
    }
}

impl<'a> PageEdit<'a> {
    pub fn new(doc: &'a mut Document) -> Result<Self> {
        let root = doc
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Pages").ok())
            .and_then(|p| p.as_reference().ok())
            .ok_or_else(|| Error::Invalid("arbre des pages absent".into()))?;
        Ok(PageEdit {
            doc,
            changed: BTreeSet::new(),
            root,
        })
    }

    /// Pages dans l'ordre du document.
    pub fn order(&self) -> Vec<ObjectId> {
        self.doc.get_pages().into_values().collect()
    }

    fn alloc(&mut self, o: Object) -> ObjectId {
        let id = self.doc.add_object(o);
        self.changed.insert(id);
        id
    }

    fn dict_mut(&mut self, id: ObjectId) -> Result<&mut Dictionary> {
        self.changed.insert(id);
        self.doc.get_dictionary_mut(id).map_err(invalid)
    }

    /// Recopie dans la page les attributs hérités (avant de la rattacher ailleurs).
    fn materialize(&mut self, page: ObjectId) -> Result<()> {
        let d = self.doc.get_dictionary(page).map_err(invalid)?.clone();
        let inh = inherited(self.doc, &d);
        if !inh.is_empty() {
            let d = self.dict_mut(page)?;
            for (k, v) in inh {
                d.set(k, v);
            }
        }
        Ok(())
    }

    /// Remplace l'ordre des pages : arbre aplati sous la racine.
    pub fn set_order(&mut self, order: &[ObjectId]) -> Result<()> {
        let root = self.root;
        for &p in order {
            self.materialize(p)?;
            self.dict_mut(p)?.set("Parent", root);
        }
        let r = self.dict_mut(root)?;
        r.set("Kids", order.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>());
        r.set("Count", order.len() as i64);
        Ok(())
    }

    pub fn rotate(&mut self, page: ObjectId, delta: i32) -> Result<()> {
        self.materialize(page)?;
        let d = self.dict_mut(page)?;
        let cur = d.get(b"Rotate").and_then(|r| r.as_i64()).unwrap_or(0);
        d.set("Rotate", (cur + delta as i64).rem_euclid(360) / 90 * 90);
        Ok(())
    }

    /// Copie d'une page du même document : contenu partagé, annotations recopiées (sauf les
    /// champs de formulaire et les popups).
    pub fn duplicate(&mut self, page: ObjectId) -> Result<ObjectId> {
        self.materialize(page)?;
        let mut d = self.doc.get_dictionary(page).map_err(invalid)?.clone();
        let annots = annots_of(self.doc, &d);
        for k in [&b"Annots"[..], b"StructParents", b"B", b"Thumb"] {
            d.remove(k);
        }
        let id = self.alloc(Object::Dictionary(d));
        let mut arr = vec![];
        for a in annots {
            let Some(ad) = deref(self.doc, &a).as_dict().ok().cloned() else {
                continue;
            };
            if matches!(name_of(&ad, b"Subtype").as_deref(), Some(b"Widget" | b"Popup")) {
                continue;
            }
            let mut nd = ad;
            nd.set("P", id);
            for k in [&b"Popup"[..], b"NM", b"StructParent", b"IRT"] {
                nd.remove(k);
            }
            arr.push(Object::Reference(self.alloc(Object::Dictionary(nd))));
        }
        if !arr.is_empty() {
            self.doc.get_dictionary_mut(id).map_err(invalid)?.set("Annots", arr);
        }
        Ok(id)
    }

    /// Page blanche au format de `reference` (A4 sans référence).
    pub fn blank_like(&mut self, reference: Option<ObjectId>) -> Result<ObjectId> {
        let mut media = Object::Array(A4.iter().map(|v| Object::Real(*v)).collect());
        let mut crop = None;
        let mut rotate = 0;
        if let Some(r) = reference {
            let d = self.doc.get_dictionary(r).map_err(invalid)?.clone();
            let mut full = d.clone();
            for (k, v) in inherited(self.doc, &d) {
                full.set(k, v);
            }
            if let Ok(m) = full.get(b"MediaBox") {
                media = deref(self.doc, m).clone();
            }
            crop = full.get(b"CropBox").ok().map(|c| deref(self.doc, c).clone());
            rotate = full.get(b"Rotate").and_then(|r| r.as_i64()).unwrap_or(0);
        }
        let mut d =
            dictionary! { "Type" => "Page", "Parent" => self.root, "MediaBox" => media, "Resources" => Dictionary::new() };
        if let Some(c) = crop {
            d.set("CropBox", c);
        }
        if rotate != 0 {
            d.set("Rotate", rotate);
        }
        Ok(self.alloc(Object::Dictionary(d)))
    }

    /// Copie des pages d'un autre document : objets renumérotés, champs de formulaire ajoutés
    /// à l'AcroForm, signets qui mènent à ces pages ajoutés à la suite. Renvoie les pages
    /// créées, dans l'ordre de `pages`.
    pub fn import(&mut self, src: &Document, pages: &[ObjectId]) -> Result<Vec<ObjectId>> {
        let src_pages: HashSet<ObjectId> = src.get_pages().into_values().collect();
        let wanted: HashSet<ObjectId> = pages.iter().copied().collect();
        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        // Pages réservées d'abord : les liens entre pages copiées restent valides.
        let mut new_pages = vec![];
        for p in pages {
            let id = match map.get(p) {
                Some(id) => *id,
                None => {
                    let id = self.alloc(Object::Null);
                    map.insert(*p, id);
                    id
                }
            };
            new_pages.push(id);
        }
        let mut ctx = CopyCtx {
            src,
            src_pages: &src_pages,
            wanted: &wanted,
            map,
        };
        for p in pages.iter().collect::<BTreeSet<_>>() {
            let mut d = src.get_dictionary(*p).map_err(invalid)?.clone();
            for (k, v) in inherited(src, &d) {
                d.set(k, v);
            }
            for k in [&b"Parent"[..], b"StructParents", b"B", b"Thumb"] {
                d.remove(k);
            }
            let Object::Dictionary(mut nd) = self.copy(&mut ctx, &Object::Dictionary(d)) else {
                continue;
            };
            nd.set("Parent", self.root);
            let id = ctx.map[p];
            self.doc.objects.insert(id, Object::Dictionary(nd));
        }
        self.merge_fields(src, &new_pages)?;
        self.merge_outlines(src, &ctx.map, pages)?;
        Ok(new_pages)
    }

    fn copy(&mut self, ctx: &mut CopyCtx, obj: &Object) -> Object {
        match obj {
            Object::Reference(r) => {
                if let Some(n) = ctx.map.get(r) {
                    return Object::Reference(*n);
                }
                // Page non copiée (destination d'un lien) ou nœud de l'arbre des pages : rien.
                if ctx.src_pages.contains(r) && !ctx.wanted.contains(r) {
                    return Object::Null;
                }
                let Ok(target) = ctx.src.get_object(*r) else {
                    return Object::Null;
                };
                if let Ok(d) = target.as_dict()
                    && name_of(d, b"Type").as_deref() == Some(b"Pages")
                {
                    return Object::Null;
                }
                let id = self.alloc(Object::Null);
                ctx.map.insert(*r, id);
                let copied = self.copy(ctx, &target.clone());
                self.doc.objects.insert(id, copied);
                Object::Reference(id)
            }
            Object::Dictionary(d) => {
                let mut nd = Dictionary::new();
                for (k, v) in d.iter() {
                    nd.set(k.clone(), self.copy(ctx, v));
                }
                Object::Dictionary(nd)
            }
            Object::Array(a) => Object::Array(a.iter().map(|v| self.copy(ctx, v)).collect()),
            Object::Stream(s) => {
                let Object::Dictionary(dict) = self.copy(ctx, &Object::Dictionary(s.dict.clone())) else {
                    return Object::Null;
                };
                let mut ns = Stream::new(dict, s.content.clone());
                ns.allows_compression = false;
                Object::Stream(ns)
            }
            other => other.clone(),
        }
    }

    /// Ajoute aux `/Fields` les champs (racines) des widgets copiés ; noms en double renommés.
    fn merge_fields(&mut self, src: &Document, new_pages: &[ObjectId]) -> Result<()> {
        let mut roots: Vec<ObjectId> = vec![];
        for p in new_pages {
            let Ok(pd) = self.doc.get_dictionary(*p) else { continue };
            for a in annots_of(self.doc, &pd.clone()) {
                let Ok(id) = a.as_reference() else { continue };
                let Ok(ad) = self.doc.get_dictionary(id) else { continue };
                if name_of(ad, b"Subtype").as_deref() != Some(b"Widget") {
                    continue;
                }
                let mut top = id;
                for _ in 0..32 {
                    match self
                        .doc
                        .get_dictionary(top)
                        .ok()
                        .and_then(|d| d.get(b"Parent").ok())
                        .and_then(|p| p.as_reference().ok())
                    {
                        Some(parent) => top = parent,
                        None => break,
                    }
                }
                if !roots.contains(&top) {
                    roots.push(top);
                }
            }
        }
        if roots.is_empty() {
            return Ok(());
        }
        let (af_id, af) = self.acroform()?;
        let fields_obj = af.get(b"Fields").ok().cloned();
        let mut fields: Vec<Object> = match fields_obj.as_ref().map(|f| deref(self.doc, f)) {
            Some(Object::Array(a)) => a.clone(),
            _ => vec![],
        };
        let names: HashSet<Vec<u8>> = fields
            .iter()
            .filter_map(|f| deref(self.doc, f).as_dict().ok())
            .filter_map(|d| d.get(b"T").and_then(|t| t.as_str()).ok().map(<[u8]>::to_vec))
            .collect();
        for r in &roots {
            if let Ok(t) = self
                .doc
                .get_dictionary(*r)
                .and_then(|d| d.get(b"T"))
                .and_then(|t| t.as_str())
                .map(<[u8]>::to_vec)
                && names.contains(&t)
            {
                let base = String::from_utf8_lossy(&t).into_owned();
                let unique = (2..)
                    .map(|n| format!("{base}_{n}"))
                    .find(|n| !names.contains(n.as_bytes()))
                    .unwrap();
                self.dict_mut(*r)?.set("T", Object::string_literal(unique));
            }
            fields.push(Object::Reference(*r));
        }
        match fields_obj {
            Some(Object::Reference(fid)) => {
                self.changed.insert(fid);
                *self.doc.get_object_mut(fid).map_err(invalid)? = Object::Array(fields);
            }
            _ => {
                self.dict_mut(af_id)?.set("Fields", fields);
            }
        }
        // Polices et apparence par défaut du formulaire source, si absentes ici.
        let src_af = src
            .catalog()
            .ok()
            .and_then(|c| c.get(b"AcroForm").ok())
            .map(|a| deref(src, a))
            .and_then(|a| a.as_dict().ok())
            .cloned();
        if let Some(saf) = src_af {
            let src_fonts = saf
                .get(b"DR")
                .map(|d| deref(src, d))
                .ok()
                .and_then(|d| d.as_dict().ok())
                .and_then(|d| d.get(b"Font").ok())
                .map(|f| deref(src, f))
                .and_then(|f| f.as_dict().ok())
                .cloned();
            let af = self.doc.get_dictionary(af_id).map_err(invalid)?.clone();
            let mut dr = af
                .get(b"DR")
                .map(|d| deref(self.doc, d))
                .ok()
                .and_then(|d| d.as_dict().ok())
                .cloned()
                .unwrap_or_default();
            let mut fonts = dr
                .get(b"Font")
                .map(|f| deref(self.doc, f))
                .ok()
                .and_then(|f| f.as_dict().ok())
                .cloned()
                .unwrap_or_default();
            let mut touched = false;
            if let Some(sf) = src_fonts {
                let mut ctx = CopyCtx {
                    src,
                    src_pages: &HashSet::new(),
                    wanted: &HashSet::new(),
                    map: HashMap::new(),
                };
                for (k, v) in sf.iter() {
                    if !fonts.has(k) {
                        let c = self.copy(&mut ctx, v);
                        fonts.set(k.clone(), c);
                        touched = true;
                    }
                }
            }
            let afm = self.dict_mut(af_id)?;
            if touched {
                dr.set("Font", fonts);
                afm.set("DR", dr);
            }
            if !afm.has(b"DA")
                && let Ok(da) = saf.get(b"DA")
            {
                afm.set("DA", da.clone());
            }
        }
        Ok(())
    }

    /// AcroForm du catalogue (créé au besoin, rendu indirect pour être modifiable).
    fn acroform(&mut self) -> Result<(ObjectId, Dictionary)> {
        let cat_id = self
            .doc
            .trailer
            .get(b"Root")
            .and_then(|r| r.as_reference())
            .map_err(invalid)?;
        let cat = self.doc.get_dictionary(cat_id).map_err(invalid)?.clone();
        match cat.get(b"AcroForm") {
            Ok(Object::Reference(r)) => Ok((*r, self.doc.get_dictionary(*r).map_err(invalid)?.clone())),
            other => {
                let d = match other {
                    Ok(Object::Dictionary(d)) => d.clone(),
                    _ => dictionary! { "Fields" => Vec::<Object>::new() },
                };
                let id = self.alloc(Object::Dictionary(d.clone()));
                self.dict_mut(cat_id)?.set("AcroForm", id);
                Ok((id, d))
            }
        }
    }

    /// Signets du document source qui mènent aux pages copiées, ajoutés (à plat) à la fin.
    fn merge_outlines(&mut self, src: &Document, map: &HashMap<ObjectId, ObjectId>, pages: &[ObjectId]) -> Result<()> {
        let wanted: HashSet<ObjectId> = pages.iter().copied().collect();
        let mut items: Vec<(Object, Vec<Object>)> = vec![];
        let first = src
            .catalog()
            .ok()
            .and_then(|c| c.get(b"Outlines").ok())
            .map(|o| deref(src, o))
            .and_then(|o| o.as_dict().ok())
            .and_then(|o| o.get(b"First").ok())
            .and_then(|f| f.as_reference().ok());
        collect_outlines(src, first, &wanted, &mut items, 0);
        if items.is_empty() {
            return Ok(());
        }
        let cat_id = self
            .doc
            .trailer
            .get(b"Root")
            .and_then(|r| r.as_reference())
            .map_err(invalid)?;
        let cat = self.doc.get_dictionary(cat_id).map_err(invalid)?.clone();
        let outlines_id = match cat.get(b"Outlines") {
            Ok(Object::Reference(r)) => *r,
            _ => {
                let id = self.alloc(Object::Dictionary(dictionary! { "Type" => "Outlines", "Count" => 0 }));
                self.dict_mut(cat_id)?.set("Outlines", id);
                id
            }
        };
        let outlines = self.doc.get_dictionary(outlines_id).map_err(invalid)?.clone();
        let mut prev = outlines.get(b"Last").and_then(|l| l.as_reference()).ok();
        let mut first_new = None;
        for (title, dest) in items {
            let mut dest = dest;
            if let Some(Object::Reference(p)) = dest.first() {
                match map.get(p) {
                    Some(n) => dest[0] = Object::Reference(*n),
                    None => continue,
                }
            }
            let mut d = dictionary! { "Title" => title, "Parent" => outlines_id, "Dest" => dest };
            if let Some(p) = prev {
                d.set("Prev", p);
            }
            let id = self.alloc(Object::Dictionary(d));
            if let Some(p) = prev {
                self.dict_mut(p)?.set("Next", id);
            }
            first_new.get_or_insert(id);
            prev = Some(id);
        }
        let Some(first_new) = first_new else { return Ok(()) };
        let added = {
            let mut n = 0;
            let mut cur = Some(first_new);
            while let Some(c) = cur {
                n += 1;
                cur = self
                    .doc
                    .get_dictionary(c)
                    .ok()
                    .and_then(|d| d.get(b"Next").ok())
                    .and_then(|x| x.as_reference().ok());
            }
            n
        };
        let o = self.dict_mut(outlines_id)?;
        if !o.has(b"First") {
            o.set("First", first_new);
        }
        o.set("Last", prev.unwrap());
        let count = o.get(b"Count").and_then(|c| c.as_i64()).unwrap_or(0);
        o.set("Count", count.abs() + added);
        Ok(())
    }
}

struct CopyCtx<'s> {
    src: &'s Document,
    src_pages: &'s HashSet<ObjectId>,
    wanted: &'s HashSet<ObjectId>,
    map: HashMap<ObjectId, ObjectId>,
}

/// Signets (titre, destination explicite) dont la page est dans `wanted`.
fn collect_outlines(
    src: &Document,
    mut cur: Option<ObjectId>,
    wanted: &HashSet<ObjectId>,
    out: &mut Vec<(Object, Vec<Object>)>,
    depth: u32,
) {
    let mut guard = 0;
    while let Some(id) = cur {
        guard += 1;
        if guard > 10_000 || depth > 32 {
            return;
        }
        let Ok(d) = src.get_dictionary(id) else { return };
        let dest = d
            .get(b"Dest")
            .ok()
            .or_else(|| {
                d.get(b"A")
                    .ok()
                    .map(|a| deref(src, a))
                    .and_then(|a| a.as_dict().ok())
                    .and_then(|a| a.get(b"D").ok())
            })
            .map(|x| deref(src, x));
        if let (Some(Object::Array(arr)), Ok(title)) = (dest, d.get(b"Title"))
            && arr
                .first()
                .and_then(|p| p.as_reference().ok())
                .is_some_and(|p| wanted.contains(&p))
        {
            out.push((title.clone(), arr.clone()));
        }
        collect_outlines(
            src,
            d.get(b"First").and_then(|f| f.as_reference()).ok(),
            wanted,
            out,
            depth + 1,
        );
        cur = d.get(b"Next").and_then(|n| n.as_reference()).ok();
    }
}

/// Nouveau PDF contenant les pages choisies (et leurs champs, leurs signets).
pub fn extract(src: &Document, pages: &[ObjectId]) -> Result<Vec<u8>> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 }),
    );
    doc.trailer.set("Root", catalog_id);
    {
        let mut pe = PageEdit::new(&mut doc)?;
        let new = pe.import(src, pages)?;
        pe.set_order(&new)?;
    }
    let mut out = vec![];
    doc.save_to(&mut out).map_err(invalid)?;
    Ok(out)
}

/// Nouvel ordre après `op` (hors copie depuis un autre document) ; `f` crée les pages.
pub fn reorder(order: &[ObjectId], pages: &[u32], to: u32) -> Vec<ObjectId> {
    let sel: BTreeSet<usize> = pages.iter().map(|p| *p as usize).filter(|p| *p < order.len()).collect();
    let moving: Vec<ObjectId> = sel.iter().map(|i| order[*i]).collect();
    // Point d'insertion : première page non déplacée à partir de `to`.
    let anchor = (to as usize..order.len()).find(|i| !sel.contains(i)).map(|i| order[i]);
    let mut rest: Vec<ObjectId> = order
        .iter()
        .enumerate()
        .filter(|(i, _)| !sel.contains(i))
        .map(|(_, p)| *p)
        .collect();
    let at = anchor.and_then(|a| rest.iter().position(|p| *p == a)).unwrap_or(rest.len());
    rest.splice(at..at, moving);
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_moves_selection_before_target() {
        let o: Vec<ObjectId> = (1..=5).map(|n| (n, 0)).collect();
        let ids = |v: Vec<ObjectId>| v.into_iter().map(|x| x.0).collect::<Vec<_>>();
        assert_eq!(ids(reorder(&o, &[3], 1)), [1, 4, 2, 3, 5]);
        assert_eq!(ids(reorder(&o, &[0, 1], 4)), [3, 4, 1, 2, 5]);
        assert_eq!(ids(reorder(&o, &[0], 5)), [2, 3, 4, 5, 1]);
        // Cible dans la sélection : avant la première page non déplacée qui suit.
        assert_eq!(ids(reorder(&o, &[1, 2], 2)), [1, 2, 3, 4, 5]);
        assert_eq!(ids(reorder(&o, &[4, 0], 2)), [2, 1, 5, 3, 4]);
    }
}
