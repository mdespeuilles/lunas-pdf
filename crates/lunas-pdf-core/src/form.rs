//! Formulaires AcroForm : lecture des champs, valeurs et apparences des widgets.
//!
//! Les valeurs modifiées sont écrites dans la révision incrémentale de l'écrivain : `/V` du
//! champ, `/AS` des cases et radios (leurs apparences existent déjà), et une apparence `/AP /N`
//! régénérée pour les champs texte et les listes, avec les polices standard en WinAnsi.

use std::collections::HashMap;
use std::io::Write;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::annot::FontFamily;
use crate::appearance::{FontSpec, encode_winansi, font_spec, text_width, wrap};
use crate::form_script::{self, Calculation, FieldFormat, RangeRule};
use crate::geom::Affine;
use crate::pdfwrite::{write_real, write_string};
use crate::types::Rect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ChoiceOption {
    /// Valeur exportée (`/V`).
    pub value: String,
    /// Libellé affiché.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FieldKind {
    Text {
        multiline: bool,
        password: bool,
        /// Cases régulières (`/MaxLen` caractères répartis sur la largeur).
        comb: bool,
        #[serde(rename = "maxLen")]
        max_len: Option<u32>,
    },
    Checkbox,
    Radio,
    Combo {
        options: Vec<ChoiceOption>,
        /// Saisie libre autorisée.
        editable: bool,
    },
    List {
        options: Vec<ChoiceOption>,
        multi: bool,
    },
    /// Champ de signature (phase 4) : affiché, non rempli ici.
    Signature,
    /// Bouton poussoir : sans valeur.
    Button,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub page: u32,
    /// Rectangle d'affichage (points, repère de la page affichée).
    pub rect: Rect,
    /// Nom de l'état « coché » (cases et radios).
    pub on_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    /// Nom complet du champ (`a.b.c`), unique.
    pub id: String,
    /// Info-bulle (`/TU`).
    pub label: Option<String>,
    pub kind: FieldKind,
    /// Texte : `[texte]` ; case ou radio : `[état coché]` ou `[]` ; liste : valeurs choisies.
    pub value: Vec<String>,
    /// Valeur par défaut (`/DV`), rétablie par « Effacer le formulaire ».
    pub default_value: Vec<String>,
    pub read_only: bool,
    pub required: bool,
    /// Taille du texte (0 = automatique).
    pub font_size: f32,
    /// Alignement (`/Q`) : 0 gauche, 1 centre, 2 droite.
    pub align: u8,
    pub font: FontFamily,
    pub widgets: Vec<Widget>,
    /// Mise en forme, plage autorisée et calcul (fonctions standard d'Acrobat).
    pub format: Option<FieldFormat>,
    pub range: Option<RangeRule>,
    pub calc: Option<Calculation>,
    /// Le champ a des scripts personnalisés, non exécutés.
    pub custom_script: bool,
}

/// Objets d'origine d'un champ (non exportés vers l'interface).
#[derive(Debug, Clone)]
pub struct FieldSource {
    pub field: ObjectId,
    /// Objets des widgets, dans l'ordre de `FormField::widgets`.
    pub widgets: Vec<ObjectId>,
    /// Opérateurs de couleur du texte extraits de `/DA` (ex. « 0 g »).
    pub color_ops: String,
}

// Indicateurs `/Ff`.
const FF_READ_ONLY: i64 = 1;
const FF_REQUIRED: i64 = 1 << 1;
const FF_MULTILINE: i64 = 1 << 12;
const FF_PASSWORD: i64 = 1 << 13;
const FF_RADIO: i64 = 1 << 15;
const FF_PUSHBUTTON: i64 = 1 << 16;
const FF_COMBO: i64 = 1 << 17;
const FF_EDIT: i64 = 1 << 18;
const FF_MULTISELECT: i64 = 1 << 21;
const FF_COMB: i64 = 1 << 24;

/// Attributs hérités le long de l'arbre des champs.
#[derive(Clone, Default)]
struct Inherit {
    name: String,
    ft: Option<String>,
    ff: i64,
    v: Option<Object>,
    dv: Option<Object>,
    da: Option<String>,
    q: i64,
    max_len: Option<i64>,
}

fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(o),
        _ => o,
    }
}

fn dict_of<'a>(doc: &'a Document, d: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
    d.get(key).ok().map(|o| resolve(doc, o)).and_then(|o| o.as_dict().ok())
}

fn text_of(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).into_owned()),
        _ => lopdf::decode_text_string(o).ok(),
    }
}

/// Pages du document, pour situer les widgets.
pub struct PageIndex {
    /// Objet et transformation d'affichage de chaque page.
    pub ids: Vec<(ObjectId, Affine)>,
    /// Widget → page, d'après les tableaux `/Annots`.
    pub widgets: HashMap<ObjectId, u32>,
}

/// Lit les champs du formulaire.
pub fn parse_fields(doc: &Document, pages: &PageIndex) -> Vec<(FormField, FieldSource)> {
    let Some(af) = doc.catalog().ok().and_then(|c| dict_of(doc, c, b"AcroForm")) else {
        return vec![];
    };
    let fonts = dict_of(doc, af, b"DR").and_then(|dr| dict_of(doc, dr, b"Font"));
    let root = Inherit {
        da: af.get(b"DA").ok().and_then(text_of),
        ..Default::default()
    };
    let mut out = vec![];
    if let Ok(Object::Array(arr)) = af.get(b"Fields").map(|o| resolve(doc, o)) {
        for f in arr {
            if let Object::Reference(id) = f {
                walk(doc, *id, &root, pages, fonts, &mut out, 0);
            }
        }
    }
    // Noms en double (fichier non conforme) : suffixe pour garder des identifiants uniques.
    let mut seen: HashMap<String, u32> = HashMap::new();
    for (f, _) in out.iter_mut() {
        let n = seen.entry(f.id.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            f.id = format!("{}#{}", f.id, n);
        }
    }
    out
}

fn walk(
    doc: &Document,
    id: ObjectId,
    parent: &Inherit,
    pages: &PageIndex,
    fonts: Option<&Dictionary>,
    out: &mut Vec<(FormField, FieldSource)>,
    depth: u32,
) {
    let Ok(d) = doc.get_dictionary(id) else { return };
    if depth > 32 {
        return;
    }
    let mut inh = parent.clone();
    if let Some(t) = d.get(b"T").ok().and_then(text_of) {
        inh.name = if parent.name.is_empty() {
            t
        } else {
            format!("{}.{t}", parent.name)
        };
    }
    if let Ok(ft) = d.get(b"FT").and_then(|o| o.as_name()) {
        inh.ft = Some(String::from_utf8_lossy(ft).into_owned());
    }
    if let Ok(ff) = d.get(b"Ff").map(|o| resolve(doc, o)).and_then(|o| o.as_i64()) {
        inh.ff = ff;
    }
    if let Ok(v) = d.get(b"V") {
        inh.v = Some(resolve(doc, v).clone());
    }
    if let Ok(v) = d.get(b"DV") {
        inh.dv = Some(resolve(doc, v).clone());
    }
    if let Some(da) = d.get(b"DA").ok().and_then(text_of) {
        inh.da = Some(da);
    }
    if let Ok(q) = d.get(b"Q").and_then(|o| o.as_i64()) {
        inh.q = q;
    }
    if let Ok(m) = d.get(b"MaxLen").map(|o| resolve(doc, o)).and_then(|o| o.as_i64()) {
        inh.max_len = Some(m);
    }

    let kids: Vec<ObjectId> = match d.get(b"Kids").map(|o| resolve(doc, o)) {
        Ok(Object::Array(a)) => a.iter().filter_map(|k| k.as_reference().ok()).collect(),
        _ => vec![],
    };
    let (fields, widgets): (Vec<ObjectId>, Vec<ObjectId>) = kids
        .into_iter()
        .partition(|k| doc.get_dictionary(*k).is_ok_and(|kd| kd.has(b"T")));
    for k in fields {
        walk(doc, k, &inh, pages, fonts, out, depth + 1);
    }
    let widgets = if widgets.is_empty() && !d.has(b"Kids") {
        vec![id]
    } else {
        widgets
    };
    if widgets.is_empty() || inh.name.is_empty() {
        return;
    }
    out.push(terminal(doc, id, d, &inh, &widgets, pages, fonts));
}

