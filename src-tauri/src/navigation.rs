//! La fenêtre ne quitte jamais l'interface : un lien externe activé d'une façon ou d'une autre
//! (clic du milieu, Ctrl clic, lien glissé, `<a href>` non intercepté…) s'ouvre dans le
//! navigateur par défaut au lieu de remplacer l'application.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime, Url};
use tauri_plugin_opener::OpenerExt;

/// Adresse de l'application elle-même (interface, IPC, protocole de rendu, serveur de dev).
pub fn is_internal(url: &Url) -> bool {
    match url.scheme() {
        "tauri" | "ipc" | "lunas-pdf" | "about" | "data" | "blob" => true,
        "http" | "https" => url.host_str().is_some_and(|h| h == "localhost" || h.ends_with(".localhost")),
        _ => false,
    }
}

/// Lien ouvert dans l'application associée (navigateur, messagerie).
fn opens_externally(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "mailto")
}

pub fn guard<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navigation-guard")
        .on_navigation(|webview, url| {
            if is_internal(url) {
                return true;
            }
            if opens_externally(url)
                && let Err(e) = webview.app_handle().opener().open_url(url.as_str(), None::<&str>)
            {
                eprintln!("[lunas-pdf] lien non ouvert ({url}) : {e}");
            }
            false
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn internal_and_external_addresses() {
        for s in [
            "tauri://localhost/index.html",
            "http://localhost:1420/",
            "http://tauri.localhost/",
            "http://ipc.localhost/cmd",
            "lunas-pdf://localhost/render",
            "about:blank",
        ] {
            assert!(is_internal(&u(s)), "{s}");
        }
        for s in [
            "https://www.example.com/doc",
            "http://evil.localhost.example.com/",
            "mailto:a@b.fr",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(!is_internal(&u(s)), "{s}");
        }
        assert!(opens_externally(&u("https://www.example.com")) && opens_externally(&u("mailto:a@b.fr")));
        assert!(!opens_externally(&u("file:///etc/passwd")));
    }
}
