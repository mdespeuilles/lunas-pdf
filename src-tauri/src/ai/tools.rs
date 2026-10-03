//! Outils exposés à l'agent (serveur MCP) : lire le document (texte positionné, rendu des
//! pages, champs, annotations) et le modifier (remplir les champs, ajouter ou retirer des
//! zones de texte et des coches).
//!
//! Les modifications passent par le même chemin que l'interface (`Engine::edit`) : chaque
//! appel d'outil est un pas d'annulation, et rien n'est écrit sur le disque avant que
//! l'utilisateur enregistre. Pages numérotées à partir de 1, coordonnées en points depuis
//! le coin supérieur gauche de la page affichée.

use std::sync::Arc;

use base64::Engine as _;
use lunas_pdf_core::annot::{Annot, AnnotBody, AnnotOp, CheckStyle, EditState, FontFamily};
use lunas_pdf_core::appearance::{font_spec, text_width, wrap};
use lunas_pdf_core::form::{FieldKind, FormField};
use lunas_pdf_core::{DocId, EditRequest, Engine, Rect, RenderRequest};
use serde_json::{Value, json};

use super::mcp;

/// Plus grand côté du rendu d'une page envoyé à l'agent, en pixels.
const VIEW_PX: f32 = 1400.0;
/// Quadrillage du rendu, en points (trait plus marqué tous les 100 pt).
const GRID_PT: f32 = 50.0;
/// Marge intérieure et interligne des zones de texte (identiques à l'interface, `afm.ts`).
const FREETEXT_PAD: f32 = 2.0;
const FREETEXT_LEADING: f32 = 1.2;
/// Couleur « noir » de la palette des annotations.
const INK: &str = "#1b1b20";

pub struct Toolbox {
    pub engine: Engine,
    pub doc: DocId,
    /// Taille des pages affichées (points), pour les vérifications et le rendu.
    pub pages: Vec<(f32, f32)>,
    /// Appelé après chaque modification : l'interface met l'onglet à jour.
    pub edited: Arc<dyn Fn(EditState) + Send + Sync>,
    /// Proposition d'informations à mémoriser (ajouts, informations remplacées) : carte
    /// dans le panneau, l'utilisateur accepte ou refuse.
    pub propose: Arc<dyn Fn(Vec<String>, Vec<String>) + Send + Sync>,
}

pub fn definitions() -> Vec<Value> {
    let page = json!({ "type": "integer", "minimum": 1, "description": "Page number, starting at 1." });
    vec![
        json!({
            "name": "get_page_text",
            "description": "Text of one page with the position of each text run: [x,y,w,h] in PDF points from the top-left corner of the page. Use it to find labels and where to write answers.",
            "inputSchema": { "type": "object", "properties": { "page": page }, "required": ["page"] },
        }),
        json!({
            "name": "view_page",
            "description": "Image of one page as currently displayed (with annotations and form values), overlaid with a light grid every 50 pt (darker every 100 pt) starting at the top-left corner. Use it to see the layout, blank lines, boxes and scanned pages, and to check your additions.",
            "inputSchema": { "type": "object", "properties": { "page": page }, "required": ["page"] },
        }),
        json!({
            "name": "list_form_fields",
            "description": "Interactive form fields (AcroForm) with their id, label, kind, current value, allowed values and position. Empty if the document has no interactive form.",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": "fill_form_fields",
            "description": "Set the values of interactive form fields, in one undoable step. Text: a string. Checkbox: true or false. Radio: one of its states (\"\" to clear). Choice: an option value or label (an array for multi-select lists).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "fields": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": { "id": { "type": "string" }, "value": {} },
                            "required": ["id", "value"],
                        },
                    },
                },
                "required": ["fields"],
            },
        }),
        json!({
            "name": "list_annotations",
            "description": "Annotations on the document (id, page, type, rectangle, text).",
            "inputSchema": { "type": "object", "properties": {} },
        }),
        json!({
            "name": "add_annotations",
            "description": "Add annotations in one undoable step. \"text\": a text box whose text starts at (x, y), the top-left corner of the first line; \\n starts a new line. \"check\": a tick mark centered on (x, y). Coordinates in PDF points from the top-left corner of the page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string", "enum": ["text", "check"] },
                                "page": page,
                                "x": { "type": "number" },
                                "y": { "type": "number" },
                                "text": { "type": "string", "description": "Text of a \"text\" item." },
                                "size": { "type": "number", "description": "Font size of a text item (default 10), or box size of a check (default 12)." },
                                "font": { "type": "string", "enum": ["sans", "serif", "mono"] },
                                "style": { "type": "string", "enum": ["check", "cross", "dot"], "description": "Mark of a check item (default check)." },
                            },
                            "required": ["type", "page", "x", "y"],
                        },
                    },
                },
                "required": ["items"],
            },
        }),
        json!({
            "name": "propose_memory",
            "description": "Offer to remember personal facts the user gave, to fill future documents (shown as a card the user accepts or declines). One short fact per item, prefixed with the person.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "add": { "type": "array", "items": { "type": "string" }, "description": "New facts, e.g. \"Parent – E-mail : jean@example.com\"." },
                    "replace": { "type": "array", "items": { "type": "string" }, "description": "Saved facts (verbatim) that the new ones update." },
                },
                "required": ["add"],
            },
        }),
        json!({
            "name": "remove_annotations",
            "description": "Remove annotations by id, in one undoable step.",
            "inputSchema": {
                "type": "object",
                "properties": { "ids": { "type": "array", "items": { "type": "string" } } },
                "required": ["ids"],
            },
        }),
    ]
}

