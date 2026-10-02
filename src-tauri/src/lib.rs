mod commands;
mod protocol;
mod store;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use feuillet_core::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::Manager;
use tauri_specta::{Event, collect_commands, collect_events};

use commands::PendingFiles;
use store::Store;

/// Fichiers à ouvrir reçus pendant l'exécution (seconde instance, association de fichiers).
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct OpenFilesEvent {
    pub paths: Vec<String>,
}

pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::open_document,
            commands::close_document,
            commands::set_render_epoch,
            commands::get_outline,
            commands::load_annotations,
            commands::apply_annotations,
            commands::undo,
            commands::redo,
            commands::set_annotation_hidden,
            commands::text_range,
            commands::import_image,
            commands::copy_image,
            commands::import_image_bytes,
            commands::read_image_file,
            commands::list_signatures,
            commands::save_signature,
            commands::delete_signature,
            commands::save_document,
            commands::get_links,
            commands::get_signatures,
            commands::trust_signature_root,
            commands::list_trusted_roots,
            commands::remove_trusted_root,
            commands::get_page_text,
            commands::search,
            commands::cancel_search,
            commands::get_settings,
            commands::set_settings,
            commands::get_recents,
            commands::remove_recent,
            commands::clear_recents,
            commands::system_locale,
            commands::take_pending_files,
            commands::log_frontend_error,
        ])
        .events(collect_events![OpenFilesEvent])
        // Les flottants transmis (géométrie) ne sont jamais NaN : `number` plutôt que `number | null`.
        .semantic_types(specta_typescript::semantic::Configuration::empty().enable_lossless_floats())
}

/// Chemins de fichiers existants parmi des arguments de ligne de commande.
fn file_args(args: impl IntoIterator<Item = String>, cwd: &Path) -> Vec<String> {
    args.into_iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| {
            let p = PathBuf::from(a.strip_prefix("file://").unwrap_or(&a));
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .filter(|p| p.is_file())
        .map(|p| p.canonicalize().unwrap_or(p).display().to_string())
        .collect()
}

/// Dossier de libpdfium : variable d'environnement, ressources de l'app, puis dossier de dev.
fn pdfium_dir(app: &tauri::App) -> Option<PathBuf> {
    let lib = |d: PathBuf| d.join(Engine::platform_library_name()).is_file().then_some(d);
    std::env::var_os("FEUILLET_PDFIUM_DIR")
        .map(PathBuf::from)
        .and_then(lib)
        .or_else(|| app.path().resource_dir().ok().map(|d| d.join("pdfium")).and_then(lib))
        .or_else(|| lib(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdfium/lib")))
        .or_else(|| lib(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pdfium/bin")))
}

/// WebKitGTK plante au démarrage sous Wayland avec le pilote NVIDIA propriétaire
/// (« Error 71 (Protocol error) dispatching to Wayland display ») : on désactive le rendu
/// DMA-BUF dans ce cas (voir ADR-001 § 6).
#[cfg(target_os = "linux")]
fn linux_webkit_workarounds() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() && Path::new("/proc/driver/nvidia/version").exists() {
        // SAFETY : appelé au démarrage, avant la création de tout autre fil.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

pub fn run() {
    #[cfg(target_os = "linux")]
    linux_webkit_workarounds();

    let builder = specta_builder();
    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default().header("// Généré par tauri-specta — ne pas modifier.\n// @ts-nocheck"),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"),
        )
        .expect("export des types TypeScript");

    let cwd = std::env::current_dir().unwrap_or_default();
    let initial = file_args(std::env::args().skip(1), &cwd);

    let mut builder_app = tauri::Builder::default();
    // Instance unique : seulement en version publiée. En développement, une instance déjà
    // ouverte (version installée, autre `tauri dev`) capturerait le lancement et la nouvelle
    // instance se fermerait sans rien afficher. FEUILLET_NO_SINGLE_INSTANCE la désactive aussi.
    if !cfg!(debug_assertions) && std::env::var_os("FEUILLET_NO_SINGLE_INSTANCE").is_none() {
        builder_app = builder_app.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            let paths = file_args(argv.into_iter().skip(1), Path::new(&cwd));
            if !paths.is_empty() {
                let _ = OpenFilesEvent { paths }.emit(app);
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }));
    }
    builder_app
        // La fenêtre est créée invisible (pas de flash blanc avant le thème) et affichée par
        // l'interface ; filet de sécurité : on l'affiche de toute façon une fois la page chargée,
        // pour qu'une erreur au démarrage de l'interface ne laisse jamais l'app sans fenêtre.
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let w = webview.window();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    if !w.is_visible().unwrap_or(true) {
                        eprintln!("[feuillet] l'interface n'a pas affiché la fenêtre : affichage forcé");
                        let _ = w.show();
                    }
                });
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(PendingFiles(Mutex::new(initial)))
        .invoke_handler(builder.invoke_handler())
        .register_asynchronous_uri_scheme_protocol("feuillet", protocol::handle)
        .setup(move |app| {
            builder.mount_events(app);
            let dir = pdfium_dir(app);
            let engine = Engine::start(dir.as_deref()).map_err(|e| e.to_string())?;
            let paths = app.path();
            let store = Store::load(paths.app_config_dir()?, paths.app_cache_dir()?);
            engine.set_author(commands::author_name(&store.settings.lock().unwrap()));
            app.manage(engine);
            app.manage(store);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("erreur au démarrage de Feuillet")
        .run(|app, event| {
            // macOS : ouverture via le Finder ou l'association de fichiers.
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let tauri::RunEvent::Opened { urls } = &event {
                let paths: Vec<String> = urls
                    .iter()
                    .filter_map(|u| u.to_file_path().ok())
                    .map(|p| p.display().to_string())
                    .collect();
                if !paths.is_empty() {
                    app.state::<PendingFiles>().0.lock().unwrap().extend(paths.clone());
                    let _ = OpenFilesEvent { paths }.emit(app);
                }
            }
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bindings() {
        // Garantit que les types exportés restent valides (exécuté aussi en CI).
        specta_builder()
            .export(
                specta_typescript::Typescript::default().header("// Généré par tauri-specta — ne pas modifier.\n// @ts-nocheck"),
                concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"),
            )
            .unwrap();
    }

    #[test]
    fn file_args_resolves_relative_paths_and_skips_flags() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let got = file_args(
            ["--verbose".into(), "fixtures/texte-simple.pdf".into(), "absent.pdf".into()],
            &root,
        );
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("fixtures/texte-simple.pdf"));
    }
}