fn terminal(
    doc: &Document,
    id: ObjectId,
    d: &Dictionary,
    inh: &Inherit,
    widget_ids: &[ObjectId],
    pages: &PageIndex,
    fonts: Option<&Dictionary>,
) -> (FormField, FieldSource) {
    let ff = inh.ff;
    let options = parse_options(doc, d);
    let kind = match inh.ft.as_deref() {
        Some("Tx") => FieldKind::Text {
            multiline: ff & FF_MULTILINE != 0,
            password: ff & FF_PASSWORD != 0,
            comb: ff & FF_COMB != 0 && inh.max_len.is_some_and(|m| m > 0),
            max_len: inh.max_len.filter(|m| *m > 0).map(|m| m as u32),
        },
        Some("Btn") if ff & FF_PUSHBUTTON != 0 => FieldKind::Button,
        Some("Btn") if ff & FF_RADIO != 0 => FieldKind::Radio,
        Some("Btn") => FieldKind::Checkbox,
        Some("Ch") if ff & FF_COMBO != 0 => FieldKind::Combo {
            options,
            editable: ff & FF_EDIT != 0,
        },
        Some("Ch") => FieldKind::List {
            options,
            multi: ff & FF_MULTISELECT != 0,
        },
        Some("Sig") => FieldKind::Signature,
        _ => FieldKind::Button,
    };
    let mut widgets = vec![];
    let mut sources = vec![];
    for wid in widget_ids {
        let Ok(wd) = doc.get_dictionary(*wid) else { continue };
        // Page : par les `/Annots`, sinon par `/P`.
        let page = pages.widgets.get(wid).copied().or_else(|| {
            let p = wd.get(b"P").and_then(|p| p.as_reference()).ok()?;
            pages.ids.iter().position(|(id, _)| *id == p).map(|i| i as u32)
        });
        let Some((page, t)) = page.and_then(|p| pages.ids.get(p as usize).map(|(_, t)| (p, *t))) else {
            continue;
        };
        let r: Vec<f32> = match wd.get(b"Rect").map(|o| resolve(doc, o)) {
            Ok(Object::Array(a)) => a.iter().filter_map(|v| v.as_float().ok()).collect(),
            _ => continue,
        };
        if r.len() != 4 {
            continue;
        }
        let on_state = dict_of(doc, wd, b"AP").and_then(|ap| dict_of(doc, ap, b"N")).and_then(|n| {
            n.iter()
                .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                .find(|k| k != "Off")
        });
        widgets.push(Widget {
            page,
            rect: t.rect(r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3])),
            on_state,
        });
        sources.push(*wid);
    }
    let scripts = field_scripts(doc, d);
    let value = value_from(&kind, inh.v.as_ref());
    let default_value = value_from(&kind, inh.dv.as_ref());
    let da = inh.da.clone().unwrap_or_default();
    let (font_res, font_size, color_ops) = parse_da(&da);
    let font = font_family(doc, fonts, &font_res);
    let field = FormField {
        id: inh.name.clone(),
        label: d.get(b"TU").ok().and_then(text_of).filter(|s| !s.is_empty()),
        format: scripts.format,
        range: scripts.range,
        calc: scripts.calc,
        custom_script: scripts.custom,
        kind,
        value,
        default_value,
        read_only: ff & FF_READ_ONLY != 0,
        required: ff & FF_REQUIRED != 0,
        font_size,
        align: inh.q.clamp(0, 2) as u8,
        font,
        widgets,
    };
    (
        field,
        FieldSource {
            field: id,
            widgets: sources,
            color_ops,
        },
    )
}

/// Code JavaScript d'une action (`/JS` en chaîne ou en flux).
fn action_js(doc: &Document, aa: &Dictionary, key: &[u8]) -> Option<String> {
    let action = dict_of(doc, aa, key)?;
    if action.get(b"S").and_then(|s| s.as_name()).ok() != Some(b"JavaScript".as_slice()) {
        return None;
    }
    match resolve(doc, action.get(b"JS").ok()?) {
        Object::Stream(s) => {
            let bytes = s.decompressed_content().unwrap_or_else(|_| s.content.clone());
            Some(lopdf::decode_text_string(&Object::string_literal(bytes)).unwrap_or_default())
        }
        o => text_of(o),
    }
}