impl Toolbox {
    pub async fn call(self: &Arc<Self>, name: &str, args: &Value) -> Result<Vec<Value>, String> {
        let (tb, name, args) = (self.clone(), name.to_owned(), args.clone());
        // Le moteur répond de façon bloquante (acteur PDFium sur son propre fil).
        tauri::async_runtime::spawn_blocking(move || tb.call_blocking(&name, &args))
            .await
            .map_err(|e| e.to_string())?
    }

    fn call_blocking(&self, name: &str, args: &Value) -> Result<Vec<Value>, String> {
        match name {
            "get_page_text" => self.page_text(page_arg(args)?).map(|t| vec![mcp::text(t)]),
            "view_page" => self.view_page(page_arg(args)?),
            "list_form_fields" => self.list_fields().map(|t| vec![mcp::text(t)]),
            "fill_form_fields" => self.fill_fields(args).map(|t| vec![mcp::text(t)]),
            "list_annotations" => self.list_annots().map(|t| vec![mcp::text(t)]),
            "add_annotations" => self.add_annots(args).map(|t| vec![mcp::text(t)]),
            "remove_annotations" => self.remove_annots(args).map(|t| vec![mcp::text(t)]),
            "propose_memory" => {
                let list = |k: &str| -> Vec<String> {
                    args.get(k)
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.trim().to_owned())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let add = list("add");
                if add.is_empty() {
                    return Err("nothing to propose".into());
                }
                (self.propose)(add, list("replace"));
                Ok(vec![mcp::text(
                    "Proposed to the user, who accepts or declines in the panel. Do not mention it again.",
                )])
            }
            _ => Err(format!("unknown tool: {name}")),
        }
    }

    fn size(&self, page: u32) -> Result<(f32, f32), String> {
        self.pages.get(page as usize).copied().ok_or_else(|| {
            format!(
                "page {} does not exist (the document has {} pages)",
                page + 1,
                self.pages.len()
            )
        })
    }

    fn page_text(&self, page: u32) -> Result<String, String> {
        let (w, h) = self.size(page)?;
        let text = self.engine.text(self.doc, page).map_err(|e| e.to_string())?;
        let mut out = format!("Page {} — {:.0} × {:.0} pt\n", page + 1, w, h);
        if text.runs.is_empty() {
            out.push_str("(no text on this page: probably a scanned image, use view_page)\n");
        }
        for (r, t) in segments(&text.runs) {
            out.push_str(&format!("[{:.1},{:.1},{:.1},{:.1}] {}\n", r.x, r.y, r.w, r.h, t.trim()));
        }
        Ok(out)
    }

