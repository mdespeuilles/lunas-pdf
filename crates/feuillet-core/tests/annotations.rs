//! Annotations : écriture, relecture, rendu, enregistrement incrémental (phase 2).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

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
        let e = Engine::start(Some(&dir)).expect("libpdfium : lancer `pnpm pdfium`");
        e.set_author("Testeur".into());
        e
    })
}

/// Copie un fixture dans un dossier temporaire propre au test.
fn scratch(name: &str, test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("feuillet-annot-{}-{test}", std::process::id()));
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

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

fn annot(id: &str, page: u32, rect: Rect, color: &str, body: AnnotBody) -> Annot {
    Annot {
        id: id.into(),
        page,
        rect,
        color: color.into(),
        opacity: 1.0,
        width: 2.0,
        contents: None,
        author: None,
        modified: None,
        excerpt: None,
        body,
        hidden: false,
    }
}

fn add(a: Annot) -> AnnotOp {
    AnnotOp::Add { annot: a, index: None }
}

/// Rendu 1 px = 1 pt de la page.
fn render(doc: DocId, page: u32, info: &DocInfo) -> (u32, u32, Vec<u8>) {
    let g = &info.pages[page as usize];
    let (w, h) = (g.width.round() as u32, g.height.round() as u32);
    let b = engine()
        .render(RenderRequest {
            doc,
            page,
            width: w,
            height: h,
            tile: None,
            priority: 10,
            epoch: u32::MAX,
        })
        .unwrap();
    (w, h, b.rgba)
}

fn px(img: &(u32, u32, Vec<u8>), x: f32, y: f32) -> [u8; 3] {
    let i = ((y.round() as u32 * img.0 + x.round() as u32) * 4) as usize;
    [img.2[i], img.2[i + 1], img.2[i + 2]]
}

fn is_red(c: [u8; 3]) -> bool {
    c[0] > 180 && c[1] < 90 && c[2] < 90
}

