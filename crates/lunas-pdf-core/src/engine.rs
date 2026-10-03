//! Acteur PDFium : un fil dédié possède la bibliothèque (non thread-safe) et tous les
//! documents ouverts. Les autres fils lui parlent par messages.
//!
//! Ordonnancement : les commandes courtes sont traitées dès réception ; les rendus passent
//! par une file à priorités (le plus urgent, puis le plus récent) ; la recherche avance par
//! petits lots entre deux rendus pour ne jamais bloquer l'affichage.

use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use pdfium_render::prelude::*;

use crate::annot::{Annot, AnnotOp, EditState, Point};
use crate::error::{Error, Result};
use crate::text::{Glyph, build_runs, search_page};
use crate::types::*;
use crate::writer::{Editor, ImportedImage};

/// Nombre de pages dont le texte reste en cache par document.
const TEXT_CACHE_PAGES: usize = 64;

pub type SearchSink = Box<dyn FnMut(SearchEvent) + Send>;

enum Msg {
    Open {
        path: PathBuf,
        password: Option<String>,
        reply: Sender<Result<DocInfo>>,
    },
    Close {
        doc: DocId,
    },
    SetEpoch {
        doc: DocId,
        epoch: u32,
    },
    Render {
        req: RenderRequest,
        reply: Sender<Result<Bitmap>>,
    },
    Outline {
        doc: DocId,
        reply: Sender<Result<Vec<OutlineItem>>>,
    },
    Edit {
        doc: DocId,
        req: EditRequest,
        reply: Sender<Result<EditState>>,
    },
    TextRange {
        doc: DocId,
        page: u32,
        from: Point,
        to: Point,
        reply: Sender<Result<TextSelection>>,
    },
    ImportImage {
        doc: DocId,
        bytes: Vec<u8>,
        reply: Sender<Result<ImageInfo>>,
    },
    CopyImage {
        from: DocId,
        to: DocId,
        key: String,
        reply: Sender<Result<()>>,
    },
    Save {
        doc: DocId,
        path: Option<PathBuf>,
        reply: Sender<Result<DocInfo>>,
    },
    SetAuthor {
        name: String,
    },
    Links {
        doc: DocId,
        page: u32,
        reply: Sender<Result<Vec<LinkInfo>>>,
    },
    Export {
        doc: DocId,
        opts: crate::export::ExportOptions,
        /// `None` : estimation de taille seulement.
        path: Option<PathBuf>,
        reply: Sender<Result<crate::export::ExportResult>>,
    },
    ClipPages {
        doc: DocId,
        pages: Vec<u32>,
        reply: Sender<Result<DocInfo>>,
    },
    Extract {
        doc: DocId,
        pages: Vec<u32>,
        path: PathBuf,
        reply: Sender<Result<()>>,
    },
    SavedBytes {
        doc: DocId,
        reply: Sender<Result<SavedFile>>,
    },
    Text {
        doc: DocId,
        page: u32,
        reply: Sender<Result<PageText>>,
    },
    Search {
        doc: DocId,
        search_id: u32,
        query: String,
        case_sensitive: bool,
        sink: SearchSink,
    },
    CancelSearch {
        doc: DocId,
    },
    Png {
        doc: DocId,
        page: u32,
        width: u32,
        reply: Sender<Result<Vec<u8>>>,
    },
}

/// Octets enregistrés d'un document et son mot de passe.
pub type SavedFile = (Arc<Vec<u8>>, Option<String>);

/// Demande d'édition des annotations (voir `writer::Editor`).
#[derive(Debug, Clone)]
pub enum EditRequest {
    /// Charge le modèle (annotations existantes) sans rien modifier.
    Load,
    Apply(Vec<AnnotOp>),
    /// Opération sur les pages (un pas d'annulation).
    Pages(crate::pages::PageOp),
    Undo,
    Redo,
    SetHidden {
        id: String,
        hidden: bool,
    },
}

/// Poignée clonable vers l'acteur PDFium.
#[derive(Clone)]
pub struct Engine {
    tx: Sender<Msg>,
}

