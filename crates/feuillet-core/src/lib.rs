//! Moteur PDF de Feuillet. Phase 1 : lecture seule (rendu, texte, recherche, sommaire,
//! annotations existantes, documents chiffrés). Voir docs/ARCHITECTURE.md.

pub mod annot;
pub mod appearance;
pub mod engine;
pub mod error;
pub mod fonts;
pub mod fsutil;
pub mod geom;
pub mod pdfwrite;
pub mod redact;
pub mod text;
pub mod types;
pub mod writer;

pub use annot::{Annot, AnnotBody, AnnotOp, CheckStyle, EditState, FontFamily, Point};
pub use engine::{EditRequest, Engine};
pub use error::{Error, PdfError, Result};
pub use types::*;