fn field_scripts(doc: &Document, d: &Dictionary) -> form_script::Scripts {
    let Some(aa) = dict_of(doc, d, b"AA") else {
        return Default::default();
    };
    let js = |k: &[u8]| action_js(doc, aa, k);
    form_script::analyze(
        js(b"K").as_deref(),
        js(b"F").as_deref(),
        js(b"V").as_deref(),
        js(b"C").as_deref(),
    )
}

/// Ordre des calculs (`/CO`) : objets des champs calculés.
pub fn calc_order(doc: &Document) -> Vec<ObjectId> {
    let Some(af) = doc.catalog().ok().and_then(|c| dict_of(doc, c, b"AcroForm")) else {
        return vec![];
    };
    match af.get(b"CO").map(|o| resolve(doc, o)) {
        Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
        _ => vec![],
    }
}

/// Champs dont dépend un calcul : noms exacts ou descendants (« total » → « total.a »).
pub fn calc_inputs<'a>(fields: &'a [FormField], calc: &Calculation) -> Vec<&'a FormField> {
    fields
        .iter()
        .filter(|f| calc.fields.iter().any(|n| f.id == *n || f.id.starts_with(&format!("{n}."))))
        .collect()
}

fn parse_options(doc: &Document, d: &Dictionary) -> Vec<ChoiceOption> {
    let Ok(Object::Array(arr)) = d.get(b"Opt").map(|o| resolve(doc, o)) else {
        return vec![];
    };
    arr.iter()
        .filter_map(|o| match resolve(doc, o) {
            Object::Array(pair) if pair.len() >= 2 => Some(ChoiceOption {
                value: text_of(resolve(doc, &pair[0]))?,
                label: text_of(resolve(doc, &pair[1]))?,
            }),
            o => text_of(o).map(|s| ChoiceOption {
                value: s.clone(),
                label: s,
            }),
        })
        .collect()
}

fn value_from(kind: &FieldKind, v: Option<&Object>) -> Vec<String> {
    let Some(v) = v else {
        return match kind {
            FieldKind::Text { .. } => vec![String::new()],
            _ => vec![],
        };
    };
    match kind {
        FieldKind::Text { .. } => vec![text_of(v).unwrap_or_default()],
        FieldKind::Checkbox | FieldKind::Radio => text_of(v).filter(|s| !s.is_empty() && s != "Off").into_iter().collect(),
        FieldKind::Combo { .. } | FieldKind::List { .. } => match v {
            Object::Array(a) => a.iter().filter_map(text_of).collect(),
            o => text_of(o).filter(|s| !s.is_empty()).into_iter().collect(),
        },
        _ => vec![],
    }
}

/// `/DA` → (ressource de police, taille, opérateurs de couleur).
fn parse_da(da: &str) -> (String, f32, String) {
    let toks: Vec<&str> = da.split_whitespace().collect();
    let mut font = String::from("Helv");
    let mut size: f32 = 0.0;
    let mut color = String::from("0 g");
    for (i, t) in toks.iter().enumerate() {
        let take = |n: usize| -> Option<String> {
            (i >= n && toks[i - n..i].iter().all(|v| v.parse::<f32>().is_ok())).then(|| toks[i - n..=i].join(" "))
        };
        match *t {
            "Tf" if i >= 2 => {
                font = toks[i - 2].trim_start_matches('/').to_string();
                size = toks[i - 1].parse().unwrap_or(0.0);
            }
            "g" => color = take(1).unwrap_or(color),
            "rg" => color = take(3).unwrap_or(color),
            "k" => color = take(4).unwrap_or(color),
            _ => {}
        }
    }
    (font, size.max(0.0), color)
}

fn font_family(doc: &Document, fonts: Option<&Dictionary>, res: &str) -> FontFamily {
    let base = fonts
        .and_then(|f| dict_of(doc, f, res.as_bytes()))
        .and_then(|f| f.get(b"BaseFont").ok())
        .and_then(|b| b.as_name().ok())
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_else(|| res.to_string());
    if base.contains("Times") || base == "TiRo" {
        FontFamily::Serif
    } else if base.contains("Courier") || base == "Cour" {
        FontFamily::Mono
    } else {
        FontFamily::Sans
    }
}

