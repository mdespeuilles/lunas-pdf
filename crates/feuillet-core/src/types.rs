//! Types échangés avec l'interface (exportés en TypeScript via specta).
//!
//! Convention géométrique : toutes les coordonnées sont en points PDF, dans le repère de la page
//! *affichée* (rotation et CropBox appliquées), origine en haut à gauche, y vers le bas.

use serde::{Deserialize, Serialize};
use specta::Type;

pub type DocId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn union(self, o: Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect {
            x,
            y,
            w: (self.x + self.w).max(o.x + o.w) - x,
            h: (self.y + self.h).max(o.y + o.h) - y,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PageGeom {
    /// Largeur affichée en points (rotation appliquée).
    pub width: f32,
    pub height: f32,
    /// Libellé de page (`/PageLabels`), ex. « iv ».
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FormKind {
    None,
    AcroForm,
    /// XFA dynamique : non pris en charge (message clair dans l'UI).
    Xfa,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    pub id: DocId,
    pub path: String,
    pub name: String,
    pub title: Option<String>,
    pub pages: Vec<PageGeom>,
    pub encrypted: bool,
    pub form: FormKind,
    pub signature_count: u32,
    pub can_copy: bool,
    pub can_print: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub title: String,
    /// Page cible (index 0) si la destination est locale.
    pub page: Option<u32>,
    pub children: Vec<OutlineItem>,
}

/// Sélection de texte pour les outils de marquage : une boîte par ligne.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TextSelection {
    pub rects: Vec<Rect>,
    pub text: String,
}

/// Image importée pour un tampon.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ImageInfo {
    pub key: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LinkTarget {
    Page { page: u32 },
    Uri { uri: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LinkInfo {
    pub rect: Rect,
    pub target: LinkTarget,
}

/// Fragment de texte pour la couche de sélection : un mot (ou une suite de glyphes contigus)
/// sur une même ligne.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub text: String,
    pub rect: Rect,
    /// Dernier fragment de sa ligne (l'UI insère un saut de ligne pour la copie).
    pub eol: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PageText {
    pub runs: Vec<TextRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub page: u32,
    /// Une boîte par ligne couverte par l'occurrence.
    pub rects: Vec<Rect>,
    pub before: String,
    pub matched: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SearchEvent {
    /// Résultats d'une page (envoyé même vide pour la progression).
    Page {
        search_id: u32,
        page: u32,
        hits: Vec<SearchHit>,
    },
    Done {
        search_id: u32,
        total: u32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Demande de rendu : la page entière est mise à l'échelle `width`×`height` px ;
/// `tile` restreint le rendu à une portion de cette image.
#[derive(Debug, Clone, Copy)]
pub struct RenderRequest {
    pub doc: DocId,
    pub page: u32,
    pub width: u32,
    pub height: u32,
    pub tile: Option<Tile>,
    /// Plus grand = plus urgent (page visible > miniature > préchargement).
    pub priority: u8,
    /// Les demandes d'une époque antérieure à celle du document sont abandonnées.
    pub epoch: u32,
}

/// Bitmap RGBA 8 bits, prête pour `ImageData`.
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