    fn view_page(&self, page: u32) -> Result<Vec<Value>, String> {
        let (w, h) = self.size(page)?;
        let k = VIEW_PX / w.max(h);
        let (bw, bh) = ((w * k).round().max(1.0) as u32, (h * k).round().max(1.0) as u32);
        let bmp = self
            .engine
            .render(RenderRequest {
                doc: self.doc,
                page,
                width: bw,
                height: bh,
                tile: None,
                priority: 200,
                epoch: u32::MAX,
            })
            .map_err(|e| e.to_string())?;
        let mut img = image::RgbaImage::from_raw(bmp.width, bmp.height, bmp.rgba).ok_or("bitmap")?;
        draw_grid(&mut img, bmp.width as f32 / w);
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        let info = format!(
            "Page {} — {:.0} × {:.0} pt, image {} × {} px ({:.2} px per pt). Grid every 50 pt from the top-left corner (darker every 100 pt).",
            page + 1,
            w,
            h,
            bmp.width,
            bmp.height,
            bmp.width as f32 / w
        );
        Ok(vec![
            mcp::text(info),
            mcp::png(base64::engine::general_purpose::STANDARD.encode(png.into_inner())),
        ])
    }

    fn state(&self) -> Result<EditState, String> {
        self.engine.edit(self.doc, EditRequest::Load).map_err(|e| e.to_string())
    }

    fn apply(&self, ops: Vec<AnnotOp>) -> Result<EditState, String> {
        let st = self
            .engine
            .edit(self.doc, EditRequest::Apply(ops))
            .map_err(|e| e.to_string())?;
        (self.edited)(st.clone());
        Ok(st)
    }

    fn list_fields(&self) -> Result<String, String> {
        let fields: Vec<Value> = self
            .state()?
            .fields
            .iter()
            .filter(|f| !matches!(f.kind, FieldKind::Button))
            .map(field_json)
            .collect();
        if fields.is_empty() {
            return Ok("No interactive form fields: use add_annotations to fill this document.".into());
        }
        Ok(Value::Array(fields).to_string())
    }

    fn fill_fields(&self, args: &Value) -> Result<String, String> {
        let st = self.state()?;
        let items = args
            .get("fields")
            .and_then(Value::as_array)
            .ok_or("missing \"fields\" array")?;
        let mut ops = Vec::new();
        let mut errors = Vec::new();
        for it in items {
            let id = it.get("id").and_then(Value::as_str).unwrap_or_default();
            let Some(field) = st.fields.iter().find(|f| f.id == id) else {
                errors.push(format!("{id}: unknown field"));
                continue;
            };
            match field_value(field, it.get("value").unwrap_or(&Value::Null)) {
                Ok(value) => ops.push(AnnotOp::SetField {
                    id: id.to_owned(),
                    value,
                }),
                Err(e) => errors.push(format!("{id}: {e}")),
            }
        }
        let filled = ops.len();
        if filled > 0 {
            self.apply(ops)?;
        }
        let mut out = format!("{filled} field(s) filled.");
        if !errors.is_empty() {
            out.push_str(&format!(" Not filled: {}", errors.join("; ")));
        }
        Ok(out)
    }

    fn list_annots(&self) -> Result<String, String> {
        let annots: Vec<Value> = self
            .state()?
            .annots
            .iter()
            .map(|a| {
                let text = match &a.body {
                    AnnotBody::FreeText { text, .. } => Some(text.clone()),
                    _ => a.contents.clone().or_else(|| a.excerpt.clone()),
                };
                json!({ "id": a.id, "page": a.page + 1, "type": kind_name(&a.body), "rect": rect_json(&a.rect), "text": text })
            })
            .collect();
        Ok(if annots.is_empty() {
            "No annotations.".into()
        } else {
            Value::Array(annots).to_string()
        })
    }

    fn add_annots(&self, args: &Value) -> Result<String, String> {
        let items = args.get("items").and_then(Value::as_array).ok_or("missing \"items\" array")?;
        let mut annots = Vec::new();
        for (i, it) in items.iter().enumerate() {
            annots.push(self.annot(it).map_err(|e| format!("item {}: {e}", i + 1))?);
        }
        if annots.is_empty() {
            return Ok("Nothing to add.".into());
        }
        let ids: Vec<String> = annots.iter().map(|a| a.id.clone()).collect();
        self.apply(annots.into_iter().map(|annot| AnnotOp::Add { annot, index: None }).collect())?;
        Ok(format!("{} annotation(s) added, ids: {}", ids.len(), ids.join(", ")))
    }