impl Engine {
    /// Démarre l'acteur. `pdfium_dir` : dossier contenant `libpdfium` ; à défaut, la
    /// bibliothèque système est utilisée.
    pub fn start(pdfium_dir: Option<&Path>) -> Result<Engine> {
        let bindings = match pdfium_dir {
            Some(dir) => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))
                .or_else(|_| Pdfium::bind_to_system_library()),
            None => Pdfium::bind_to_system_library(),
        }
        .map_err(|e| Error::Engine(format!("libpdfium introuvable : {e:?}")))?;
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("pdfium".into())
            .spawn(move || {
                let pdfium: &'static Pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
                Actor::new(pdfium).run(rx);
            })
            .map_err(|e| Error::Engine(e.to_string()))?;
        Ok(Engine { tx })
    }

    /// Nom de fichier de libpdfium sur la plateforme courante.
    pub fn platform_library_name() -> std::ffi::OsString {
        Pdfium::pdfium_platform_library_name()
    }

    fn call<T>(&self, f: impl FnOnce(Sender<Result<T>>) -> Msg) -> Result<T> {
        let (tx, rx) = channel();
        self.tx.send(f(tx)).map_err(|_| Error::Engine("acteur arrêté".into()))?;
        rx.recv().map_err(|_| Error::Engine("acteur arrêté".into()))?
    }

    pub fn open(&self, path: impl Into<PathBuf>, password: Option<String>) -> Result<DocInfo> {
        let path = path.into();
        self.call(|reply| Msg::Open { path, password, reply })
    }
    pub fn close(&self, doc: DocId) {
        let _ = self.tx.send(Msg::Close { doc });
    }
    /// Toute demande de rendu d'une époque antérieure sera abandonnée (défilement, zoom).
    pub fn set_epoch(&self, doc: DocId, epoch: u32) {
        let _ = self.tx.send(Msg::SetEpoch { doc, epoch });
    }
    pub fn render(&self, req: RenderRequest) -> Result<Bitmap> {
        self.call(|reply| Msg::Render { req, reply })
    }
    pub fn outline(&self, doc: DocId) -> Result<Vec<OutlineItem>> {
        self.call(|reply| Msg::Outline { doc, reply })
    }
    /// Annotations du document (modèle d'édition, chargé à la demande).
    pub fn annotations(&self, doc: DocId) -> Result<Vec<Annot>> {
        self.edit(doc, EditRequest::Load).map(|s| s.annots)
    }
    pub fn edit(&self, doc: DocId, req: EditRequest) -> Result<EditState> {
        self.call(|reply| Msg::Edit { doc, req, reply })
    }
    /// Boîtes des lignes de texte entre deux points (outils surligner, souligner, barrer).
    pub fn text_range(&self, doc: DocId, page: u32, from: Point, to: Point) -> Result<TextSelection> {
        self.call(|reply| Msg::TextRange {
            doc,
            page,
            from,
            to,
            reply,
        })
    }
    pub fn import_image(&self, doc: DocId, path: impl Into<PathBuf>) -> Result<ImageInfo> {
        let path = path.into();
        let bytes = std::fs::read(&path).map_err(|e| Error::NotFound(format!("{}: {e}", path.display())))?;
        self.import_image_bytes(doc, bytes)
    }
    /// Image PNG ou JPEG fournie par l'interface (signature dessinée, tapée…).
    pub fn import_image_bytes(&self, doc: DocId, bytes: Vec<u8>) -> Result<ImageInfo> {
        self.call(|reply| Msg::ImportImage { doc, bytes, reply })
    }
    /// Rend une image importée dans `from` utilisable dans `to` (même clé) : collage d'un
    /// tampon image d'un document à l'autre.
    pub fn copy_image(&self, from: DocId, to: DocId, key: String) -> Result<()> {
        self.call(|reply| Msg::CopyImage { from, to, key, reply })
    }
    /// Enregistre (sur place ou sous un autre nom) ; écriture atomique.
    pub fn save(&self, doc: DocId, path: Option<PathBuf>) -> Result<DocInfo> {
        self.call(|reply| Msg::Save { doc, path, reply })
    }
    /// Nom d'auteur des nouvelles annotations (`/T`).
    pub fn set_author(&self, name: String) {
        let _ = self.tx.send(Msg::SetAuthor { name });
    }
    /// Exporte le document (planche 10) vers `path` ; renvoie les fichiers écrits.
    pub fn export(
        &self,
        doc: DocId,
        opts: crate::export::ExportOptions,
        path: impl Into<PathBuf>,
    ) -> Result<crate::export::ExportResult> {
        let path = Some(path.into());
        self.call(|reply| Msg::Export { doc, opts, path, reply })
    }
    /// Taille du PDF que produirait l'export (formats PDF seulement), sans rien écrire.
    pub fn export_estimate(&self, doc: DocId, opts: crate::export::ExportOptions) -> Result<crate::export::ExportResult> {
        self.call(|reply| Msg::Export {
            doc,
            opts,
            path: None,
            reply,
        })
    }
    /// Copie figée des pages choisies, ouverte comme document en mémoire (presse-papiers de
    /// pages ; le fermer avec `close` quand il est remplacé).
    pub fn clip_pages(&self, doc: DocId, pages: Vec<u32>) -> Result<DocInfo> {
        self.call(|reply| Msg::ClipPages { doc, pages, reply })
    }
    /// Enregistre les pages choisies (état courant) dans un nouveau PDF.
    pub fn extract_pages(&self, doc: DocId, pages: Vec<u32>, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        self.call(|reply| Msg::Extract { doc, pages, path, reply })
    }
    /// Octets enregistrés (sans la révision en cours d'édition) et mot de passe.
    pub fn saved_file(&self, doc: DocId) -> Result<SavedFile> {
        self.call(|reply| Msg::SavedBytes { doc, reply })
    }
    /// Vérifie les signatures numériques du fichier enregistré (hors du fil du moteur).
    /// `trusted` : autorités approuvées par l'utilisateur (DER), en plus du système et des
    /// listes de confiance.
    pub fn signatures(&self, doc: DocId, trusted: &[Vec<u8>]) -> Result<Vec<crate::signature::SignatureInfo>> {
        let (bytes, password) = self.saved_file(doc)?;
        Ok(crate::signature::verify_document(
            &bytes,
            password.as_deref(),
            &crate::signature::default_store(trusted),
        ))
    }
    pub fn links(&self, doc: DocId, page: u32) -> Result<Vec<LinkInfo>> {
        self.call(|reply| Msg::Links { doc, page, reply })
    }
    pub fn text(&self, doc: DocId, page: u32) -> Result<PageText> {
        self.call(|reply| Msg::Text { doc, page, reply })
    }
    /// Lance une recherche ; les résultats arrivent page par page dans `sink`.
    /// Une nouvelle recherche sur le même document annule la précédente.
    pub fn search(&self, doc: DocId, search_id: u32, query: String, case_sensitive: bool, sink: SearchSink) {
        let _ = self.tx.send(Msg::Search {
            doc,
            search_id,
            query,
            case_sensitive,
            sink,
        });
    }
    pub fn cancel_search(&self, doc: DocId) {
        let _ = self.tx.send(Msg::CancelSearch { doc });
    }
    /// Rendu PNG (miniatures des récents).
    pub fn png(&self, doc: DocId, page: u32, width: u32) -> Result<Vec<u8>> {
        self.call(|reply| Msg::Png { doc, page, width, reply })
    }
}

