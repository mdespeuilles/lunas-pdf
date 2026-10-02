//! Modèle d'annotation partagé avec l'interface. Coordonnées d'affichage en points
//! (voir `types.rs`) ; la conversion vers l'espace utilisateur PDF est faite à l'écriture.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::form::FormField;
use crate::types::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FontFamily {
    /// Helvetica
    Sans,
    /// Times-Roman
    Serif,
    /// Courier
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CheckStyle {
    Check,
    Cross,
    Dot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AnnotBody {
    Highlight {
        quads: Vec<Rect>,
    },
    Underline {
        quads: Vec<Rect>,
    },
    StrikeOut {
        quads: Vec<Rect>,
    },
    FreeText {
        text: String,
        font: FontFamily,
        size: f32,
    },
    /// Note (annotation /Text) : icône sur la page, texte dans `contents`.
    Note,
    Square,
    Circle,
    Line {
        from: Point,
        to: Point,
        arrow: bool,
    },
    Check {
        style: CheckStyle,
    },
    /// Tampon image ; `image` identifie l'image importée (ou l'apparence existante).
    Image {
        image: String,
    },
    /// Zone à caviarder : le contenu sous les zones est supprimé à l'enregistrement.
    Redact {
        quads: Vec<Rect>,
    },
    /// Annotation existante non éditable (Ink, Polygon, tampon externe…) : déplacer ou supprimer.
    Other {
        subtype: String,
    },
}

impl AnnotBody {
    pub fn subtype(&self) -> &str {
        match self {
            AnnotBody::Highlight { .. } => "Highlight",
            AnnotBody::Underline { .. } => "Underline",
            AnnotBody::StrikeOut { .. } => "StrikeOut",
            AnnotBody::FreeText { .. } => "FreeText",
            AnnotBody::Note => "Text",
            AnnotBody::Square => "Square",
            AnnotBody::Circle => "Circle",
            AnnotBody::Line { .. } => "Line",
            AnnotBody::Check { .. } | AnnotBody::Image { .. } => "Stamp",
            AnnotBody::Redact { .. } => "Redact",
            AnnotBody::Other { subtype } => subtype,
        }
    }

    pub fn quads(&self) -> Option<&[Rect]> {
        match self {
            AnnotBody::Highlight { quads }
            | AnnotBody::Underline { quads }
            | AnnotBody::StrikeOut { quads }
            | AnnotBody::Redact { quads } => Some(quads),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Annot {
    /// Identifiant stable (`/NM` si présent, sinon généré).
    pub id: String,
    pub page: u32,
    pub rect: Rect,
    /// Couleur « #rrggbb ».
    pub color: String,
    /// Opacité 0–1 (`/CA`).
    pub opacity: f32,
    /// Épaisseur du trait en points.
    pub width: f32,
    /// Commentaire associé (`/Contents`).
    pub contents: Option<String>,
    pub author: Option<String>,
    /// Date de modification (`/M`, format PDF).
    pub modified: Option<String>,
    /// Texte recouvert (surlignage, soulignement…), calculé pour la liste.
    pub excerpt: Option<String>,
    pub body: AnnotBody,
    /// Masquée le temps d'une édition en place (jamais enregistrée).
    #[serde(default)]
    pub hidden: bool,
}

/// Opération élémentaire sur les annotations (pattern commande).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum AnnotOp {
    /// `index` : position d'insertion (utilisée pour annuler une suppression).
    Add {
        annot: Annot,
        index: Option<u32>,
    },
    Update {
        annot: Annot,
    },
    Remove {
        id: String,
    },
    /// Valeur d'un champ de formulaire (voir `FormField::value`).
    SetField {
        id: String,
        value: Vec<String>,
    },
}

/// État d'édition renvoyé à l'interface après chaque opération.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditState {
    pub annots: Vec<Annot>,
    /// Champs du formulaire AcroForm (vide sans formulaire).
    pub fields: Vec<FormField>,
    /// Pages dont le rendu a changé depuis l'état précédent.
    pub changed_pages: Vec<u32>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    /// Des zones de caviardage seront appliquées à l'enregistrement.
    pub pending_redactions: bool,
    /// Nombre d'annotations ajoutées, modifiées ou supprimées depuis l'enregistrement.
    pub unsaved_count: u32,
    /// Auteur des nouvelles annotations (« Vous » dans la liste).
    pub author: String,
    /// Nouvelle géométrie des pages quand leur nombre, leur ordre ou leur rotation a changé.
    pub pages: Option<Vec<crate::types::PageGeom>>,
}

/// Couleur « #rrggbb » → composantes 0–1.
pub fn parse_color(hex: &str) -> [f32; 3] {
    let h = hex.trim_start_matches('#');
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32 / 255.0;
    if h.len() >= 6 { [c(0), c(2), c(4)] } else { [0.0, 0.0, 0.0] }
}

pub fn format_color(rgb: [f32; 3]) -> String {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

/// Entrée de la pile annuler/rétablir : opérations inverses, ou état complet antérieur (pour
/// les opérations sur les pages, qui réécrivent le document).
pub enum Entry {
    Ops(Vec<AnnotOp>),
    Snapshot(Box<crate::writer::Snapshot>),
}

#[derive(Default)]
pub struct History {
    pub(crate) undo: Vec<Entry>,
    pub(crate) redo: Vec<Entry>,
}

impl History {
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Applique des opérations et mémorise leur inverse. Renvoie les pages touchées.
    pub fn apply(&mut self, annots: &mut Vec<Annot>, fields: &mut [FormField], ops: Vec<AnnotOp>) -> Vec<u32> {
        let (inverse, pages) = apply_ops(annots, fields, ops);
        if !inverse.is_empty() {
            self.undo.push(Entry::Ops(inverse));
            self.redo.clear();
        }
        pages
    }

    pub fn push_snapshot(&mut self, s: crate::writer::Snapshot) {
        self.undo.push(Entry::Snapshot(Box::new(s)));
        self.redo.clear();
    }

    /// Annule une entrée d'opérations ; une entrée d'état est rendue telle quelle (l'éditeur
    /// la restaure et pousse l'état courant dans l'autre pile).
    pub fn undo(&mut self, annots: &mut Vec<Annot>, fields: &mut [FormField]) -> Result<Vec<u32>, Box<crate::writer::Snapshot>> {
        match self.undo.pop() {
            None => Ok(vec![]),
            Some(Entry::Ops(ops)) => {
                let (inverse, pages) = apply_ops(annots, fields, ops);
                self.redo.push(Entry::Ops(inverse));
                Ok(pages)
            }
            Some(Entry::Snapshot(s)) => Err(s),
        }
    }

    pub fn redo(&mut self, annots: &mut Vec<Annot>, fields: &mut [FormField]) -> Result<Vec<u32>, Box<crate::writer::Snapshot>> {
        match self.redo.pop() {
            None => Ok(vec![]),
            Some(Entry::Ops(ops)) => {
                let (inverse, pages) = apply_ops(annots, fields, ops);
                self.undo.push(Entry::Ops(inverse));
                Ok(pages)
            }
            Some(Entry::Snapshot(s)) => Err(s),
        }
    }
}

/// Applique les opérations ; renvoie les opérations inverses (dans l'ordre d'annulation)
/// et les pages touchées. Les opérations sur un identifiant inconnu sont ignorées.
fn apply_ops(annots: &mut Vec<Annot>, fields: &mut [FormField], ops: Vec<AnnotOp>) -> (Vec<AnnotOp>, Vec<u32>) {
    let mut inverse = Vec::with_capacity(ops.len());
    let mut pages = vec![];
    for op in ops {
        match op {
            AnnotOp::Add { annot, index } => {
                if annots.iter().any(|a| a.id == annot.id) {
                    continue;
                }
                pages.push(annot.page);
                inverse.push(AnnotOp::Remove { id: annot.id.clone() });
                let at = index.map(|i| (i as usize).min(annots.len())).unwrap_or(annots.len());
                annots.insert(at, annot);
            }
            AnnotOp::Update { annot } => {
                let Some(i) = annots.iter().position(|a| a.id == annot.id) else {
                    continue;
                };
                pages.push(annot.page);
                pages.push(annots[i].page);
                let mut old = std::mem::replace(&mut annots[i], annot);
                // Le masquage d'édition en place ne fait jamais partie de l'historique.
                old.hidden = false;
                inverse.push(AnnotOp::Update { annot: old });
            }
            AnnotOp::Remove { id } => {
                let Some(i) = annots.iter().position(|a| a.id == id) else {
                    continue;
                };
                let mut old = annots.remove(i);
                old.hidden = false;
                pages.push(old.page);
                // Réinsertion à la même place pour conserver l'ordre d'affichage.
                inverse.push(AnnotOp::Add {
                    annot: old,
                    index: Some(i as u32),
                });
            }
            AnnotOp::SetField { id, value } => {
                let Some(f) = fields.iter_mut().find(|f| f.id == id) else {
                    continue;
                };
                if f.value == value {
                    continue;
                }
                pages.extend(f.widgets.iter().map(|w| w.page));
                let old = std::mem::replace(&mut f.value, value);
                inverse.push(AnnotOp::SetField { id, value: old });
            }
        }
    }
    inverse.reverse();
    pages.sort_unstable();
    pages.dedup();
    (inverse, pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(id: &str, page: u32) -> Annot {
        Annot {
            id: id.into(),
            page,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            color: "#e03131".into(),
            opacity: 1.0,
            width: 1.0,
            contents: None,
            author: None,
            modified: None,
            excerpt: None,
            body: AnnotBody::Square,
            hidden: false,
        }
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut h = History::default();
        let mut list = vec![a("x", 0)];
        let mut moved = a("x", 0);
        moved.rect.x = 50.0;
        h.apply(
            &mut list,
            &mut [],
            vec![
                AnnotOp::Add {
                    annot: a("y", 2),
                    index: None,
                },
                AnnotOp::Update { annot: moved.clone() },
            ],
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].rect.x, 50.0);
        assert_eq!(h.undo(&mut list, &mut []).ok().unwrap(), vec![0, 2]);
        assert_eq!(list, vec![a("x", 0)]);
        h.redo(&mut list, &mut []).ok().unwrap();
        assert_eq!(list[0], moved);
        assert_eq!(list[1].id, "y");
        assert!(h.can_undo() && !h.can_redo());
    }

    #[test]
    fn hidden_state_never_restored_by_undo() {
        let mut h = History::default();
        let mut hidden = a("x", 0);
        hidden.hidden = true; // masquée pendant un glisser
        let mut list = vec![hidden];
        let mut moved = a("x", 0);
        moved.rect.x = 40.0;
        h.apply(&mut list, &mut [], vec![AnnotOp::Update { annot: moved }]);
        h.undo(&mut list, &mut []).ok().unwrap();
        assert!(!list[0].hidden && list[0].rect.x == 0.0);
        list[0].hidden = true;
        h.apply(&mut list, &mut [], vec![AnnotOp::Remove { id: "x".into() }]);
        h.undo(&mut list, &mut []).ok().unwrap();
        assert!(!list[0].hidden);
    }

    #[test]
    fn remove_restores_position() {
        let mut h = History::default();
        let mut list = vec![a("a", 0), a("b", 0), a("c", 0)];
        h.apply(&mut list, &mut [], vec![AnnotOp::Remove { id: "b".into() }]);
        assert_eq!(list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        h.undo(&mut list, &mut []).ok().unwrap();
        assert_eq!(list.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    }

    #[test]
    fn colors() {
        assert_eq!(format_color(parse_color("#e03131")), "#e03131");
        assert_eq!(parse_color("#ffffff"), [1.0, 1.0, 1.0]);
    }
}
