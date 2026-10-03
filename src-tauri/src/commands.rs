//! Commandes Tauri typées (types TS générés dans `src/bindings.ts` via tauri-specta).

use std::sync::Mutex;

use lunas_pdf_core::{
    AnnotOp, DocId, DocInfo, EditRequest, EditState, Engine, Error, ImageInfo, LinkInfo, OutlineItem, PageText, Point,
    SearchEvent, TextSelection,
};
use tauri::State;
use tauri::ipc::Channel;

use crate::store::{RecentDoc, SavedSignature, Settings, Store, TrustedRoot, path_id};

type Result<T> = std::result::Result<T, Error>;

/// Fichiers reçus en ligne de commande avant que l'interface soit prête.
pub struct PendingFiles(pub Mutex<Vec<String>>);

const KEYRING_SERVICE: &str = "fr.lunas.feuillet";

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .expect("tâche bloquante interrompue")
}

fn io(e: std::io::Error) -> Error {
    Error::Engine(e.to_string())
}

fn keyring_entry(path: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, &path_id(path)).ok()
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Ouvre un document. Sans mot de passe, essaie celui mémorisé dans le trousseau.
/// `remember` mémorise le mot de passe fourni s'il est correct.
#[tauri::command]
#[specta::specta]
pub async fn open_document(
    engine: State<'_, Engine>,
    store: State<'_, Store>,
    path: String,
    password: Option<String>,
    remember: bool,
) -> Result<DocInfo> {
    let eng = engine.inner().clone();
    let p = path.clone();
    let info = blocking(move || -> Result<DocInfo> {
        let engine = eng;
        match password {
            Some(pw) => {
                let info = engine.open(&p, Some(pw.clone()))?;
                if remember
                    && let Some(e) = keyring_entry(&p)
                    && let Err(err) = e.set_password(&pw)
                {
                    eprintln!("trousseau indisponible : {err}");
                }
                Ok(info)
            }
            None => match engine.open(&p, None) {
                Err(Error::PasswordRequired) => {
                    let Some(entry) = keyring_entry(&p) else {
                        return Err(Error::PasswordRequired);
                    };
                    let Ok(saved) = entry.get_password() else {
                        return Err(Error::PasswordRequired);
                    };
                    match engine.open(&p, Some(saved)) {
                        Err(Error::WrongPassword) => {
                            let _ = entry.delete_credential();
                            Err(Error::PasswordRequired)
                        }
                        r => r,
                    }
                }
                r => r,
            },
        }
    })
    .await;

    // Les documents protégés restent dans les récents, mais sans miniature.
    let locked_recent = |locked| RecentDoc {
        path: path.clone(),
        name: std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        opened_at: now_ms(),
        page_count: 0,
        locked,
        signed: false,
        form: false,
        thumb: None,
    };
    match &info {
        Ok(info) => {
            let mut recent = locked_recent(info.encrypted);
            recent.page_count = info.pages.len() as u32;
            recent.signed = info.signature_count > 0;
            recent.form = info.form != lunas_pdf_core::FormKind::None;
            if !info.encrypted {
                let (engine, id, dir) = (engine.inner().clone(), info.id, store.thumbs_dir.clone());
                let thumb = path_id(&path);
                let file = dir.join(format!("{thumb}.png"));
                let ok = blocking(move || {
                    engine
                        .png(id, 0, 240)
                        .ok()
                        .and_then(|png: Vec<u8>| crate::store::write_atomic(&file, &png).ok())
                })
                .await;
                recent.thumb = ok.map(|_| thumb);
            }
            store.touch_recent(recent).map_err(io)?;
        }
        Err(Error::PasswordRequired | Error::WrongPassword) => {
            store.touch_recent(locked_recent(true)).map_err(io)?;
        }
        Err(_) => {}
    }
    info
}

#[tauri::command]
#[specta::specta]
pub fn close_document(engine: State<'_, Engine>, doc: DocId) {
    engine.close(doc);
}

/// Abandonne les rendus en attente d'une époque antérieure (défilement rapide, zoom).
#[tauri::command]
#[specta::specta]
pub fn set_render_epoch(engine: State<'_, Engine>, doc: DocId, epoch: u32) {
    engine.set_epoch(doc, epoch);
}

#[tauri::command]
#[specta::specta]
pub async fn get_outline(engine: State<'_, Engine>, doc: DocId) -> Result<Vec<OutlineItem>> {
    let e = engine.inner().clone();
    blocking(move || e.outline(doc)).await
}

async fn edit(engine: &Engine, doc: DocId, req: EditRequest) -> Result<EditState> {
    let e = engine.clone();
    blocking(move || e.edit(doc, req)).await
}

/// Modèle d'annotations du document (chargé à la demande).
#[tauri::command]
#[specta::specta]
pub async fn load_annotations(engine: State<'_, Engine>, doc: DocId) -> Result<EditState> {
    edit(&engine, doc, EditRequest::Load).await
}