struct Job {
    req: RenderRequest,
    reply: Sender<Result<Bitmap>>,
    seq: u64,
}
impl PartialEq for Job {
    fn eq(&self, o: &Self) -> bool {
        self.seq == o.seq
    }
}
impl Eq for Job {}
impl PartialOrd for Job {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Job {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        (self.req.priority, self.seq).cmp(&(o.req.priority, o.seq))
    }
}

struct Search {
    doc: DocId,
    id: u32,
    query: String,
    case_sensitive: bool,
    next: u32,
    total: u32,
    sink: SearchSink,
}

struct Doc {
    /// Déclaré avant `bytes` : PDFium lit dans `bytes`, qui doit lui survivre.
    pdf: PdfDocument<'static>,
    bytes: Arc<Vec<u8>>,
    password: Option<String>,
    editor: Option<Editor>,
    info: DocInfo,
    epoch: u32,
    glyphs: HashMap<u32, Arc<Vec<Glyph>>>,
    glyph_order: VecDeque<u32>,
}

struct Actor {
    pdfium: &'static Pdfium,
    docs: HashMap<DocId, Doc>,
    next_id: DocId,
    renders: BinaryHeap<Job>,
    search: Option<Search>,
    seq: u64,
    author: String,
}

impl Actor {
    fn new(pdfium: &'static Pdfium) -> Self {
        Actor {
            pdfium,
            docs: HashMap::new(),
            next_id: 1,
            renders: BinaryHeap::new(),
            search: None,
            seq: 0,
            author: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_default(),
        }
    }

