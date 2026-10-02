//! Organisation des pages : ordre, rotation, suppression, duplication, page blanche, copie
//! entre documents, extraction, annulation, documents chiffrés et signés.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use feuillet_core::pages::PageOp;
use feuillet_core::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| {
        let dir = std::env::var_os("FEUILLET_PDFIUM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root().join("src-tauri/pdfium/lib"));
        Engine::start(Some(&dir)).expect("libpdfium : lancer `pnpm pdfium`")
    })
}

fn scratch(name: &str, test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("feuillet-pages-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(name);
    std::fs::copy(root().join("fixtures").join(name), &dst).unwrap();
    dst
}

fn qpdf_check(path: &Path, password: Option<&str>) {
    let mut cmd = Command::new("qpdf");
    cmd.arg("--check");
    if let Some(p) = password {
        cmd.arg(format!("--password={p}"));
    }
    match cmd.arg(path).output() {
        Ok(out) => assert!(
            out.status.success(),
            "qpdf --check {}:\n{}",
            path.display(),
            String::from_utf8_lossy(&out.stdout)
        ),
        Err(_) => eprintln!("qpdf absent : vérification ignorée"),
    }
}

/// Premier mot significatif de chaque page (« Page 12 », « Article »…) pour suivre l'ordre.
fn page_text(doc: DocId, page: u32) -> String {
    engine()
        .text(doc, page)
        .unwrap()
        .runs
        .iter()
        .map(|r| r.text.as_str())
        .collect::<String>()
}

fn pages_op(doc: DocId, op: PageOp) -> EditState {
    engine().edit(doc, EditRequest::Pages(op)).unwrap()
}

#[test]
fn reorder_rotate_delete_duplicate_blank_and_undo() {
    let e = engine();
    let path = scratch("long-320-pages.pdf", "basic");
    let info = e.open(&path, None).unwrap();
    let doc = info.id;
    let t0 = page_text(doc, 0);
    let t1 = page_text(doc, 1);
    let t2 = page_text(doc, 2);
    assert_ne!(t0, t1);

    // Déplacer la page 1 en tête.
    let st = pages_op(doc, PageOp::Move { pages: vec![1], to: 0 });
    let geoms = st.pages.expect("géométrie renvoyée");
    assert_eq!(geoms.len(), 320);
    assert!(st.dirty && st.can_undo);
    assert_eq!((page_text(doc, 0), page_text(doc, 1)), (t1.clone(), t0.clone()));

    // Rotation : la page affichée devient paysage.
    let (w, h) = (geoms[0].width, geoms[0].height);
    let st = pages_op(
        doc,
        PageOp::Rotate {
            pages: vec![0],
            delta: 90,
        },
    );
    let g = st.pages.unwrap();
    assert_eq!((g[0].width.round(), g[0].height.round()), (h.round(), w.round()));

    // Supprimer, dupliquer, page blanche.
    let st = pages_op(doc, PageOp::Delete { pages: vec![2] });
    assert_eq!(st.pages.unwrap().len(), 319);
    let st = pages_op(doc, PageOp::Duplicate { pages: vec![1] });
    assert_eq!(st.pages.unwrap().len(), 320);
    assert_eq!(page_text(doc, 2), t0, "la copie suit l'original");
    let st = pages_op(doc, PageOp::InsertBlank { at: 3 });
    assert_eq!(st.pages.unwrap().len(), 321);
    assert_eq!(page_text(doc, 3), "");
    assert_eq!(st.unsaved_count, 5);

    // Tout annuler, pas à pas.
    for _ in 0..5 {
        e.edit(doc, EditRequest::Undo).unwrap();
    }
    let st = e.edit(doc, EditRequest::Load).unwrap();
    assert!(!st.dirty && !st.can_undo && st.can_redo);
    assert_eq!(
        (page_text(doc, 0), page_text(doc, 1), page_text(doc, 2)),
        (t0.clone(), t1.clone(), t2)
    );
    let st = e.edit(doc, EditRequest::Redo).unwrap();
    assert_eq!(st.pages.unwrap().len(), 320);
    assert_eq!(page_text(doc, 0), t1);

    // Impossible de supprimer toutes les pages.
    assert!(
        e.edit(
            doc,
            EditRequest::Pages(PageOp::Delete {
                pages: (0..320).collect()
            })
        )
        .is_err()
    );

    e.save(doc, None).unwrap();
    qpdf_check(&path, None);
    let again = e.open(&path, None).unwrap();
    assert_eq!(again.pages.len(), 320);
    assert_eq!(page_text(again.id, 0), t1);
}

#[test]
fn annotations_follow_their_page_and_undo_crosses_page_ops() {
    let e = engine();
    let path = scratch("texte-simple.pdf", "annots");
    let info = e.open(&path, None).unwrap();
    let doc = info.id;
    let square = Annot {
        id: "carre".into(),
        page: 0,
        rect: Rect {
            x: 100.0,
            y: 100.0,
            w: 50.0,
            h: 40.0,
        },
        color: "#e03131".into(),
        opacity: 1.0,
        width: 2.0,
        contents: None,
        author: None,
        modified: None,
        excerpt: None,
        body: AnnotBody::Square,
        hidden: false,
    };
    e.edit(
        doc,
        EditRequest::Apply(vec![AnnotOp::Add {
            annot: square,
            index: None,
        }]),
    )
    .unwrap();
    let st = pages_op(doc, PageOp::Move { pages: vec![0], to: 3 });
    let a = st.annots.iter().find(|a| a.id == "carre").expect("annotation conservée");
    assert_eq!(a.page, 2);
    assert!((a.rect.x - 100.0).abs() < 0.5);
    // Annuler le déplacement puis l'ajout : on revient au document d'origine.
    let st = e.edit(doc, EditRequest::Undo).unwrap();
    assert_eq!(st.annots.iter().find(|a| a.id == "carre").unwrap().page, 0);
    let st = e.edit(doc, EditRequest::Undo).unwrap();
    assert!(st.annots.iter().all(|a| a.id != "carre"));
    assert!(!st.dirty);
    e.edit(doc, EditRequest::Redo).unwrap();
    e.edit(doc, EditRequest::Redo).unwrap();
    e.save(doc, None).unwrap();
    qpdf_check(&path, None);
    let again = e.open(&path, None).unwrap();
    let annots = e.annotations(again.id).unwrap();
    assert_eq!(annots.iter().find(|a| a.id == "carre").unwrap().page, 2);
}

#[test]
fn import_pages_with_fields_and_bookmarks() {
    let e = engine();
    let path = scratch("texte-simple.pdf", "import");
    let dst = e.open(&path, None).unwrap();
    let form = e.open(root().join("fixtures/formulaire-acroform.pdf"), None).unwrap();
    let long = e.open(root().join("fixtures/long-320-pages.pdf"), None).unwrap();

    // Formulaire inséré en page 2 : ses champs deviennent ceux du document.
    let st = pages_op(
        dst.id,
        PageOp::Import {
            from: form.id,
            pages: vec![0],
            at: 1,
        },
    );
    assert_eq!(st.pages.unwrap().len(), 4);
    assert_eq!(st.fields.len(), 8);
    assert!(st.fields.iter().all(|f| f.widgets.iter().all(|w| w.page == 1)));
    // Une seconde copie du même formulaire : noms rendus uniques.
    let st = pages_op(
        dst.id,
        PageOp::Import {
            from: form.id,
            pages: vec![0],
            at: 4,
        },
    );
    assert_eq!(st.fields.len(), 16);
    assert!(st.fields.iter().any(|f| f.id == "nom_2"));

    // Pages 10 à 12 du long document : signets « Chapitre 2 » repris.
    let st = pages_op(
        dst.id,
        PageOp::Import {
            from: long.id,
            pages: vec![10, 11, 12],
            at: 5,
        },
    );
    assert_eq!(st.pages.unwrap().len(), 8);
    let outline = e.outline(dst.id).unwrap();
    let titles: Vec<String> = flat(&outline).into_iter().map(|(t, _)| t).collect();
    assert!(titles.iter().any(|t| t == "Chapitre 2"), "{titles:?}");
    assert_eq!(flat(&outline).into_iter().find(|(t, _)| t == "Chapitre 2").unwrap().1, 5);
    assert_eq!(page_text(dst.id, 5), page_text(long.id, 10));

    e.save(dst.id, None).unwrap();
    qpdf_check(&path, None);
    let again = e.open(&path, None).unwrap();
    assert_eq!(again.pages.len(), 8);
    let f = e.edit(again.id, EditRequest::Load).unwrap().fields;
    assert_eq!(f.len(), 16);
}

fn flat(items: &[OutlineItem]) -> Vec<(String, u32)> {
    items
        .iter()
        .flat_map(|i| std::iter::once((i.title.clone(), i.page.unwrap_or(u32::MAX))).chain(flat(&i.children)))
        .collect()
}

#[test]
fn extract_pages_to_new_file() {
    let e = engine();
    let long = e.open(root().join("fixtures/long-320-pages.pdf"), None).unwrap();
    let out = std::env::temp_dir().join(format!("feuillet-pages-{}-extrait.pdf", std::process::id()));
    e.extract_pages(long.id, vec![20, 21], &out).unwrap();
    qpdf_check(&out, None);
    let x = e.open(&out, None).unwrap();
    assert_eq!(x.pages.len(), 2);
    assert_eq!(page_text(x.id, 1), page_text(long.id, 21));
    let titles: Vec<String> = flat(&e.outline(x.id).unwrap()).into_iter().map(|(t, _)| t).collect();
    assert!(titles.iter().any(|t| t == "Chapitre 3"), "{titles:?}");
}

#[test]
fn encrypted_and_signed_documents() {
    let e = engine();
    let path = scratch("chiffre-aes256.pdf", "chiffre");
    let info = e.open(&path, Some("feuillet".into())).unwrap();
    let w = info.pages[0].width;
    pages_op(
        info.id,
        PageOp::Rotate {
            pages: vec![0],
            delta: -90,
        },
    );
    e.save(info.id, None).unwrap();
    qpdf_check(&path, Some("feuillet"));
    let again = e.open(&path, Some("feuillet".into())).unwrap();
    assert_eq!(again.pages[0].height.round(), w.round());

    // Document signé : la révision ajoutée laisse la partie signée intacte.
    let path = scratch("signe-valide.pdf", "signe");
    let info = e.open(&path, None).unwrap();
    pages_op(info.id, PageOp::Move { pages: vec![2], to: 0 });
    e.save(info.id, None).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let s = &signature::verify_document(&bytes, None, &signature::TrustStore::default())[0];
    assert!(s.intact && !s.covers_whole);
}

#[test]
fn page_clipboard_copy_then_paste_elsewhere() {
    let e = engine();
    let src_path = scratch("long-320-pages.pdf", "clip");
    let src = e.open(&src_path, None).unwrap();
    let clip = e.clip_pages(src.id, vec![5, 6]).unwrap();
    assert_eq!(clip.pages.len(), 2);
    // Coupe : la source perd ses pages, la copie reste collable.
    pages_op(src.id, PageOp::Delete { pages: vec![5, 6] });
    let path = scratch("texte-simple.pdf", "clip");
    let dst = e.open(&path, None).unwrap();
    let st = pages_op(
        dst.id,
        PageOp::Import {
            from: clip.id,
            pages: vec![0, 1],
            at: 3,
        },
    );
    assert_eq!(st.pages.unwrap().len(), 5);
    assert_eq!(page_text(dst.id, 4), page_text(clip.id, 1));
    e.close(clip.id);
    e.save(dst.id, None).unwrap();
    qpdf_check(&path, None);
}
