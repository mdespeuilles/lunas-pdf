//! Tests d'intégration sur les fixtures (nécessitent libpdfium : `bun pdfium`).

use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::channel;
use std::time::Duration;

use feuillet_core::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("fixtures").join(name)
}

/// PDFium ne peut être initialisé qu'une fois par processus : moteur partagé.
fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| {
        let dir = std::env::var_os("FEUILLET_PDFIUM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root().join("src-tauri/pdfium/lib"));
        Engine::start(Some(&dir)).expect("libpdfium : lancer `bun pdfium`")
    })
}

fn req(doc: DocId, page: u32, width: u32, height: u32) -> RenderRequest {
    RenderRequest {
        doc,
        page,
        width,
        height,
        tile: None,
        priority: 10,
        epoch: 0,
    }
}

#[test]
fn opens_long_document_with_outline() {
    let e = engine();
    let info = e.open(fixture("long-320-pages.pdf"), None).unwrap();
    assert_eq!(info.pages.len(), 320);
    assert_eq!(info.title.as_deref(), Some("Document long avec signets"));
    assert!((info.pages[0].width - 595.27).abs() < 0.1);
    let outline = e.outline(info.id).unwrap();
    assert_eq!(outline.len(), 32, "32 chapitres");
    assert_eq!(outline[0].children.len(), 1, "une section par chapitre");
    assert_eq!(outline[1].page, Some(10));
    assert_eq!(outline[1].children[0].page, Some(15));
    e.close(info.id);
}

#[test]
fn renders_full_page_and_tile_consistently() {
    let e = engine();
    let info = e.open(fixture("texte-simple.pdf"), None).unwrap();
    let full = e.render(req(info.id, 0, 600, 849)).unwrap();
    assert_eq!((full.width, full.height), (600, 849));
    assert_eq!(full.rgba.len(), 600 * 849 * 4);
    assert!(full.rgba.chunks(4).any(|p| p[0] < 100), "la page contient du texte sombre");
    assert!(full.rgba.chunks(4).all(|p| p[3] == 255), "fond opaque");

    // Une tuile doit être identique à la même zone de la page complète.
    let tile = Tile {
        x: 50,
        y: 100,
        w: 200,
        h: 120,
    };
    let t = e
        .render(RenderRequest {
            tile: Some(tile),
            ..req(info.id, 0, 600, 849)
        })
        .unwrap();
    assert_eq!((t.width, t.height), (200, 120));
    let mut diff = 0u64;
    for y in 0..tile.h {
        for x in 0..tile.w {
            let a = ((y * tile.w + x) * 4) as usize;
            let b = (((y + tile.y) * 600 + x + tile.x) * 4) as usize;
            diff += (t.rgba[a] as i32 - full.rgba[b] as i32).unsigned_abs() as u64;
        }
    }
    assert!(diff < 2000, "écart tuile/page : {diff}");
    e.close(info.id);
}

#[test]
fn text_runs_and_geometry() {
    let e = engine();
    let info = e.open(fixture("texte-simple.pdf"), None).unwrap();
    let text = e.text(info.id, 0).unwrap();
    let first = &text.runs[0];
    assert_eq!(first.text.trim(), "Page");
    // Titre dessiné à 72 pt du bord gauche, ligne de base à 90 pt du haut.
    assert!((first.rect.x - 72.0).abs() < 2.0, "{:?}", first.rect);
    assert!(first.rect.y > 60.0 && first.rect.y < 90.0, "{:?}", first.rect);
    let joined: String = text.runs.iter().map(|r| r.text.as_str()).collect();
    assert!(joined.contains("Ligne 30 : Le vif renard brun saute par-dessus le chien paresseux."));
    assert_eq!(text.runs.iter().filter(|r| r.eol).count(), 31, "titre + 30 lignes");
}