    fn run(mut self, rx: Receiver<Msg>) {
        loop {
            if self.renders.is_empty() && self.search.is_none() {
                match rx.recv() {
                    Ok(m) => self.handle(m),
                    Err(_) => return,
                }
            }
            while let Ok(m) = rx.try_recv() {
                self.handle(m);
            }
            if let Some(job) = self.renders.pop() {
                let res = self.render(&job.req);
                let _ = job.reply.send(res);
            } else if self.search.is_some() {
                self.search_step(Duration::from_millis(16));
            }
        }
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Open { path, password, reply } => {
                let _ = reply.send(self.open(&path, password.as_deref()));
            }
            Msg::Close { doc } => {
                self.docs.remove(&doc);
                if self.search.as_ref().is_some_and(|s| s.doc == doc) {
                    self.search = None;
                }
            }
            Msg::SetEpoch { doc, epoch } => {
                if let Some(d) = self.docs.get_mut(&doc) {
                    d.epoch = epoch;
                }
                // Abandon immédiat des rendus périmés.
                let docs = &self.docs;
                let (keep, drop): (Vec<Job>, Vec<Job>) = std::mem::take(&mut self.renders)
                    .into_iter()
                    .partition(|j| docs.get(&j.req.doc).is_some_and(|d| j.req.epoch >= d.epoch));
                for j in drop {
                    let _ = j.reply.send(Err(Error::Cancelled));
                }
                self.renders = keep.into_iter().collect();
            }
            Msg::Render { req, reply } => {
                self.seq += 1;
                self.renders.push(Job {
                    req,
                    reply,
                    seq: self.seq,
                });
            }
            Msg::Outline { doc, reply } => {
                let _ = reply.send(self.doc(doc).map(|d| outline(&d.pdf)));
            }
            Msg::Edit { doc, req, reply } => {
                let _ = reply.send(self.edit(doc, req));
            }
            Msg::TextRange {
                doc,
                page,
                from,
                to,
                reply,
            } => {
                let _ = reply.send(self.glyphs(doc, page).map(|g| crate::text::range(&g, from, to)));
            }
            Msg::ImportImage { doc, bytes, reply } => {
                let _ = reply.send(self.import_image(doc, bytes));
            }
            Msg::CopyImage { from, to, key, reply } => {
                let _ = reply.send(self.copy_image(from, to, key));
            }
            Msg::Save { doc, path, reply } => {
                let _ = reply.send(self.save(doc, path));
            }
            Msg::SetAuthor { name } => {
                self.author = name;
            }
            Msg::Export { doc, opts, path, reply } => {
                let _ = reply.send(self.export(doc, &opts, path));
            }
            Msg::ClipPages { doc, pages, reply } => {
                let _ = reply.send(self.clip_pages(doc, pages));
            }
            Msg::Extract { doc, pages, path, reply } => {
                let _ = reply.send(self.extract(doc, pages, path));
            }
            Msg::SavedBytes { doc, reply } => {
                let _ = reply.send(self.doc(doc).map(|d| {
                    // Octets enregistrés (sans la révision en cours d'édition).
                    let bytes = d.editor.as_ref().map(|e| e.disk().clone()).unwrap_or_else(|| d.bytes.clone());
                    (bytes, d.password.clone())
                }));
            }
            Msg::Links { doc, page, reply } => {
                let _ = reply.send(self.doc(doc).and_then(|d| links(&d.pdf, page)));
            }
            Msg::Text { doc, page, reply } => {
                let _ = reply.send(self.glyphs(doc, page).map(|g| PageText { runs: build_runs(&g) }));
            }
            Msg::Search {
                doc,
                search_id,
                query,
                case_sensitive,
                sink,
            } => {
                self.search = Some(Search {
                    doc,
                    id: search_id,
                    query,
                    case_sensitive,
                    next: 0,
                    total: 0,
                    sink,
                });
            }
            Msg::CancelSearch { doc } => {
                if self.search.as_ref().is_some_and(|s| s.doc == doc) {
                    self.search = None;
                }
            }
            Msg::Png { doc, page, width, reply } => {
                let _ = reply.send(self.png(doc, page, width));
            }
        }
    }

    fn doc(&self, id: DocId) -> Result<&Doc> {
        self.docs.get(&id).ok_or(Error::UnknownDocument)
    }

    /// Charge un document PDFium depuis des octets partagés (rechargeable après édition).
    fn load(&self, bytes: &Arc<Vec<u8>>, password: Option<&str>) -> Result<PdfDocument<'static>> {
        // SAFETY : le `PdfDocument` est toujours détruit avant les octets qu'il lit (ordre des
        // champs de `Doc`, et `reload` remplace le document avant les octets).
        let slice: &'static [u8] = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes.as_slice()) };
        match self.pdfium.load_pdf_from_byte_slice(slice, password) {
            Ok(d) => Ok(d),
            Err(PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError)) => Err(if password.is_some() {
                Error::WrongPassword
            } else {
                Error::PasswordRequired
            }),
            Err(e) => Err(Error::Invalid(format!("{e:?}"))),
        }
    }

    fn open(&mut self, path: &Path, password: Option<&str>) -> Result<DocInfo> {
        if !path.exists() {
            return Err(Error::NotFound(path.display().to_string()));
        }
        let bytes = Arc::new(std::fs::read(path).map_err(|e| Error::NotFound(format!("{}: {e}", path.display())))?);
        self.open_bytes(bytes, path, password)
    }

    /// Octets du document dans l'état courant, comme à l'enregistrement (caviardage appliqué).
    fn final_bytes(&mut self, doc: DocId) -> Result<(Vec<u8>, Option<String>)> {
        let d = self.docs.get(&doc).ok_or(Error::UnknownDocument)?;
        let password = d.password.clone();
        let Some(ed) = d.editor.as_ref() else {
            return Ok((d.bytes.to_vec(), password));
        };
        let bytes = if ed.state(vec![]).pending_redactions {
            crate::redact::apply(&ed.build(true)?, password.as_deref())?
        } else {
            ed.build(true)?
        };
        Ok((bytes, password))
    }

    fn export(
        &mut self,
        doc: DocId,
        opts: &crate::export::ExportOptions,
        path: Option<PathBuf>,
    ) -> Result<crate::export::ExportResult> {
        use crate::export::{ExportFormat, ExportResult, finish_pdf, image_path, jpeg_quality};
        let count = self.docs.get(&doc).ok_or(Error::UnknownDocument)?.info.pages.len() as u32;
        let pages: Vec<u32> = match &opts.pages {
            Some(p) => p.iter().copied().filter(|i| *i < count).collect(),
            None => (0..count).collect(),
        };
        if pages.is_empty() {
            return Err(Error::Invalid("aucune page".into()));
        }
        if let ExportFormat::Png | ExportFormat::Jpg = opts.format {
            let Some(path) = path else {
                return Ok(ExportResult { files: vec![], size: 0 });
            };
            let jpg = opts.format == ExportFormat::Jpg;
            let d = self.doc(doc)?;
            let scale = opts.dpi.clamp(36, 600) as f32 / 72.0;
            let mut files = vec![];
            let mut size = 0;
            for (k, &i) in pages.iter().enumerate() {
                let page = d.pdf.pages().get(i as i32)?;
                let cfg = PdfRenderConfig::new()
                    .scale_page_by_factor(scale)
                    .render_annotations(true)
                    .render_form_data(true);
                let img = page.render_with_config(&cfg)?.as_image()?;
                let mut out = vec![];
                if jpg {
                    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(
                        std::io::Cursor::new(&mut out),
                        jpeg_quality(opts.quality),
                    );
                    image::DynamicImage::ImageRgb8(img.to_rgb8())
                        .write_with_encoder(enc)
                        .map_err(|e| Error::Engine(e.to_string()))?;
                } else {
                    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                        .map_err(|e| Error::Engine(e.to_string()))?;
                }
                let target = image_path(&path, k, pages.len(), if jpg { "jpg" } else { "png" });
                crate::fsutil::write_atomic(&target, &out).map_err(|e| Error::Engine(format!("enregistrement : {e}")))?;
                size += out.len() as u64;
                files.push(target.display().to_string());
            }
            return Ok(ExportResult { files, size });
        }
        let (mut bytes, mut password) = self.final_bytes(doc)?;
        if pages.len() != count as usize || pages.iter().enumerate().any(|(k, p)| *p != k as u32) {
            let opts_l = lopdf::LoadOptions {
                password: password.clone(),
                ..Default::default()
            };
            let src = lopdf::Document::load_mem_with_options(&bytes, opts_l).map_err(|e| Error::Invalid(e.to_string()))?;
            let order: Vec<lopdf::ObjectId> = src.get_pages().into_values().collect();
            let ids: Vec<lopdf::ObjectId> = pages.iter().filter_map(|p| order.get(*p as usize).copied()).collect();
            bytes = crate::pages::extract(&src, &ids)?;
            password = None;
        }
        if opts.format == ExportFormat::Flattened {
            let arc = Arc::new(bytes);
            let flat = {
                let pdf = self.load(&arc, password.as_deref())?;
                for p in pdf.pages().iter() {
                    let mut p = p;
                    p.flatten()?;
                }
                pdf.save_to_bytes()?
            };
            bytes = flat;
        }
        let out = finish_pdf(
            &bytes,
            password.as_deref(),
            opts.quality,
            opts.protection.as_ref(),
            opts.format == ExportFormat::Flattened,
        )?;
        let size = out.len() as u64;
        let mut files = vec![];
        if let Some(path) = path {
            crate::fsutil::write_atomic(&path, &out).map_err(|e| Error::Engine(format!("enregistrement : {e}")))?;
            files.push(path.display().to_string());
        }
        Ok(ExportResult { files, size })
    }

    /// Copie figée de pages (presse-papiers de pages) : document en mémoire, sans fichier.
    fn clip_pages(&mut self, doc: DocId, pages: Vec<u32>) -> Result<DocInfo> {
        let src = self.current_document(doc)?;
        let order: Vec<lopdf::ObjectId> = src.get_pages().into_values().collect();
        let ids: Vec<lopdf::ObjectId> = pages.iter().filter_map(|p| order.get(*p as usize).copied()).collect();
        if ids.is_empty() {
            return Err(Error::Invalid("aucune page".into()));
        }
        let bytes = crate::pages::extract(&src, &ids)?;
        self.open_bytes(Arc::new(bytes), Path::new("pages.pdf"), None)
    }

    fn open_bytes(&mut self, bytes: Arc<Vec<u8>>, path: &Path, password: Option<&str>) -> Result<DocInfo> {
        let pdf = self.load(&bytes, password)?;
        let id = self.next_id;
        self.next_id += 1;

        let geoms = page_geoms(&pdf)?;
        let form = match pdf.form().map(|f| f.form_type()) {
            Some(PdfFormType::XfaFull | PdfFormType::XfaForeground) => FormKind::Xfa,
            Some(PdfFormType::Acrobat) => FormKind::AcroForm,
            _ => FormKind::None,
        };
        let perms = pdf.permissions();
        let info = DocInfo {
            id,
            path: path.display().to_string(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            title: pdf
                .metadata()
                .get(PdfDocumentMetadataTagType::Title)
                .map(|t| t.value().to_owned())
                .filter(|t| !t.trim().is_empty()),
            pages: geoms,
            encrypted: password.is_some(),
            form,
            signature_count: pdf.signatures().len() as u32,
            // pdfium-render ne décode pas les permissions R6 : en cas d'erreur, on autorise
            // (la lecture du document est déjà permise par le mot de passe).
            can_copy: perms.can_extract_text_and_graphics().unwrap_or(true),
            can_print: perms.can_print_high_quality().unwrap_or(true) || perms.can_print_only_low_quality().unwrap_or(false),
        };
        self.docs.insert(
            id,
            Doc {
                pdf,
                bytes,
                password: password.map(str::to_owned),
                editor: None,
                info: info.clone(),
                epoch: 0,
                glyphs: HashMap::new(),
                glyph_order: VecDeque::new(),
            },
        );
        Ok(info)
    }

    // --- Édition ---------------------------------------------------------------------------------

    fn editor(&mut self, doc: DocId) -> Result<&mut Doc> {
        let author = self.author.clone();
        let d = self.docs.get_mut(&doc).ok_or(Error::UnknownDocument)?;
        if d.editor.is_none() {
            d.editor = Some(Editor::open(d.bytes.clone(), d.password.as_deref(), author)?);
        }
        Ok(d)
    }

    /// Remplace les octets du document et recharge PDFium (même identifiant de document).
    fn reload(&mut self, doc: DocId, bytes: Vec<u8>) -> Result<()> {
        let bytes = Arc::new(bytes);
        let password = self.docs.get(&doc).ok_or(Error::UnknownDocument)?.password.clone();
        let pdf = self.load(&bytes, password.as_deref())?;
        let d = self.docs.get_mut(&doc).ok_or(Error::UnknownDocument)?;
        let old = std::mem::replace(&mut d.pdf, pdf);
        drop(old);
        d.bytes = bytes;
        Ok(())
    }

    /// Document lopdf d'un onglet dans son état courant (source d'une copie de pages).
    fn current_document(&mut self, doc: DocId) -> Result<lopdf::Document> {
        let d = self.editor(doc)?;
        let bytes = d.editor.as_ref().unwrap().build(false)?;
        let opts = lopdf::LoadOptions {
            password: d.password.clone(),
            ..Default::default()
        };
        lopdf::Document::load_mem_with_options(&bytes, opts).map_err(|e| Error::Invalid(e.to_string()))
    }

    /// Nouveau PDF des pages choisies, écrit à `path`.
    fn extract(&mut self, doc: DocId, pages: Vec<u32>, path: PathBuf) -> Result<()> {
        let src = self.current_document(doc)?;
        let order: Vec<lopdf::ObjectId> = src.get_pages().into_values().collect();
        let ids: Vec<lopdf::ObjectId> = pages.iter().filter_map(|p| order.get(*p as usize).copied()).collect();
        if ids.is_empty() {
            return Err(Error::Invalid("aucune page".into()));
        }
        let bytes = crate::pages::extract(&src, &ids)?;
        crate::fsutil::write_atomic(&path, &bytes).map_err(|e| Error::Engine(format!("enregistrement : {e}")))
    }

    fn edit(&mut self, doc: DocId, req: EditRequest) -> Result<EditState> {
        let src = match &req {
            EditRequest::Pages(crate::pages::PageOp::Import { from, .. }) => Some(self.current_document(*from)?),
            _ => None,
        };
        let d = self.editor(doc)?;
        let ed = d.editor.as_mut().unwrap();
        let rev = ed.structure_rev;
        let changed = match req {
            EditRequest::Load => vec![],
            EditRequest::Apply(ops) => ed.apply(ops),
            EditRequest::Pages(op) => ed.apply_pages(&op, src.as_ref())?,
            EditRequest::Undo => ed.undo(),
            EditRequest::Redo => ed.redo(),
            EditRequest::SetHidden { id, hidden } => ed.set_hidden(&id, hidden),
        };
        if !changed.is_empty() {
            let bytes = ed.build(false)?;
            self.reload(doc, bytes)?;
        }
        // Pages ajoutées, retirées, déplacées ou pivotées : nouvelle géométrie, textes à relire.
        let mut pages = None;
        let d = self.docs.get_mut(&doc).ok_or(Error::UnknownDocument)?;
        if d.editor.as_ref().is_some_and(|e| e.structure_rev != rev) {
            let g = page_geoms(&d.pdf)?;
            d.info.pages = g.clone();
            d.glyphs.clear();
            d.glyph_order.clear();
            pages = Some(g);
        }
        self.fill_excerpts(doc);
        let ed = self
            .docs
            .get(&doc)
            .and_then(|d| d.editor.as_ref())
            .ok_or(Error::UnknownDocument)?;
        let mut st = ed.state(changed);
        st.pages = pages;
        Ok(st)
    }

    /// Texte recouvert par les annotations de marquage (pour la liste latérale).
    fn fill_excerpts(&mut self, doc: DocId) {
        let todo: Vec<(usize, u32, Vec<Rect>)> = match self.docs.get(&doc).and_then(|d| d.editor.as_ref()) {
            Some(ed) => ed
                .annots
                .iter()
                .enumerate()
                .filter(|(_, a)| a.excerpt.is_none())
                .filter_map(|(i, a)| a.body.quads().map(|q| (i, a.page, q.to_vec())))
                .collect(),
            None => return,
        };
        for (i, page, quads) in todo {
            let text = self
                .glyphs(doc, page)
                .map(|g| crate::text::text_in(&g, &quads))
                .unwrap_or_default();
            if let Some(a) = self
                .docs
                .get_mut(&doc)
                .and_then(|d| d.editor.as_mut())
                .and_then(|e| e.annots.get_mut(i))
            {
                a.excerpt = Some(text);
            }
        }
    }

    fn import_image(&mut self, doc: DocId, bytes: Vec<u8>) -> Result<ImageInfo> {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| Error::Invalid(e.to_string()))?
            .into_dimensions()
            .map_err(|e| Error::Invalid(format!("image illisible : {e}")))?;
        let key = format!(
            "img{}",
            crate::writer::pdf_date_now().trim_start_matches("D:").trim_end_matches('Z')
        ) + &format!("{:x}n{}", bytes.len(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let d = self.editor(doc)?;
        d.editor.as_mut().unwrap().add_image(
            key.clone(),
            ImportedImage {
                bytes: Arc::new(bytes),
                width,
                height,
            },
        );
        Ok(ImageInfo { key, width, height })
    }

    fn copy_image(&mut self, from: DocId, to: DocId, key: String) -> Result<()> {
        let img = self
            .editor(from)?
            .editor
            .as_ref()
            .unwrap()
            .image(&key)
            .ok_or_else(|| Error::NotFound(format!("image {key}")))?;
        self.editor(to)?.editor.as_mut().unwrap().add_image(key, img);
        Ok(())
    }

    fn save(&mut self, doc: DocId, path: Option<PathBuf>) -> Result<DocInfo> {
        let d = self.editor(doc)?;
        let target = path.unwrap_or_else(|| PathBuf::from(&d.info.path));
        let ed = d.editor.as_ref().unwrap();
        let bytes = if ed.state(vec![]).pending_redactions {
            crate::redact::apply(&ed.build(true)?, d.password.as_deref())?
        } else {
            ed.build(true)?
        };
        crate::fsutil::write_atomic(&target, &bytes).map_err(|e| Error::Engine(format!("enregistrement : {e}")))?;
        let redacted = ed.state(vec![]).pending_redactions;
        self.reload(doc, bytes)?;
        let d = self.docs.get_mut(&doc).ok_or(Error::UnknownDocument)?;
        let (base, password) = (d.bytes.clone(), d.password.clone());
        d.editor.as_mut().unwrap().rebase(base, password.as_deref())?;
        if redacted {
            // Le texte a changé : vider les caches de glyphes.
            d.glyphs.clear();
            d.glyph_order.clear();
        }
        d.info.path = target.display().to_string();
        d.info.name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(d.info.clone())
    }

    fn render(&mut self, req: &RenderRequest) -> Result<Bitmap> {
        let d = self.doc(req.doc)?;
        if req.epoch < d.epoch {
            return Err(Error::Cancelled);
        }
        if req.page as usize >= d.info.pages.len() {
            return Err(Error::PageOutOfRange);
        }
        let page = d.pdf.pages().get(req.page as i32)?;
        let tile = req.tile.unwrap_or(Tile {
            x: 0,
            y: 0,
            w: req.width,
            h: req.height,
        });
        let mut bmp = PdfBitmap::empty(tile.w as i32, tile.h as i32, PdfBitmapFormat::BGRA)?;
        let cfg = PdfRenderConfig::new()
            .set_fixed_size(req.width as i32, req.height as i32)
            .set_origin(-(tile.x as i32), -(tile.y as i32))
            .set_reverse_byte_order(true)
            .render_annotations(true)
            .render_form_data(true)
            .set_clear_color(PdfColor::WHITE);
        page.render_into_bitmap_with_config(&mut bmp, &cfg)?;
        Ok(Bitmap {
            width: tile.w,
            height: tile.h,
            rgba: bmp.as_raw_bytes(),
        })
    }

    fn png(&mut self, doc: DocId, page: u32, width: u32) -> Result<Vec<u8>> {
        let d = self.doc(doc)?;
        let g = d.info.pages.get(page as usize).ok_or(Error::PageOutOfRange)?;
        let height = ((width as f32) * g.height / g.width).round().max(1.0) as u32;
        let bmp = self.render(&RenderRequest {
            doc,
            page,
            width,
            height,
            tile: None,
            priority: 0,
            epoch: u32::MAX,
        })?;
        let img = image::RgbaImage::from_raw(bmp.width, bmp.height, bmp.rgba).ok_or(Error::Engine("bitmap".into()))?;
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| Error::Engine(e.to_string()))?;
        Ok(out.into_inner())
    }

    /// Glyphes de la page en coordonnées d'affichage (cache LRU par document).
    fn glyphs(&mut self, doc: DocId, page: u32) -> Result<Arc<Vec<Glyph>>> {
        let d = self.docs.get_mut(&doc).ok_or(Error::UnknownDocument)?;
        if let Some(g) = d.glyphs.get(&page) {
            return Ok(g.clone());
        }
        let geom = d.info.pages.get(page as usize).ok_or(Error::PageOutOfRange)?.clone();
        let p = d.pdf.pages().get(page as i32)?;
        let tr = DisplayTransform::new(&p, &geom)?;
        let text = p.text()?;
        let mut out = Vec::with_capacity(text.len().max(0) as usize);
        for ch in text.chars().iter() {
            let Some(c) = ch.unicode_char() else { continue };
            let rect = match ch.loose_bounds() {
                Ok(b) if b.width().value > 0.0 && b.height().value > 0.0 => tr.rect(&b),
                _ => Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                },
            };
            out.push(Glyph { c, rect });
        }
        let g = Arc::new(out);
        d.glyphs.insert(page, g.clone());
        d.glyph_order.push_back(page);
        if d.glyph_order.len() > TEXT_CACHE_PAGES
            && let Some(old) = d.glyph_order.pop_front()
        {
            d.glyphs.remove(&old);
        }
        Ok(g)
    }

    fn search_step(&mut self, budget: Duration) {
        let start = Instant::now();
        let Some(mut s) = self.search.take() else { return };
        let Some(count) = self.docs.get(&s.doc).map(|d| d.info.pages.len() as u32) else {
            return;
        };
        while s.next < count && start.elapsed() < budget {
            let page = s.next;
            s.next += 1;
            let hits = match self.glyphs(s.doc, page) {
                Ok(g) => search_page(page, &g, &s.query, s.case_sensitive),
                Err(_) => vec![],
            };
            s.total += hits.len() as u32;
            (s.sink)(SearchEvent::Page {
                search_id: s.id,
                page,
                hits,
            });
        }
        if s.next >= count {
            (s.sink)(SearchEvent::Done {
                search_id: s.id,
                total: s.total,
            });
        } else {
            self.search = Some(s);
        }
    }
}