/// Applique des opérations (ajout, modification, suppression) en une seule étape d'annulation.
#[tauri::command]
#[specta::specta]
pub async fn apply_annotations(engine: State<'_, Engine>, doc: DocId, ops: Vec<AnnotOp>) -> Result<EditState> {
    edit(&engine, doc, EditRequest::Apply(ops)).await
}

/// Opération sur les pages (déplacement, rotation, suppression, duplication, page blanche,
/// copie depuis un autre document), en une étape d'annulation.
#[tauri::command]
#[specta::specta]
pub async fn apply_pages(engine: State<'_, Engine>, doc: DocId, op: lunas_pdf_core::pages::PageOp) -> Result<EditState> {
    edit(&engine, doc, EditRequest::Pages(op)).await
}

/// Ouvre un PDF comme source de pages (fichier déposé sur les miniatures), sans l'ajouter aux
/// documents récents. À fermer avec `close_document`.
#[tauri::command]
#[specta::specta]
pub async fn open_page_source(engine: State<'_, Engine>, path: String) -> Result<DocInfo> {
    let e = engine.inner().clone();
    blocking(move || e.open(&path, None)).await
}

/// Copie figée de pages (presse-papiers de pages) : document en mémoire à coller ailleurs.
#[tauri::command]
#[specta::specta]
pub async fn clip_pages(engine: State<'_, Engine>, doc: DocId, pages: Vec<u32>) -> Result<DocInfo> {
    let e = engine.inner().clone();
    blocking(move || e.clip_pages(doc, pages)).await
}

/// Exporte le document (planche 10) ; renvoie les fichiers écrits.
#[tauri::command]
#[specta::specta]
pub async fn export_document(
    engine: State<'_, Engine>,
    doc: DocId,
    opts: lunas_pdf_core::export::ExportOptions,
    path: String,
) -> Result<lunas_pdf_core::export::ExportResult> {
    let e = engine.inner().clone();
    blocking(move || e.export(doc, opts, path)).await
}

/// Taille estimée d'un export PDF (rien n'est écrit).
#[tauri::command]
#[specta::specta]
pub async fn export_estimate(
    engine: State<'_, Engine>,
    doc: DocId,
    opts: lunas_pdf_core::export::ExportOptions,
) -> Result<lunas_pdf_core::export::ExportResult> {
    let e = engine.inner().clone();
    blocking(move || e.export_estimate(doc, opts)).await
}

/// Imprime le document (dialogue d'impression du système) ; vrai si l'impression est lancée.
#[tauri::command]
#[specta::specta]
pub async fn print_document(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    engine: State<'_, Engine>,
    doc: DocId,
    name: String,
    pages: Vec<(f32, f32)>,
) -> Result<bool> {
    let e = engine.inner().clone();
    let (tx, rx) = std::sync::mpsc::channel();
    // GTK : sur le fil principal.
    app.run_on_main_thread(move || {
        let _ = tx.send(crate::print::print(&window, e, doc, name, pages));
    })
    .map_err(|e| Error::Engine(e.to_string()))?;
    blocking(move || rx.recv().map_err(|e| Error::Engine(e.to_string()))?).await
}

/// Enregistre les pages choisies dans un nouveau PDF.
#[tauri::command]
#[specta::specta]
pub async fn extract_pages(engine: State<'_, Engine>, doc: DocId, pages: Vec<u32>, path: String) -> Result<()> {
    let e = engine.inner().clone();
    blocking(move || e.extract_pages(doc, pages, path)).await
}

#[tauri::command]
#[specta::specta]
pub async fn undo(engine: State<'_, Engine>, doc: DocId) -> Result<EditState> {
    edit(&engine, doc, EditRequest::Undo).await
}

#[tauri::command]
#[specta::specta]
pub async fn redo(engine: State<'_, Engine>, doc: DocId) -> Result<EditState> {
    edit(&engine, doc, EditRequest::Redo).await
}

/// Masque une annotation pendant son édition en place (hors historique).
#[tauri::command]
#[specta::specta]
pub async fn set_annotation_hidden(engine: State<'_, Engine>, doc: DocId, id: String, hidden: bool) -> Result<EditState> {
    edit(&engine, doc, EditRequest::SetHidden { id, hidden }).await
}

/// Boîtes des lignes de texte entre deux points (outils de marquage).
#[tauri::command]
#[specta::specta]
pub async fn text_range(engine: State<'_, Engine>, doc: DocId, page: u32, from: Point, to: Point) -> Result<TextSelection> {
    let e = engine.inner().clone();
    blocking(move || e.text_range(doc, page, from, to)).await
}