#[test]
fn search_streams_results_per_page() {
    let e = engine();
    let info = e.open(fixture("texte-simple.pdf"), None).unwrap();
    let (tx, rx) = channel();
    e.search(info.id, 7, "LIGNE 1".into(), false, Box::new(move |ev| tx.send(ev).unwrap()));
    let mut pages = 0;
    let total = loop {
        match rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            SearchEvent::Page { search_id, hits, .. } => {
                assert_eq!(search_id, 7);
                pages += 1;
                for h in &hits {
                    assert_eq!(h.matched.to_lowercase(), "ligne 1");
                    assert_eq!(h.rects.len(), 1);
                }
            }
            SearchEvent::Done { total, .. } => break total,
        }
    };
    assert_eq!(pages, 3);
    // « Ligne 1 », « Ligne 10 » … « Ligne 19 » : 11 par page.
    assert_eq!(total, 33);

    let (tx, rx) = channel();
    e.search(info.id, 8, "LIGNE 1".into(), true, Box::new(move |ev| tx.send(ev).unwrap()));
    let total = loop {
        if let SearchEvent::Done { total, .. } = rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            break total;
        }
    };
    assert_eq!(total, 0, "respect de la casse");
}

#[test]
fn encrypted_document_requires_password() {
    let e = engine();
    let path = fixture("chiffre-aes256.pdf");
    assert_eq!(e.open(&path, None).unwrap_err(), Error::PasswordRequired);
    assert_eq!(e.open(&path, Some("mauvais".into())).unwrap_err(), Error::WrongPassword);
    let info = e.open(&path, Some("feuillet".into())).unwrap();
    assert!(info.encrypted);
    assert_eq!(info.pages.len(), 3);
    assert!(e.text(info.id, 0).unwrap().runs.len() > 10);
}

#[test]
fn lists_annotations_and_links() {
    let e = engine();
    let info = e.open(fixture("avec-annotations.pdf"), None).unwrap();
    let annots = e.annotations(info.id).unwrap();
    assert_eq!(annots.len(), 1, "les liens ne sont pas des annotations listées");
    assert!(matches!(annots[0].body, AnnotBody::Square));
    assert_eq!(annots[0].contents.as_deref(), Some("Carré existant"));
    let links = e.links(info.id, 0).unwrap();
    assert_eq!(links.len(), 2);
    assert!(
        links
            .iter()
            .any(|l| matches!(&l.target, LinkTarget::Uri { uri } if uri == "https://example.org"))
    );
    assert!(links.iter().any(|l| matches!(l.target, LinkTarget::Page { page: 2 })));
}

#[test]
fn detects_forms_and_signatures() {
    let e = engine();
    let form = e.open(fixture("formulaire-acroform.pdf"), None).unwrap();
    assert_eq!(form.form, FormKind::AcroForm);
    let signed = e.open(fixture("signe-valide.pdf"), None).unwrap();
    assert_eq!(signed.signature_count, 1);
    let plain = e.open(fixture("texte-simple.pdf"), None).unwrap();
    assert_eq!((plain.form, plain.signature_count), (FormKind::None, 0));
}

#[test]
fn stale_renders_are_cancelled() {
    let e = engine();
    let info = e.open(fixture("scan-lourd.pdf"), None).unwrap();
    // Remplit la file, puis change d'époque : les demandes en attente doivent être abandonnées.
    let handles: Vec<_> = (0..8)
        .map(|p| {
            let e = e.clone();
            std::thread::spawn(move || {
                e.render(RenderRequest {
                    epoch: 0,
                    ..req(info.id, p, 1200, 1700)
                })
            })
        })
        .collect();
    std::thread::sleep(Duration::from_millis(30));
    e.set_epoch(info.id, 1);
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(
        results.iter().any(|r| matches!(r, Err(Error::Cancelled))),
        "au moins une demande annulée"
    );
    assert!(
        e.render(RenderRequest {
            epoch: 1,
            ..req(info.id, 0, 300, 425)
        })
        .is_ok()
    );
}

#[test]
fn png_thumbnail() {
    let e = engine();
    let info = e.open(fixture("texte-simple.pdf"), None).unwrap();
    let png = e.png(info.id, 0, 240).unwrap();
    assert_eq!(&png[1..4], b"PNG");
}

#[test]
fn missing_and_invalid_files() {
    let e = engine();
    assert!(matches!(e.open("/nonexistent.pdf", None), Err(Error::NotFound(_))));
    assert!(matches!(e.open(root().join("Cargo.toml"), None), Err(Error::Invalid(_))));
}
