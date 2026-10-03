use serde::Serialize;
use specta::Type;

#[derive(Debug, thiserror::Error, Serialize, Type, Clone, PartialEq)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum PdfError {
    #[error("mot de passe requis")]
    PasswordRequired,
    #[error("mot de passe incorrect")]
    WrongPassword,
    #[error("fichier introuvable : {0}")]
    NotFound(String),
    #[error("fichier PDF illisible : {0}")]
    Invalid(String),
    #[error("document inconnu")]
    UnknownDocument,
    #[error("page hors limites")]
    PageOutOfRange,
    #[error("demande annulée")]
    Cancelled,
    #[error("moteur PDF indisponible : {0}")]
    Engine(String),
}

/// Alias court utilisé dans tout le crate.
pub type Error = PdfError;

pub type Result<T> = std::result::Result<T, Error>;

impl From<pdfium_render::prelude::PdfiumError> for PdfError {
    fn from(e: pdfium_render::prelude::PdfiumError) -> Self {
        Error::Engine(format!("{e:?}"))
    }
}