fn tiny_png(path: &Path) {
    let img = image::RgbaImage::from_fn(40, 20, |x, _| {
        if x < 20 {
            image::Rgba([0, 0, 255, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    img.save(path).unwrap();
}

#[test]
fn all_types_round_trip_and_incremental_save() {
    let e = engine();
    let path = scratch("texte-simple.pdf", "types");
    let original = std::fs::read(&path).unwrap();
    let info = e.open(&path, None).unwrap();
    let doc = info.id;

    // Sélection de « Ligne 1 : … » par les points de la première ligne.
    let line1 = e
        .text_range(doc, 0, Point { x: 73.0, y: 125.0 }, Point { x: 300.0, y: 125.0 })
        .unwrap();
    assert!(line1.text.starts_with("Ligne 1 : Le vif renard"), "{}", line1.text);
    let line2 = e
        .text_range(doc, 0, Point { x: 73.0, y: 143.0 }, Point { x: 200.0, y: 143.0 })
        .unwrap();
    let png = path.with_file_name("tampon.png");
    tiny_png(&png);
    let img = e.import_image(doc, &png).unwrap();
    assert_eq!((img.width, img.height), (40, 20));

    let ops = vec![
        add(annot(
            "hl",
            0,
            line1.rects[0],
            "#ffd43b",
            AnnotBody::Highlight {
                quads: line1.rects.clone(),
            },
        )),
        add(annot(
            "ul",
            0,
            line2.rects[0],
            "#1f6fe0",
            AnnotBody::Underline {
                quads: line2.rects.clone(),
            },
        )),
        add(annot(
            "st",
            0,
            line2.rects[0],
            "#e03131",
            AnnotBody::StrikeOut {
                quads: line2.rects.clone(),
            },
        )),
        add(annot(
            "ft",
            0,
            r(320.0, 200.0, 200.0, 60.0),
            "#e03131",
            AnnotBody::FreeText {
                text: "Montant à confirmer — 18 400 €".into(),
                font: FontFamily::Sans,
                size: 12.0,
            },
        )),
        add(Annot {
            contents: Some("Dix jours : proposer quinze ?".into()),
            ..annot("nt", 0, r(540.0, 60.0, 22.0, 22.0), "#ffd43b", AnnotBody::Note)
        }),
        add(annot("sq", 0, r(320.0, 300.0, 150.0, 80.0), "#e03131", AnnotBody::Square)),
        add(annot("ci", 0, r(320.0, 400.0, 150.0, 80.0), "#2f9e44", AnnotBody::Circle)),
        add(annot(
            "ln",
            0,
            r(0.0, 0.0, 0.0, 0.0),
            "#1b1b20",
            AnnotBody::Line {
                from: Point { x: 330.0, y: 520.0 },
                to: Point { x: 480.0, y: 560.0 },
                arrow: true,
            },
        )),
        add(annot(
            "ck",
            0,
            r(330.0, 600.0, 16.0, 16.0),
            "#1d4ed8",
            AnnotBody::Check {
                style: CheckStyle::Check,
            },
        )),
        add(annot(
            "cx",
            0,
            r(360.0, 600.0, 16.0, 16.0),
            "#1d4ed8",
            AnnotBody::Check {
                style: CheckStyle::Cross,
            },
        )),
        add(annot(
            "cd",
            0,
            r(390.0, 600.0, 16.0, 16.0),
            "#1d4ed8",
            AnnotBody::Check { style: CheckStyle::Dot },
        )),
        add(annot(
            "im",
            0,
            r(330.0, 640.0, 80.0, 40.0),
            "#000000",
            AnnotBody::Image { image: img.key.clone() },
        )),
    ];
    let st = e.edit(doc, EditRequest::Apply(ops)).unwrap();
    assert_eq!(st.annots.len(), 12);
    assert_eq!(st.changed_pages, vec![0]);
    assert!(st.dirty && st.can_undo && !st.can_redo);
    let hl = st.annots.iter().find(|a| a.id == "hl").unwrap();
    assert!(hl.excerpt.as_deref().unwrap().starts_with("Ligne 1 : Le vif renard"));
    let ln = st.annots.iter().find(|a| a.id == "ln").unwrap();
    assert!(ln.rect.w > 150.0, "rectangle de ligne recalculé : {:?}", ln.rect);

    // Le rendu PDFium montre les annotations aux bons endroits.
    let img = render(doc, 0, &info);
    assert!(
        is_red(px(&img, 320.0 + 1.0, 340.0)),
        "bord gauche du rectangle rouge : {:?}",
        px(&img, 321.0, 340.0)
    );
    assert_eq!(px(&img, 395.0, 340.0), [255, 255, 255], "intérieur du rectangle vide");
    let blue = px(&img, 335.0, 660.0);
    assert!(blue[2] > 200 && blue[0] < 60, "image bleue du tampon : {blue:?}");
    assert_eq!(px(&img, 400.0, 660.0), [255, 255, 255], "transparence PNG respectée");

    // Enregistrement : révision incrémentale ajoutée aux octets d'origine.
    e.save(doc, None).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert!(
        saved.len() > original.len() && saved[..original.len()] == original[..],
        "préfixe d'origine intact"
    );
    qpdf_check(&path, None);
    e.close(doc);

    // Relecture : mêmes annotations, mêmes types, géométrie conservée.
    let info = e.open(&path, None).unwrap();
    let back = e.annotations(info.id).unwrap();
    assert_eq!(back.len(), 12);
    for a in &st.annots {
        let b = back
            .iter()
            .find(|b| b.id == a.id)
            .unwrap_or_else(|| panic!("{} perdue", a.id));
        assert_eq!(std::mem::discriminant(&a.body), std::mem::discriminant(&b.body), "{}", a.id);
        assert!(
            (a.rect.x - b.rect.x).abs() < 0.6 && (a.rect.y - b.rect.y).abs() < 0.6,
            "{} : {:?} ≠ {:?}",
            a.id,
            a.rect,
            b.rect
        );
        assert_eq!(b.author.as_deref(), Some("Testeur"));
        if !matches!(a.body, AnnotBody::Image { .. }) {
            assert_eq!(a.color, b.color, "{}", a.id);
        }
    }
    let ft = back.iter().find(|a| a.id == "ft").unwrap();
    assert_eq!(
        ft.body,
        AnnotBody::FreeText {
            text: "Montant à confirmer — 18 400 €".into(),
            font: FontFamily::Sans,
            size: 12.0
        }
    );
    let nt = back.iter().find(|a| a.id == "nt").unwrap();
    assert_eq!(nt.contents.as_deref(), Some("Dix jours : proposer quinze ?"));
    assert!(matches!(
        back.iter().find(|a| a.id == "ln").unwrap().body,
        AnnotBody::Line { arrow: true, .. }
    ));
    assert!(matches!(
        back.iter().find(|a| a.id == "cx").unwrap().body,
        AnnotBody::Check {
            style: CheckStyle::Cross
        }
    ));
    assert!(!e.edit(info.id, EditRequest::Load).unwrap().dirty);
}

#[test]
fn undo_redo_and_hidden() {
    let e = engine();
    let path = scratch("texte-simple.pdf", "undo");
    let info = e.open(&path, None).unwrap();
    let doc = info.id;
    e.edit(
        doc,
        EditRequest::Apply(vec![add(annot(
            "a",
            1,
            r(100.0, 100.0, 50.0, 50.0),
            "#e03131",
            AnnotBody::Square,
        ))]),
    )
    .unwrap();
    let mut moved = annot("a", 1, r(200.0, 100.0, 50.0, 50.0), "#e03131", AnnotBody::Square);
    moved.width = 4.0;
    e.edit(doc, EditRequest::Apply(vec![AnnotOp::Update { annot: moved }]))
        .unwrap();
    let st = e.edit(doc, EditRequest::Undo).unwrap();
    assert_eq!(st.annots[0].rect.x, 100.0);
    assert!(st.can_redo && st.dirty);
    let st = e.edit(doc, EditRequest::Undo).unwrap();
    assert!(st.annots.is_empty() && !st.dirty && !st.can_undo);
    let st = e.edit(doc, EditRequest::Redo).unwrap();
    assert_eq!(st.annots.len(), 1);
    // Masquage pour édition en place : n'apparaît plus au rendu, hors historique.
    let before = render(doc, 1, &info);
    assert!(is_red(px(&before, 101.0, 125.0)));
    let st = e
        .edit(
            doc,
            EditRequest::SetHidden {
                id: "a".into(),
                hidden: true,
            },
        )
        .unwrap();
    assert!(st.annots[0].hidden);
    let hidden = render(doc, 1, &info);
    assert_eq!(px(&hidden, 101.0, 125.0), [255, 255, 255]);
    e.edit(
        doc,
        EditRequest::SetHidden {
            id: "a".into(),
            hidden: false,
        },
    )
    .unwrap();
    // Un masquage en cours n'est jamais enregistré.
    e.edit(
        doc,
        EditRequest::SetHidden {
            id: "a".into(),
            hidden: true,
        },
    )
    .unwrap();
    e.save(doc, None).unwrap();
    e.close(doc);
    let info = e.open(&path, None).unwrap();
    let after = render(info.id, 1, &info);
    assert!(is_red(px(&after, 101.0, 125.0)), "annotation visible après enregistrement");
}

/// Applique `/Rotate` et un décalage de `/CropBox` aux pages d'un PDF.
fn rotated_fixture(dst: &Path) {
    let mut doc = lopdf::Document::load(root().join("fixtures/texte-simple.pdf")).unwrap();
    let pages: Vec<_> = doc.get_pages().into_values().collect();
    for (i, id) in pages.iter().enumerate() {
        let d = doc.get_dictionary_mut(*id).unwrap();
        d.set("Rotate", [90, 180, 270][i % 3]);
        d.set("CropBox", vec![10.into(), 20.into(), 585.into(), 822.into()]);
    }
    doc.save(dst).unwrap();
}

#[test]
fn rotated_and_cropped_pages() {
    let e = engine();
    let dir = scratch("texte-simple.pdf", "rotate");
    let path = dir.with_file_name("tourne.pdf");
    rotated_fixture(&path);
    let info = e.open(&path, None).unwrap();
    assert!(
        (info.pages[0].width - 802.0).abs() < 0.5,
        "page tournée : largeur = hauteur de la CropBox"
    );
    let before: Vec<_> = (0..3).map(|p| render(info.id, p, &info)).collect();
    let mut ops = vec![];
    for p in 0..3 {
        ops.push(add(annot(
            &format!("sq{p}"),
            p,
            r(100.0, 100.0, 120.0, 60.0),
            "#e03131",
            AnnotBody::Square,
        )));
        ops.push(add(annot(
            &format!("ft{p}"),
            p,
            r(300.0, 100.0, 200.0, 40.0),
            "#e03131",
            AnnotBody::FreeText {
                text: "Droit".into(),
                font: FontFamily::Sans,
                size: 20.0,
            },
        )));
    }
    e.edit(info.id, EditRequest::Apply(ops)).unwrap();
    for p in 0..3 {
        let img = render(info.id, p, &info);
        assert!(
            is_red(px(&img, 101.0, 130.0)),
            "page {p} : bord gauche du carré à sa place ({:?})",
            px(&img, 101.0, 130.0)
        );
        assert!(is_red(px(&img, 160.0, 101.0)), "page {p} : bord haut");
        assert!(!is_red(px(&img, 160.0, 130.0)), "page {p} : intérieur");
        // Texte droit : des pixels rouges ajoutés en haut de la zone, aucun en dessous de la ligne.
        let added = |y0: u32, y1: u32| {
            let mut n = 0;
            for y in y0..y1 {
                for x in 300..420 {
                    if is_red(px(&img, x as f32, y as f32)) && !is_red(px(&before[p as usize], x as f32, y as f32)) {
                        n += 1;
                    }
                }
            }
            n
        };
        assert!(added(100, 122) > 50, "page {p} : texte présent en haut de la zone");
        assert_eq!(added(124, 140), 0, "page {p} : texte orienté comme la page affichée");
    }
    let saved = path.with_file_name("tourne-annote.pdf");
    e.save(info.id, Some(saved.clone())).unwrap();
    qpdf_check(&saved, None);
    e.close(info.id);
    let back = e.open(&saved, None).unwrap();
    let annots = e.annotations(back.id).unwrap();
    let sq = annots.iter().find(|a| a.id == "sq2").unwrap();
    assert!((sq.rect.x - 100.0).abs() < 0.6 && (sq.rect.y - 100.0).abs() < 0.6 && (sq.rect.w - 120.0).abs() < 0.6);
}

#[test]
fn xref_stream_base() {
    let e = engine();
    let dir = scratch("texte-simple.pdf", "xrefstm");
    let path = dir.with_file_name("objstm.pdf");
    let mut doc = lopdf::Document::load(root().join("fixtures/texte-simple.pdf")).unwrap();
    doc.save_modern(&mut std::fs::File::create(&path).unwrap()).unwrap();
    let info = e.open(&path, None).unwrap();
    e.edit(
        info.id,
        EditRequest::Apply(vec![add(annot(
            "x",
            0,
            r(50.0, 50.0, 40.0, 40.0),
            "#e03131",
            AnnotBody::Circle,
        ))]),
    )
    .unwrap();
    e.save(info.id, None).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(4000)..]).to_string();
    assert!(tail.contains("/XRef"), "révision avec flux de références, comme l'original");
    qpdf_check(&path, None);
    e.close(info.id);
    let info = e.open(&path, None).unwrap();
    assert_eq!(e.annotations(info.id).unwrap().len(), 1);
}

#[test]
fn encrypted_document() {
    let e = engine();
    let path = scratch("chiffre-aes256.pdf", "crypt");
    let info = e.open(&path, Some("feuillet".into())).unwrap();
    e.edit(
        info.id,
        EditRequest::Apply(vec![add(Annot {
            contents: Some("Note chiffrée é".into()),
            ..annot("n", 0, r(60.0, 60.0, 22.0, 22.0), "#ffd43b", AnnotBody::Note)
        })]),
    )
    .unwrap();
    e.save(info.id, None).unwrap();
    qpdf_check(&path, Some("feuillet"));
    e.close(info.id);
    assert_eq!(e.open(&path, None).unwrap_err(), Error::PasswordRequired);
    let info = e.open(&path, Some("feuillet".into())).unwrap();
    let a = e.annotations(info.id).unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].contents.as_deref(), Some("Note chiffrée é"));
    // Le texte de la note n'apparaît pas en clair dans le fichier.
    let raw = std::fs::read(&path).unwrap();
    assert!(!raw.windows(5).any(|w| w == b"Note "), "contenu chiffré");
}

#[test]
fn existing_annotations_move_and_remove() {
    let e = engine();
    let path = scratch("avec-annotations.pdf", "existing");
    let info = e.open(&path, None).unwrap();
    let list = e.annotations(info.id).unwrap();
    assert_eq!(list.len(), 1, "les liens ne font pas partie du modèle");
    let sq = list[0].clone();
    assert!(matches!(sq.body, AnnotBody::Square));
    assert_eq!(sq.contents.as_deref(), Some("Carré existant"));
    assert_eq!(sq.color, "#ff0000");
    let mut moved = sq.clone();
    moved.rect.x += 100.0;
    moved.color = "#2f9e44".into();
    e.edit(info.id, EditRequest::Apply(vec![AnnotOp::Update { annot: moved.clone() }]))
        .unwrap();
    e.save(info.id, None).unwrap();
    qpdf_check(&path, None);
    e.close(info.id);
    let info = e.open(&path, None).unwrap();
    let list = e.annotations(info.id).unwrap();
    assert_eq!(list.len(), 1);
    assert!((list[0].rect.x - moved.rect.x).abs() < 0.6);
    assert_eq!(list[0].color, "#2f9e44");
    assert_eq!(list[0].contents.as_deref(), Some("Carré existant"));
    assert_eq!(e.links(info.id, 0).unwrap().len(), 2, "liens conservés");
    e.edit(info.id, EditRequest::Apply(vec![AnnotOp::Remove { id: list[0].id.clone() }]))
        .unwrap();
    e.save(info.id, None).unwrap();
    e.close(info.id);
    let info = e.open(&path, None).unwrap();
    assert!(e.annotations(info.id).unwrap().is_empty());
    assert_eq!(e.links(info.id, 0).unwrap().len(), 2, "liens conservés");
    qpdf_check(&path, None);
}

#[test]
fn signed_document_keeps_signed_bytes() {
    let e = engine();
    let path = scratch("signe-valide.pdf", "signed");
    let original = std::fs::read(&path).unwrap();
    let info = e.open(&path, None).unwrap();
    let sel = e
        .text_range(info.id, 0, Point { x: 73.0, y: 125.0 }, Point { x: 200.0, y: 125.0 })
        .unwrap();
    e.edit(
        info.id,
        EditRequest::Apply(vec![add(annot(
            "h",
            0,
            sel.rects[0],
            "#ffd43b",
            AnnotBody::Highlight { quads: sel.rects },
        ))]),
    )
    .unwrap();
    e.save(info.id, None).unwrap();
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(&saved[..original.len()], &original[..]);
    qpdf_check(&path, None);
    let info = e.open(&path, None).unwrap();
    assert_eq!(info.signature_count, 1);
}

#[test]
fn save_as_leaves_original_untouched() {
    let e = engine();
    let path = scratch("texte-simple.pdf", "saveas");
    let original = std::fs::read(&path).unwrap();
    let info = e.open(&path, None).unwrap();
    e.edit(
        info.id,
        EditRequest::Apply(vec![add(annot(
            "s",
            0,
            r(10.0, 10.0, 30.0, 30.0),
            "#e03131",
            AnnotBody::Square,
        ))]),
    )
    .unwrap();
    let copy = path.with_file_name("copie.pdf");
    let new_info = e.save(info.id, Some(copy.clone())).unwrap();
    assert_eq!(new_info.name, "copie.pdf");
    assert_eq!(std::fs::read(&path).unwrap(), original, "original non modifié");
    assert!(std::fs::read(&copy).unwrap().len() > original.len());
}

#[test]
fn image_copied_between_documents() {
    let e = engine();
    let a = e.open(scratch("texte-simple.pdf", "copyimg-a"), None).unwrap();
    let path_b = scratch("texte-simple.pdf", "copyimg-b");
    let b = e.open(&path_b, None).unwrap();
    let png = path_b.with_file_name("tampon.png");
    tiny_png(&png);
    let img = e.import_image(a.id, &png).unwrap();

    assert!(e.copy_image(a.id, b.id, "inconnue".into()).is_err());
    e.copy_image(a.id, b.id, img.key.clone()).unwrap();
    e.edit(
        b.id,
        EditRequest::Apply(vec![add(annot(
            "img",
            0,
            r(100.0, 100.0, 40.0, 20.0),
            "#000000",
            AnnotBody::Image { image: img.key },
        ))]),
    )
    .unwrap();
    e.save(b.id, None).unwrap();
    qpdf_check(&path_b, None);
    let bytes = std::fs::read(&path_b).unwrap();
    assert!(
        bytes.windows(b"/Subtype /Image".len()).any(|w| w == b"/Subtype /Image")
            || bytes.windows(b"/Subtype/Image".len()).any(|w| w == b"/Subtype/Image")
    );
}

#[test]
fn pending_redaction_renders_black() {
    let e = engine();
    let info = e.open(scratch("texte-simple.pdf", "redact-black"), None).unwrap();
    let area = r(100.0, 200.0, 120.0, 40.0);
    e.edit(
        info.id,
        EditRequest::Apply(vec![add(annot(
            "x",
            0,
            area,
            "#e03131",
            AnnotBody::Redact { quads: vec![area] },
        ))]),
    )
    .unwrap();
    let img = render(info.id, 0, &info);
    assert_eq!(px(&img, 160.0, 220.0), [0, 0, 0], "intérieur noir");
    assert!(is_red(px(&img, 100.0, 220.0)), "liseré rouge");
}