    fn annot(&self, it: &Value) -> Result<Annot, String> {
        let page = page_arg(it)?;
        let (pw, ph) = self.size(page)?;
        let num = |k: &str| it.get(k).and_then(Value::as_f64).map(|v| v as f32);
        let (x, y) = (num("x").ok_or("missing x")?, num("y").ok_or("missing y")?);
        if !(0.0..=pw).contains(&x) || !(0.0..=ph).contains(&y) {
            return Err(format!("({x}, {y}) is outside the page ({pw:.0} × {ph:.0} pt)"));
        }
        let (rect, body, width) = match it.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = it
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim_end()
                    .to_owned();
                if text.is_empty() {
                    return Err("empty text".into());
                }
                let size = num("size").unwrap_or(10.0).clamp(4.0, 72.0);
                let font = match it.get("font").and_then(Value::as_str) {
                    Some("serif") => FontFamily::Serif,
                    Some("mono") => FontFamily::Mono,
                    _ => FontFamily::Sans,
                };
                (
                    text_box(&text, font, size, x, y, pw, ph),
                    AnnotBody::FreeText { text, font, size },
                    1.0,
                )
            }
            Some("check") => {
                let s = num("size").unwrap_or(12.0).clamp(6.0, 40.0);
                let style = match it.get("style").and_then(Value::as_str) {
                    Some("cross") => CheckStyle::Cross,
                    Some("dot") => CheckStyle::Dot,
                    _ => CheckStyle::Check,
                };
                let r = Rect {
                    x: (x - s / 2.0).clamp(0.0, pw - s),
                    y: (y - s / 2.0).clamp(0.0, ph - s),
                    w: s,
                    h: s,
                };
                (r, AnnotBody::Check { style }, 2.6)
            }
            _ => return Err("type must be \"text\" or \"check\"".into()),
        };
        Ok(Annot {
            id: new_id(),
            page,
            rect,
            color: INK.into(),
            opacity: 1.0,
            width,
            contents: None,
            author: None,
            modified: None,
            excerpt: None,
            body,
            hidden: false,
        })
    }

    fn remove_annots(&self, args: &Value) -> Result<String, String> {
        let ids: Vec<String> = args
            .get("ids")
            .and_then(Value::as_array)
            .ok_or("missing \"ids\" array")?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        let st = self.state()?;
        let (known, unknown): (Vec<String>, Vec<String>) = ids.into_iter().partition(|id| st.annots.iter().any(|a| &a.id == id));
        if !known.is_empty() {
            self.apply(known.iter().map(|id| AnnotOp::Remove { id: id.clone() }).collect())?;
        }
        let mut out = format!("{} annotation(s) removed.", known.len());
        if !unknown.is_empty() {
            out.push_str(&format!(" Unknown ids: {}", unknown.join(", ")));
        }
        Ok(out)
    }
}

/// Fragments de texte regroupés en segments : les mots d'une même ligne se suivent, un
/// grand espace (colonnes, « Nom : ____ Prénom : ____ ») commence un nouveau segment.
pub fn segments(runs: &[lunas_pdf_core::TextRun]) -> Vec<(Rect, String)> {
    let mut out: Vec<(Rect, String)> = Vec::new();
    let mut open = false;
    for r in runs {
        let joined = open
            && out.last().is_some_and(|(cur, _)| {
                let gap = r.rect.x - (cur.x + cur.w);
                let same_line = (r.rect.y + r.rect.h / 2.0 - (cur.y + cur.h / 2.0)).abs() < cur.h.max(r.rect.h) * 0.5;
                same_line && gap < cur.h.max(r.rect.h) * 1.2 && gap > -cur.h
            });
        if joined {
            let (cur, text) = out.last_mut().unwrap();
            let (x1, y1) = (
                (cur.x + cur.w).max(r.rect.x + r.rect.w),
                (cur.y + cur.h).max(r.rect.y + r.rect.h),
            );
            cur.x = cur.x.min(r.rect.x);
            cur.y = cur.y.min(r.rect.y);
            cur.w = x1 - cur.x;
            cur.h = y1 - cur.y;
            if !text.ends_with(' ') && !r.text.starts_with(' ') && r.rect.x - (cur.x + cur.w) > -0.5 {
                // Mots séparés par un espace non représenté dans les fragments.
                let prev_end = x1 - r.rect.w;
                if r.rect.x - prev_end > r.rect.h * 0.15 {
                    text.push(' ');
                }
            }
            text.push_str(&r.text);
        } else {
            out.push((r.rect, r.text.clone()));
        }
        open = !r.eol;
    }
    out
}