#[tauri::command]
#[specta::specta]
pub async fn import_image(engine: State<'_, Engine>, doc: DocId, path: String) -> Result<ImageInfo> {
    let e = engine.inner().clone();
    blocking(move || e.import_image(doc, path)).await
}

/// Image PNG ou JPEG produite par l'interface (signature manuscrite).
#[tauri::command]
#[specta::specta]
pub async fn import_image_bytes(engine: State<'_, Engine>, doc: DocId, bytes: Vec<u8>) -> Result<ImageInfo> {
    let e = engine.inner().clone();
    blocking(move || e.import_image_bytes(doc, bytes)).await
}

/// Octets d'une image choisie par l'utilisateur (onglet « Importer » d'une signature).
#[tauri::command]
#[specta::specta]
pub async fn read_image_file(path: String) -> Result<Vec<u8>> {
    let ext = std::path::Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !["png", "jpg", "jpeg", "svg"].contains(&ext.as_str()) {
        return Err(Error::Invalid(format!("format non pris en charge : {ext}")));
    }
    blocking(move || {
        let meta = std::fs::metadata(&path).map_err(io)?;
        if meta.len() > 20 * 1024 * 1024 {
            return Err(Error::Invalid("image trop lourde (20 Mo au plus)".into()));
        }
        std::fs::read(&path).map_err(io)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub fn list_signatures(store: State<'_, Store>) -> Vec<SavedSignature> {
    store.signatures()
}

#[tauri::command]
#[specta::specta]
pub fn save_signature(store: State<'_, Store>, png: Vec<u8>, width: u32, height: u32) -> Result<SavedSignature> {
    store.add_signature(&png, width, height, now_ms()).map_err(io)
}

#[tauri::command]
#[specta::specta]
pub fn delete_signature(store: State<'_, Store>, id: String) -> Result<()> {
    store.remove_signature(&id).map_err(io)
}

/// Copie une image importée d'un document à l'autre (collage d'un tampon image).
#[tauri::command]
#[specta::specta]
pub async fn copy_image(engine: State<'_, Engine>, from: DocId, to: DocId, key: String) -> Result<()> {
    let e = engine.inner().clone();
    blocking(move || e.copy_image(from, to, key)).await
}

/// Enregistre le document (sur place, ou sous `path`). Écriture atomique ; l'original n'est
/// jamais touché avant cet appel.
#[tauri::command]
#[specta::specta]
pub async fn save_document(
    engine: State<'_, Engine>,
    store: State<'_, Store>,
    doc: DocId,
    path: Option<String>,
) -> Result<DocInfo> {
    let e = engine.inner().clone();
    let info = blocking(move || e.save(doc, path.map(std::path::PathBuf::from))).await?;
    store.bump_recent(&info.path, now_ms()).map_err(io)?;
    Ok(info)
}

/// Signatures numériques du document et résultat de leur vérification.
#[tauri::command]
#[specta::specta]
pub async fn get_signatures(
    engine: State<'_, Engine>,
    store: State<'_, Store>,
    doc: DocId,
) -> Result<Vec<lunas_pdf_core::SignatureInfo>> {
    let e = engine.inner().clone();
    let trusted = store.trusted_der();
    blocking(move || e.signatures(doc, &trusted)).await
}

/// Approuve l'autorité en haut de la chaîne de la signature `field`.
#[tauri::command]
#[specta::specta]
pub async fn trust_signature_root(
    engine: State<'_, Engine>,
    store: State<'_, Store>,
    doc: DocId,
    field: String,
) -> Result<TrustedRoot> {
    let e = engine.inner().clone();
    let (der, name) = blocking(move || {
        let (bytes, password) = e.saved_file(doc)?;
        lunas_pdf_core::signature::chain_root(&bytes, password.as_deref(), &field)
            .ok_or_else(|| Error::Invalid("certificat introuvable".into()))
    })
    .await?;
    store.add_trusted(&der, name, now_ms()).map_err(io)
}

/// État des listes de confiance (UE, Microsoft) utilisées pour vérifier les signatures.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrustListsInfo {
    /// Date de génération (millisecondes Unix).
    pub generated: f64,
    pub eu: u32,
    pub microsoft: u32,
}

#[tauri::command]
#[specta::specta]
pub fn trust_lists_info() -> TrustListsInfo {
    let b = lunas_pdf_core::trust_lists::current();
    let eu = b.anchors.iter().filter(|a| a.source.starts_with("eu:")).count() as u32;
    TrustListsInfo {
        generated: b.generated as f64 * 1000.0,
        eu,
        microsoft: b.anchors.len() as u32 - eu,
    }
}

#[tauri::command]
#[specta::specta]
pub fn list_trusted_roots(store: State<'_, Store>) -> Vec<TrustedRoot> {
    store.trusted_roots()
}

#[tauri::command]
#[specta::specta]
pub fn remove_trusted_root(store: State<'_, Store>, id: String) -> Result<()> {
    store.remove_trusted(&id).map_err(io)
}

#[tauri::command]
#[specta::specta]
pub async fn get_links(engine: State<'_, Engine>, doc: DocId, page: u32) -> Result<Vec<LinkInfo>> {
    let e = engine.inner().clone();
    blocking(move || e.links(doc, page)).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_page_text(engine: State<'_, Engine>, doc: DocId, page: u32) -> Result<PageText> {
    let e = engine.inner().clone();
    blocking(move || e.text(doc, page)).await
}

/// Recherche plein texte ; les résultats arrivent page par page sur `on_event`.
#[tauri::command]
#[specta::specta]
pub fn search(
    engine: State<'_, Engine>,
    doc: DocId,
    search_id: u32,
    query: String,
    case_sensitive: bool,
    on_event: Channel<SearchEvent>,
) {
    engine.search(
        doc,
        search_id,
        query,
        case_sensitive,
        Box::new(move |ev| {
            let _ = on_event.send(ev);
        }),
    );
}

#[tauri::command]
#[specta::specta]
pub fn cancel_search(engine: State<'_, Engine>, doc: DocId) {
    engine.cancel_search(doc);
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(store: State<'_, Store>) -> Settings {
    store.settings.lock().unwrap().clone()
}

#[tauri::command]
#[specta::specta]
pub fn set_settings(engine: State<'_, Engine>, store: State<'_, Store>, settings: Settings) -> Result<()> {
    engine.set_author(author_name(&settings));
    store.save_settings(settings).map_err(io)
}

/// Auteur des annotations : réglage, sinon nom de l'utilisateur du système.
pub fn author_name(s: &Settings) -> String {
    let n = s.author_name.trim();
    if !n.is_empty() {
        return n.to_string();
    }
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "Lunas PDF".into())
}

#[tauri::command]
#[specta::specta]
pub fn get_recents(store: State<'_, Store>) -> Vec<RecentDoc> {
    store.recents.lock().unwrap().clone()
}

#[tauri::command]
#[specta::specta]
pub fn remove_recent(store: State<'_, Store>, path: String) -> Result<()> {
    store.remove_recent(&path).map_err(io)
}

#[tauri::command]
#[specta::specta]
pub fn clear_recents(store: State<'_, Store>) -> Result<()> {
    store.clear_recents().map_err(io)
}

/// Langue de l'OS (ex. « fr-FR »).
#[tauri::command]
#[specta::specta]
pub fn system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "fr".into())
}

/// Fichiers passés en ligne de commande au lancement (vidé à la lecture).
#[tauri::command]
#[specta::specta]
pub fn take_pending_files(pending: State<'_, PendingFiles>) -> Vec<String> {
    std::mem::take(&mut *pending.0.lock().unwrap())
}

/// Erreurs JavaScript de l'interface, recopiées dans le terminal (diagnostic sous WebKitGTK,
/// dont la console n'est pas visible).
#[tauri::command]
#[specta::specta]
pub fn log_frontend_error(message: String) {
    eprintln!("[lunas-pdf] interface : {message}");
}

// --- IA ------------------------------------------------------------------------------------

/// Installation de l'agent (préférences).
#[tauri::command]
#[specta::specta]
pub async fn ai_detect(
    ai: State<'_, crate::ai::AiService>,
    agent: crate::ai::agent::Agent,
) -> std::result::Result<crate::ai::Detected, crate::ai::AiError> {
    Ok(ai.detect(agent).await)
}

/// Lance une demande ; le texte arrive par `AiChunkEvent`, le résultat complet à la fin.
#[tauri::command]
#[specta::specta]
pub async fn ai_run(
    ai: State<'_, crate::ai::AiService>,
    request_id: String,
    task: crate::ai::AiTask,
) -> std::result::Result<String, crate::ai::AiError> {
    ai.run(request_id, task).await
}

#[tauri::command]
#[specta::specta]
pub fn ai_cancel(ai: State<'_, crate::ai::AiService>, request_id: String) {
    ai.cancel(&request_id);
}

/// Informations mémorisées pour remplir les documents (préférences, panneau IA).
#[tauri::command]
#[specta::specta]
pub fn get_ai_memory(store: State<'_, Store>) -> Vec<String> {
    store.memory()
}

/// Remplace les informations mémorisées (doublons et lignes vides retirés).
#[tauri::command]
#[specta::specta]
pub fn set_ai_memory(store: State<'_, Store>, facts: Vec<String>) -> Result<Vec<String>> {
    store.set_memory(facts).map_err(io)
}

/// Thème d'Omarchy (Linux), s'il est installé.
#[tauri::command]
#[specta::specta]
pub fn get_omarchy_theme() -> Option<crate::omarchy::OmarchyTheme> {
    crate::omarchy::read()
}