/// Transformation affine espace page PDF → points d'affichage (origine en haut à gauche),
/// déduite de `FPDF_PageToDevice` pour respecter rotation et CropBox.
struct DisplayTransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl DisplayTransform {
    const SCALE: f32 = 16.0;

    fn new(page: &PdfPage, geom: &PageGeom) -> Result<Self> {
        let cfg = PdfRenderConfig::new().set_fixed_size((geom.width * Self::SCALE) as i32, (geom.height * Self::SCALE) as i32);
        let f = |x: f32, y: f32| -> Result<(f32, f32)> {
            let (dx, dy) = page.points_to_pixels(PdfPoints::new(x), PdfPoints::new(y), &cfg)?;
            Ok((dx as f32 / Self::SCALE, dy as f32 / Self::SCALE))
        };
        let (e, ff) = f(0.0, 0.0)?;
        let (x1, y1) = f(1000.0, 0.0)?;
        let (x2, y2) = f(0.0, 1000.0)?;
        Ok(DisplayTransform {
            a: (x1 - e) / 1000.0,
            b: (y1 - ff) / 1000.0,
            c: (x2 - e) / 1000.0,
            d: (y2 - ff) / 1000.0,
            e,
            f: ff,
        })
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    fn rect(&self, r: &PdfRect) -> Rect {
        let pts = [
            self.point(r.left().value, r.bottom().value),
            self.point(r.right().value, r.bottom().value),
            self.point(r.left().value, r.top().value),
            self.point(r.right().value, r.top().value),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (x, y) in pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }
}

fn outline(pdf: &PdfDocument) -> Vec<OutlineItem> {
    fn walk(b: PdfBookmark, depth: usize) -> OutlineItem {
        let page = b
            .destination()
            .and_then(|d| d.page_index().ok())
            .or_else(|| match b.action() {
                Some(PdfAction::LocalDestination(a)) => a.destination().ok().and_then(|d| d.page_index().ok()),
                _ => None,
            })
            .map(|p| p as u32);
        let children = if depth < 32 {
            b.iter_direct_children().map(|c| walk(c, depth + 1)).collect()
        } else {
            vec![]
        };
        OutlineItem {
            title: b.title().unwrap_or_default(),
            page,
            children,
        }
    }
    // `iter_siblings()` exclut le premier nœud racine : parcours explicite.
    let mut out = vec![];
    let mut cur = pdf.bookmarks().root();
    while let Some(b) = cur {
        cur = b.next_sibling();
        out.push(walk(b, 0));
    }
    out
}

fn links(pdf: &PdfDocument, page: u32) -> Result<Vec<LinkInfo>> {
    let p = pdf.pages().get(page as i32)?;
    let geom = PageGeom {
        width: p.width().value,
        height: p.height().value,
        label: None,
    };
    let tr = DisplayTransform::new(&p, &geom)?;
    let mut out = vec![];
    for l in p.links().iter() {
        let Ok(r) = l.rect() else { continue };
        let target = if let Some(d) = l.destination() {
            d.page_index().ok().map(|p| LinkTarget::Page { page: p as u32 })
        } else {
            match l.action() {
                Some(PdfAction::Uri(u)) => u.uri().ok().map(|uri| LinkTarget::Uri { uri }),
                Some(PdfAction::LocalDestination(a)) => a
                    .destination()
                    .ok()
                    .and_then(|d| d.page_index().ok())
                    .map(|p| LinkTarget::Page { page: p as u32 }),
                _ => None,
            }
        };
        if let Some(target) = target {
            let link = LinkInfo {
                rect: tr.rect(&r),
                target,
            };
            // L'itérateur de pdfium-render 0.9.4 peut renvoyer le premier lien deux fois.
            if !out
                .iter()
                .any(|o: &LinkInfo| o.rect == link.rect && format!("{:?}", o.target) == format!("{:?}", link.target))
            {
                out.push(link);
            }
        }
    }
    Ok(out)
}

/// Taille affichée (rotation comprise) et libellé de chaque page.
fn page_geoms(pdf: &PdfDocument) -> Result<Vec<PageGeom>> {
    let pages = pdf.pages();
    let mut geoms = Vec::with_capacity(pages.len() as usize);
    for i in 0..pages.len() {
        // Taille sans charger la page.
        let r = pages.page_size(i)?;
        geoms.push(PageGeom {
            width: r.width().value,
            height: r.height().value,
            label: None,
        });
    }
    let labels_present = !pages.is_empty() && pages.get(0).ok().and_then(|p| p.label().map(str::to_owned)).is_some();
    if labels_present {
        for (i, g) in geoms.iter_mut().enumerate() {
            g.label = pages.get(i as i32).ok().and_then(|p| p.label().map(str::to_owned));
        }
    }
    Ok(geoms)
}