fn page_arg(args: &Value) -> Result<u32, String> {
    let p = args
        .get("page")
        .and_then(Value::as_u64)
        .ok_or("missing \"page\" (number, starting at 1)")?;
    if p == 0 {
        return Err("pages start at 1".into());
    }
    Ok(p as u32 - 1)
}

fn rect_json(r: &Rect) -> Value {
    let round = |v: f32| (v * 10.0).round() / 10.0;
    json!([round(r.x), round(r.y), round(r.w), round(r.h)])
}

fn kind_name(b: &AnnotBody) -> &str {
    match b {
        AnnotBody::FreeText { .. } => "text",
        AnnotBody::Check { .. } => "check",
        AnnotBody::Image { .. } => "image",
        AnnotBody::Note => "note",
        other => other.subtype(),
    }
}

/// Identifiant d'annotation (UUID v4, comme `crypto.randomUUID()` dans l'interface).
fn new_id() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Zone d'une annotation de texte dont la première ligne commence en (x, y) : même calcul
/// que l'interface (`fitTextBox`), marge intérieure compensée, bornée à la page.
pub fn text_box(text: &str, font: FontFamily, size: f32, x: f32, y: f32, pw: f32, ph: f32) -> Rect {
    let spec = font_spec(font);
    let x0 = (x - FREETEXT_PAD).max(0.0);
    let max_w = (pw - x0).max(size + 2.0 * FREETEXT_PAD);
    let longest = text
        .split('\n')
        .map(|l| text_width(&lunas_pdf_core::appearance::encode_winansi(l), &spec, size))
        .fold(size, f32::max);
    let w = (longest + 2.0 * FREETEXT_PAD + 1.0).min(max_w);
    let lines = wrap(text, &spec, size, w - 2.0 * FREETEXT_PAD).len().max(1);
    let h = lines as f32 * size * FREETEXT_LEADING + 2.0 * FREETEXT_PAD;
    Rect {
        x: x0.min((pw - w).max(0.0)),
        y: (y - FREETEXT_PAD).clamp(0.0, (ph - h).max(0.0)),
        w,
        h,
    }
}

fn field_json(f: &FormField) -> Value {
    let (kind, allowed): (&str, Option<Vec<String>>) = match &f.kind {
        FieldKind::Text { multiline, .. } => (if *multiline { "text (multiline)" } else { "text" }, None),
        FieldKind::Checkbox => ("checkbox", None),
        FieldKind::Radio => ("radio", Some(f.widgets.iter().filter_map(|w| w.on_state.clone()).collect())),
        FieldKind::Combo { options, editable } => (
            if *editable { "choice (free text allowed)" } else { "choice" },
            Some(options.iter().map(|o| o.label.clone()).collect()),
        ),
        FieldKind::List { options, multi } => (
            if *multi { "list (multi-select)" } else { "list" },
            Some(options.iter().map(|o| o.label.clone()).collect()),
        ),
        FieldKind::Signature => ("signature (cannot be filled)", None),
        FieldKind::Button => ("button", None),
    };
    let value: Value = match f.kind {
        FieldKind::Checkbox => json!(!f.value.is_empty()),
        _ if f.value.len() <= 1 => json!(f.value.first().cloned().unwrap_or_default()),
        _ => json!(f.value),
    };
    let w = f.widgets.first();
    json!({
        "id": f.id,
        "label": f.label,
        "kind": kind,
        "value": value,
        "allowed": allowed,
        "readOnly": f.read_only.then_some(true),
        "required": f.required.then_some(true),
        "page": w.map(|w| w.page + 1),
        "rect": w.map(|w| rect_json(&w.rect)),
    })
}