// --- Écriture ---------------------------------------------------------------------------------

/// Nouvelle valeur `/V` (None : clé retirée).
pub fn value_object(f: &FormField) -> Option<Object> {
    match &f.kind {
        FieldKind::Text { .. } => Some(lopdf::text_string(f.value.first().map(String::as_str).unwrap_or(""))),
        FieldKind::Checkbox | FieldKind::Radio => Some(Object::Name(
            f.value.first().map(String::as_str).unwrap_or("Off").as_bytes().to_vec(),
        )),
        FieldKind::Combo { .. } | FieldKind::List { .. } => match f.value.len() {
            0 => None,
            1 => Some(lopdf::text_string(&f.value[0])),
            _ => Some(Object::Array(f.value.iter().map(|v| lopdf::text_string(v)).collect())),
        },
        _ => None,
    }
}

/// Indices des options choisies (`/I`, listes).
pub fn selected_indices(f: &FormField) -> Option<Vec<Object>> {
    let FieldKind::List { options, .. } = &f.kind else {
        return None;
    };
    let mut idx: Vec<i64> = options
        .iter()
        .enumerate()
        .filter(|(_, o)| f.value.contains(&o.value))
        .map(|(i, _)| i as i64)
        .collect();
    idx.sort_unstable();
    (!idx.is_empty()).then(|| idx.into_iter().map(Object::Integer).collect())
}

/// Texte affiché d'un champ texte ou d'une liste déroulante.
/// Texte affiché et indicateur « en rouge » (nombre négatif selon le format).
fn display_text(f: &FormField) -> (String, bool) {
    let v = f.value.first().cloned().unwrap_or_default();
    if let (FieldKind::Text { password: false, .. }, Some(fmt)) = (&f.kind, &f.format) {
        return form_script::display(fmt, &v);
    }
    let text = match &f.kind {
        FieldKind::Text { password: true, .. } => "*".repeat(v.chars().count()),
        FieldKind::Combo { options, .. } => options.iter().find(|o| o.value == v).map(|o| o.label.clone()).unwrap_or(v),
        _ => v,
    };
    (text, false)
}

pub const LINE_HEIGHT: f32 = 1.15;
const AUTO_MAX: f32 = 12.0;
const AUTO_MIN: f32 = 4.0;

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
    fn color(&mut self, c: &[f32], stroke: bool) -> &mut Self {
        for v in c {
            self.n(*v);
        }
        let op = match (c.len(), stroke) {
            (1, false) => "g",
            (1, true) => "G",
            (4, false) => "k",
            (4, true) => "K",
            (_, false) => "rg",
            (_, true) => "RG",
        };
        self.op(op)
    }
    fn text(&mut self, x: f32, y: f32, bytes: &[u8]) -> &mut Self {
        self.n(x).n(y).op("Td");
        write_string(&mut self.0, bytes, StringFormat::Literal);
        self.op(" Tj")
    }
}

/// Taille automatique d'une ligne : remplit la hauteur (au plus 12 pt), réduite à la largeur.
fn auto_single(text: &[u8], f: &FontSpec, inner_w: f32, inner_h: f32) -> f32 {
    let mut s = ((inner_h - 2.0) / LINE_HEIGHT).clamp(AUTO_MIN, AUTO_MAX);
    let w = text_width(text, f, s);
    if w > inner_w && w > 0.0 {
        s = (s * inner_w / w).max(AUTO_MIN);
    }
    s
}