/// Valeur d'un champ (`FormField::value`) à partir de la valeur JSON donnée par l'agent.
fn field_value(f: &FormField, v: &Value) -> Result<Vec<String>, String> {
    if f.read_only {
        return Err("read-only field".into());
    }
    let as_text = |v: &Value| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    };
    match &f.kind {
        FieldKind::Text { .. } => Ok(vec![as_text(v).ok_or("expected a string")?]),
        FieldKind::Checkbox => {
            let on = match v {
                Value::Bool(b) => *b,
                Value::String(s) => {
                    matches!(s.to_lowercase().as_str(), "true" | "yes" | "oui" | "on" | "x" | "1")
                        || f.widgets.iter().any(|w| w.on_state.as_deref() == Some(s))
                }
                _ => return Err("expected true or false".into()),
            };
            let state = f
                .widgets
                .iter()
                .find_map(|w| w.on_state.clone())
                .unwrap_or_else(|| "Yes".into());
            Ok(if on { vec![state] } else { vec![] })
        }
        FieldKind::Radio => {
            let s = as_text(v).ok_or("expected one of the allowed states")?;
            if s.is_empty() {
                return Ok(vec![]);
            }
            let states: Vec<String> = f.widgets.iter().filter_map(|w| w.on_state.clone()).collect();
            states
                .iter()
                .find(|st| st.eq_ignore_ascii_case(&s))
                .map(|st| vec![st.clone()])
                .ok_or_else(|| format!("\"{s}\" is not one of {states:?}"))
        }
        FieldKind::Combo { options, editable } => {
            let s = as_text(v).ok_or("expected a string")?;
            match options
                .iter()
                .find(|o| o.value.eq_ignore_ascii_case(&s) || o.label.eq_ignore_ascii_case(&s))
            {
                Some(o) => Ok(vec![o.value.clone()]),
                None if *editable || s.is_empty() => Ok(if s.is_empty() { vec![] } else { vec![s] }),
                None => Err(format!("\"{s}\" is not one of the options")),
            }
        }
        FieldKind::List { options, multi } => {
            let wanted: Vec<String> = match v {
                Value::Array(a) => a.iter().filter_map(as_text).collect(),
                other => as_text(other).into_iter().filter(|s| !s.is_empty()).collect(),
            };
            if !multi && wanted.len() > 1 {
                return Err("only one option can be selected".into());
            }
            wanted
                .iter()
                .map(|s| {
                    options
                        .iter()
                        .find(|o| o.value.eq_ignore_ascii_case(s) || o.label.eq_ignore_ascii_case(s))
                        .map(|o| o.value.clone())
                        .ok_or_else(|| format!("\"{s}\" is not one of the options"))
                })
                .collect()
        }
        FieldKind::Signature => Err("signature fields cannot be filled".into()),
        FieldKind::Button => Err("buttons have no value".into()),
    }
}

/// Quadrillage léger tous les `GRID_PT` points, plus marqué tous les 100 pt.
fn draw_grid(img: &mut image::RgbaImage, px_per_pt: f32) {
    let (w, h) = img.dimensions();
    let blend = |p: &mut image::Rgba<u8>, alpha: f32| {
        for c in 0..3 {
            p.0[c] = (p.0[c] as f32 * (1.0 - alpha) + 90.0 * alpha) as u8;
        }
    };
    let mut k = 1;
    loop {
        let v = k as f32 * GRID_PT * px_per_pt;
        if v >= w.max(h) as f32 {
            break;
        }
        let alpha = if k % 2 == 0 { 0.35 } else { 0.15 };
        let i = v.round() as u32;
        if i < w {
            for y in 0..h {
                blend(img.get_pixel_mut(i, y), alpha);
            }
        }
        if i < h {
            for x in 0..w {
                blend(img.get_pixel_mut(x, i), alpha);
            }
        }
        k += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lunas_pdf_core::form::Widget;

    fn field(kind: FieldKind, states: &[&str]) -> FormField {
        FormField {
            id: "f".into(),
            label: None,
            kind,
            value: vec![],
            default_value: vec![],
            read_only: false,
            required: false,
            font_size: 0.0,
            align: 0,
            font: FontFamily::Sans,
            widgets: states
                .iter()
                .map(|s| Widget {
                    page: 0,
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 10.0,
                        h: 10.0,
                    },
                    on_state: Some(s.to_string()),
                })
                .collect(),
            format: None,
            range: None,
            calc: None,
            custom_script: false,
        }
    }

    #[test]
    fn valeurs_des_champs() {
        let text = field(
            FieldKind::Text {
                multiline: false,
                password: false,
                comb: false,
                max_len: None,
            },
            &[],
        );
        assert_eq!(field_value(&text, &json!("Dupont")).unwrap(), vec!["Dupont"]);
        assert_eq!(field_value(&text, &json!(75011)).unwrap(), vec!["75011"]);
        let check = field(FieldKind::Checkbox, &["Oui"]);
        assert_eq!(field_value(&check, &json!(true)).unwrap(), vec!["Oui"]);
        assert!(field_value(&check, &json!(false)).unwrap().is_empty());
        let radio = field(FieldKind::Radio, &["Homme", "Femme"]);
        assert_eq!(field_value(&radio, &json!("femme")).unwrap(), vec!["Femme"]);
        assert!(field_value(&radio, &json!("Autre")).is_err());
        let mut ro = text.clone();
        ro.read_only = true;
        assert!(field_value(&ro, &json!("x")).is_err());
    }

    #[test]
    fn zone_de_texte() {
        let r = text_box("Dupont", FontFamily::Sans, 10.0, 100.0, 200.0, 595.0, 842.0);
        assert_eq!((r.x, r.y), (98.0, 198.0));
        assert!((r.h - (10.0 * 1.2 + 4.0)).abs() < 0.01);
        assert!(r.w > 20.0 && r.w < 50.0, "{}", r.w);
        let two = text_box("Ligne 1\nLigne 2", FontFamily::Sans, 10.0, 100.0, 200.0, 595.0, 842.0);
        assert!((two.h - (2.0 * 12.0 + 4.0)).abs() < 0.01);
        // Près du bord droit : la zone se replie dans la page.
        let edge = text_box(
            "Un texte assez long pour dépasser",
            FontFamily::Sans,
            10.0,
            580.0,
            830.0,
            595.0,
            842.0,
        );
        assert!(edge.x + edge.w <= 595.0 + 0.01 && edge.y + edge.h <= 842.0 + 0.01);
    }

    #[test]
    fn segments_de_texte() {
        let run = |t: &str, x: f32, w: f32, eol: bool| lunas_pdf_core::TextRun {
            text: t.into(),
            rect: Rect { x, y: 100.0, w, h: 10.0 },
            eol,
        };
        let runs = [
            run("Nom ", 50.0, 22.0, false),
            run(":", 72.0, 3.0, false),
            run("Prénom :", 250.0, 40.0, true),
            run("Suite", 50.0, 25.0, true),
        ];
        let s = segments(&runs);
        assert_eq!(
            s.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
            ["Nom :", "Prénom :", "Suite"]
        );
        assert_eq!((s[0].0.x, s[0].0.w), (50.0, 25.0));
    }

    #[test]
    fn identifiants() {
        let (a, b) = (new_id(), new_id());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
    }

    #[test]
    fn outils_proposes() {
        let names: Vec<String> = definitions().iter().map(|d| d["name"].as_str().unwrap().to_owned()).collect();
        assert_eq!(
            names,
            [
                "get_page_text",
                "view_page",
                "list_form_fields",
                "fill_form_fields",
                "list_annotations",
                "add_annotations",
                "propose_memory",
                "remove_annotations"
            ]
        );
        // Jamais d'enregistrement, de caviardage ni de suppression de pages.
        assert!(!names.iter().any(|n| {
            ["save", "redact", "delete", "rotate", "move_page", "export"]
                .iter()
                .any(|w| n.contains(w))
        }));
    }
}