/// Apparence `/AP /N` d'un widget de champ texte ou de liste. `None` pour les autres types.
pub fn widget_appearance(doc: &Document, f: &FormField, src: &FieldSource, widget: &Dictionary) -> Option<Stream> {
    if !matches!(
        f.kind,
        FieldKind::Text { .. } | FieldKind::Combo { .. } | FieldKind::List { .. }
    ) {
        return None;
    }
    let r: Vec<f32> = match widget.get(b"Rect").map(|o| resolve(doc, o)) {
        Ok(Object::Array(a)) => a.iter().filter_map(|v| v.as_float().ok()).collect(),
        _ => return None,
    };
    if r.len() != 4 {
        return None;
    }
    let (rw, rh) = ((r[2] - r[0]).abs(), (r[3] - r[1]).abs());
    let mk = dict_of(doc, widget, b"MK");
    let rot = mk
        .and_then(|m| m.get(b"R").ok())
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    let (w, h) = if rot == 90 || rot == 270 { (rh, rw) } else { (rw, rh) };
    let nums = |d: Option<&Dictionary>, k: &[u8]| -> Vec<f32> {
        match d.and_then(|d| d.get(k).ok()).map(|o| resolve(doc, o)) {
            Some(Object::Array(a)) => a.iter().filter_map(|v| v.as_float().ok()).collect(),
            _ => vec![],
        }
    };
    let bg = nums(mk, b"BG");
    let bc = nums(mk, b"BC");
    let bs = dict_of(doc, widget, b"BS");
    let style = bs
        .and_then(|b| b.get(b"S").ok())
        .and_then(|s| s.as_name().ok())
        .map(|s| s.to_vec())
        .unwrap_or_else(|| b"S".to_vec());
    let bw = if bc.is_empty() {
        0.0
    } else {
        bs.and_then(|b| b.get(b"W").ok())
            .and_then(|v| v.as_float().ok())
            .unwrap_or(1.0)
    };
    let fs = font_spec(f.font);

    let mut o = Ops(Vec::with_capacity(256));
    if !bg.is_empty() {
        o.color(&bg, false).n(0.0).n(0.0).n(w).n(h).op("re f");
    }
    if bw > 0.0 {
        o.color(&bc, true).n(bw).op("w");
        if style == b"U" {
            o.n(0.0).n(bw / 2.0).op("m").n(w).n(bw / 2.0).op("l S");
        } else {
            if style == b"D" {
                o.op("[3] 0 d");
            }
            o.n(bw / 2.0).n(bw / 2.0).n(w - bw).n(h - bw).op("re S");
        }
    }
    let inset = if style == b"B" || style == b"I" { bw * 2.0 } else { bw };
    let pad = inset + 2.0;
    let inner_w = (w - 2.0 * pad).max(1.0);
    let inner_h = (h - 2.0 * inset).max(1.0);
    let asc = fs.ascent / 1000.0;
    let desc = -fs.descent / 1000.0;

    o.op("/Tx BMC").op("q");
    o.n(inset)
        .n(inset)
        .n((w - 2.0 * inset).max(0.0))
        .n((h - 2.0 * inset).max(0.0))
        .op("re W n");
    let (shown, red) = display_text(f);
    let font = |o: &mut Ops, size: f32| {
        let _ = write!(o.0, "/{} ", fs.res);
        o.n(size).op("Tf").op(if red { "1 0 0 rg" } else { &src.color_ops });
    };
    match &f.kind {
        FieldKind::List { options, .. } => {
            let size = if f.font_size > 0.0 { f.font_size } else { AUTO_MAX };
            let row = size * LINE_HEIGHT;
            let rows = (inner_h / row).floor().max(1.0) as usize;
            let first_sel = options.iter().position(|op| f.value.contains(&op.value)).unwrap_or(0);
            let top = first_sel.saturating_sub(rows - 1).min(options.len().saturating_sub(rows));
            for (i, op) in options.iter().enumerate().skip(top).take(rows + 1) {
                if f.value.contains(&op.value) {
                    let y = h - inset - (i - top + 1) as f32 * row;
                    o.n(0.6)
                        .n(0.75)
                        .n(0.85)
                        .op("rg")
                        .n(inset)
                        .n(y)
                        .n(w - 2.0 * inset)
                        .n(row)
                        .op("re f");
                }
            }
            o.op("BT");
            font(&mut o, size);
            let mut prev = (0.0, 0.0);
            for (i, op) in options.iter().enumerate().skip(top).take(rows + 1) {
                let base = h - inset - (i - top) as f32 * row - (row - (asc + desc) * size) / 2.0 - asc * size;
                o.text(pad - prev.0, base - prev.1, &encode_winansi(&op.label));
                prev = (pad, base);
            }
            o.op("ET");
        }
        FieldKind::Text { multiline: true, .. } => {
            let text = shown.clone();
            let fits = |s: f32| wrap(&text, &fs, s, inner_w).len() as f32 * s * LINE_HEIGHT <= inner_h - 2.0;
            let size = if f.font_size > 0.0 {
                f.font_size
            } else {
                let mut s = AUTO_MAX;
                while s > AUTO_MIN && !fits(s) {
                    s -= 0.5;
                }
                s
            };
            let lines = wrap(&text, &fs, size, inner_w);
            let leading = size * LINE_HEIGHT;
            o.op("BT");
            font(&mut o, size);
            let mut prev = (0.0, 0.0);
            for (i, line) in lines.iter().enumerate() {
                let tw = text_width(line, &fs, size);
                let x = aligned(f.align, pad, inner_w, tw);
                let y = h - inset - 2.0 - (leading - (asc + desc) * size) / 2.0 - asc * size - i as f32 * leading;
                o.text(x - prev.0, y - prev.1, line);
                prev = (x, y);
            }
            o.op("ET");
        }
        FieldKind::Text {
            comb: true,
            max_len: Some(n),
            ..
        } => {
            let chars: Vec<u8> = encode_winansi(&shown);
            let cell = w / *n as f32;
            let size = if f.font_size > 0.0 {
                f.font_size
            } else {
                ((inner_h - 2.0) / LINE_HEIGHT).clamp(AUTO_MIN, AUTO_MAX).min(cell)
            };
            let y = (h - (asc + desc) * size) / 2.0 + desc * size;
            o.op("BT");
            font(&mut o, size);
            let mut prev = 0.0;
            for (i, c) in chars.iter().take(*n as usize).enumerate() {
                let x = i as f32 * cell + (cell - text_width(&[*c], &fs, size)) / 2.0;
                o.text(x - prev, if i == 0 { y } else { 0.0 }, &[*c]);
                prev = x;
            }
            o.op("ET");
        }
        _ => {
            let text = encode_winansi(&shown);
            let size = if f.font_size > 0.0 {
                f.font_size
            } else {
                auto_single(&text, &fs, inner_w, inner_h)
            };
            let tw = text_width(&text, &fs, size);
            let x = aligned(f.align, pad, inner_w, tw);
            let y = (h - (asc + desc) * size) / 2.0 + desc * size;
            o.op("BT");
            font(&mut o, size);
            o.text(x, y, &text).op("ET");
        }
    }
    o.op("Q").op("EMC");

    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1,
        "BBox" => vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)],
        "Resources" => dictionary! {
            "Font" => dictionary! {
                fs.res => dictionary! {
                    "Type" => "Font", "Subtype" => "Type1", "BaseFont" => fs.base, "Encoding" => "WinAnsiEncoding",
                },
            },
        },
    };
    if rot != 0 {
        let (s, c) = (rot as f32).to_radians().sin_cos();
        let (s, c) = (s.round(), c.round());
        dict.set(
            "Matrix",
            vec![
                Object::Real(c),
                Object::Real(s),
                Object::Real(-s),
                Object::Real(c),
                0.into(),
                0.into(),
            ],
        );
    }
    let mut s = Stream::new(dict, o.0);
    let _ = s.compress();
    Some(s)
}

fn aligned(q: u8, pad: f32, inner_w: f32, tw: f32) -> f32 {
    match q {
        1 => pad + (inner_w - tw) / 2.0,
        2 => pad + inner_w - tw,
        _ => pad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_appearance() {
        assert_eq!(
            parse_da("/Helv 12 Tf .1 .1 .1 rg"),
            ("Helv".into(), 12.0, ".1 .1 .1 rg".into())
        );
        assert_eq!(parse_da("0 g /TiRo 0 Tf"), ("TiRo".into(), 0.0, "0 g".into()));
        assert_eq!(parse_da(""), ("Helv".into(), 0.0, "0 g".into()));
    }

    #[test]
    fn auto_size_shrinks_to_width() {
        let f = font_spec(FontFamily::Sans);
        assert_eq!(auto_single(b"abc", &f, 300.0, 18.0), 12.0);
        let long = b"Un texte bien trop long pour ce petit champ";
        let s = auto_single(long, &f, 60.0, 18.0);
        assert!(s < 12.0 && text_width(long, &f, s) <= 60.5 || s == AUTO_MIN);
    }
}
