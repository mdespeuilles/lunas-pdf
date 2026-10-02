//! Caviardage : le contenu sous les zones est réellement supprimé du fichier enregistré.

use std::path::PathBuf;
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
        Engine::start(Some(&dir)).expect("libpdfium : lancer `pnpm pdfium`")
    })
}

fn scratch(name: &str, test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("feuillet-redact-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(name);
    std::fs::copy(root().join("fixtures").join(name), &dst).unwrap();
    dst
}

fn redact(id: &str, page: u32, quads: Vec<Rect>) -> AnnotOp {
    AnnotOp::Add {
        annot: Annot {
            id: id.into(),
            page,
            rect: quads[0],
            color: "#e03131".into(),
            opacity: 1.0,
            width: 1.0,
            contents: None,
            author: None,
            modified: None,
            excerpt: None,
            body: AnnotBody::Redact { quads },
            hidden: false,
        },
        index: None,
    }
}

fn page_text(doc: DocId, page: u32) -> String {
    engine()
        .text(doc, page)
        .unwrap()
        .runs
        .iter()
        .map(|r| if r.eol { format!("{}\n", r.text) } else { r.text.clone() })
        .collect()
}

/// Tous les flux du fichier, décompressés, concaténés (recherche de traces du texte supprimé).
fn all_streams(path: &PathBuf, password: Option<&str>) -> Vec<u8> {
    let opts = lopdf::LoadOptions {
        password: password.map(str::to_owned),
        ..Default::default()
    };
    let doc = lopdf::Document::load_with_options(path, opts).unwrap();
    let mut out = vec![];
    for obj in doc.objects.values() {
        if let lopdf::Object::Stream(s) = obj {
            out.extend(s.decompressed_content().unwrap_or_else(|_| s.content.clone()));
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn partial_line_text_is_removed() {
    let e = engine();
    let path = scratch("texte-simple.pdf", "text");
    let info = e.open(&path, None).unwrap();
    // Zone sur « renard brun saute » de la ligne 1 uniquement.
    let sel = e
        .text_range(info.id, 0, Point { x: 150.0, y: 125.0 }, Point { x: 240.0, y: 125.0 })
        .unwrap();
    assert!(sel.text.contains("renard brun"), "{}", sel.text);
    let st = e
        .edit(info.id, EditRequest::Apply(vec![redact("r", 0, sel.rects.clone())]))
        .unwrap();
    assert!(st.pending_redactions);
    e.save(info.id, None).unwrap();
    // État du même document juste après l'enregistrement.
    let after = e.edit(info.id, EditRequest::Load).unwrap();
    assert!(
        !after.dirty && !after.pending_redactions && after.annots.is_empty(),
        "état après enregistrement : {after:?}"
    );
    assert!(
        !page_text(info.id, 0)
            .lines()
            .find(|l| l.starts_with("Ligne 1 "))
            .unwrap()
            .contains("renard"),
        "texte rechargé"
    );
    e.close(info.id);

    let info = e.open(&path, None).unwrap();
    let text = page_text(info.id, 0);
    let line1 = text.lines().find(|l| l.starts_with("Ligne 1 ")).unwrap();
    assert!(line1.starts_with("Ligne 1 : Le vif"), "début de ligne conservé : {line1}");
    assert!(line1.ends_with("le chien paresseux."), "fin de ligne conservée : {line1}");
    assert!(
        !line1.contains("renard") && !line1.contains("brun"),
        "texte caviardé retiré : {line1}"
    );
    let line2 = text.lines().find(|l| l.starts_with("Ligne 2 ")).unwrap();
    assert_eq!(
        line2.trim(),
        "Ligne 2 : Le vif renard brun saute par-dessus le chien paresseux.",
        "ligne voisine intacte"
    );
    assert!(e.annotations(info.id).unwrap().is_empty(), "annotation de caviardage retirée");

    // Le contenu de la page 1 ne contient plus le texte retiré (les pages 2 et 3 ont aussi une « Ligne 1 »).
    let doc = lopdf::Document::load(&path).unwrap();
    let p1 = *doc.get_pages().get(&1).unwrap();
    let ops = lopdf::content::Content::decode(&doc.get_page_content(p1)).unwrap().operations;
    let shown: Vec<String> = ops
        .iter()
        .filter(|o| matches!(o.operator.as_str(), "Tj" | "TJ" | "'" | "\""))
        .map(|o| {
            let mut s = String::new();
            for v in &o.operands {
                let items = match v {
                    lopdf::Object::Array(a) => a.clone(),
                    other => vec![other.clone()],
                };
                for it in items {
                    if let lopdf::Object::String(b, _) = it {
                        s.extend(b.iter().map(|&c| c as char));
                    }
                }
            }
            s
        })
        .collect();
    let l1 = shown.iter().find(|s| s.starts_with("Ligne 1 ")).expect("ligne 1 présente");
    assert!(!l1.contains("renard"), "glyphes retirés du flux : {l1}");
    assert!(!all_streams(&path, None).is_empty());
    let file = std::fs::read(&path).unwrap();
    assert_eq!(
        file.windows(5).filter(|w| w == b"%%EOF").count(),
        1,
        "réécriture complète, sans historique"
    );

    // Vérification indépendante (Poppler), si disponible.
    if let Ok(out) = Command::new("pdftotext")
        .args(["-f", "1", "-l", "1"])
        .arg(&path)
        .arg("-")
        .output()
    {
        let t = String::from_utf8_lossy(&out.stdout);
        let l1 = t.lines().find(|l| l.starts_with("Ligne 1 ")).unwrap_or_default();
        assert!(!l1.contains("renard"), "pdftotext : {l1}");
    }
    if let Ok(out) = Command::new("qpdf").arg("--check").arg(&path).output() {
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    }
}

#[test]
fn scanned_image_pixels_are_removed() {
    let e = engine();
    let path = scratch("scan-lourd.pdf", "image");
    let original = std::fs::read(&path).unwrap();
    let info = e.open(&path, None).unwrap();
    let area = Rect {
        x: 50.0,
        y: 60.0,
        w: 300.0,
        h: 40.0,
    };
    e.edit(info.id, EditRequest::Apply(vec![redact("r", 0, vec![area])])).unwrap();
    e.save(info.id, None).unwrap();
    e.close(info.id);

    // L'image de la page 1 a été réencodée : les pixels de la zone sont noirs dans l'image même.
    let doc = lopdf::Document::load(&path).unwrap();
    let page = *doc.get_pages().get(&1).unwrap();
    let res = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
    let xobjects = res.get(b"XObject").unwrap().as_dict().unwrap();
    let (_, img_ref) = xobjects
        .iter()
        .find(|(k, _)| k.starts_with(b"FeuilletR"))
        .expect("image réécrite");
    let stream = doc.get_object(img_ref.as_reference().unwrap()).unwrap().as_stream().unwrap();
    let img = image::load_from_memory(&stream.content).unwrap().to_rgb8();
    // Zone (affichage, pt) → pixels : page A4 de 595 pt = 2481 px.
    let k = img.width() as f32 / 595.28;
    let p = img.get_pixel(((area.x + 150.0) * k) as u32, ((area.y + 20.0) * k) as u32);
    assert!(p.0.iter().all(|&c| c < 40), "pixel sous la zone noirci : {p:?}");
    let q = img.get_pixel(((area.x + 150.0) * k) as u32, ((area.y + 120.0) * k) as u32);
    assert!(q.0.iter().all(|&c| c > 150), "pixel hors zone intact : {q:?}");

    // Les octets JPEG d'origine de cette page ont disparu du fichier.
    let orig_doc = lopdf::Document::load_mem(&original).unwrap();
    let opage = *orig_doc.get_pages().get(&1).unwrap();
    let ores = orig_doc
        .get_dictionary(opage)
        .unwrap()
        .get(b"Resources")
        .unwrap()
        .as_dict()
        .unwrap()
        .clone();
    let oimg = ores
        .get(b"XObject")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Im0")
        .unwrap()
        .as_reference()
        .unwrap();
    let ojpeg = &orig_doc.get_object(oimg).unwrap().as_stream().unwrap().content;
    let file = std::fs::read(&path).unwrap();
    assert!(!contains(&file, &ojpeg[1000..1064]), "JPEG d'origine supprimé");
}

#[test]
fn vector_paths_are_cut() {
    let e = engine();
    let path = scratch("long-320-pages.pdf", "vector");
    let info = e.open(&path, None).unwrap();
    // Les vagues bleues (courbes de Bézier) occupent le bas de chaque page, vers y ≈ 700–770 pt.
    let before = lopdf::Document::load(&path).unwrap();
    let curves = |doc: &lopdf::Document| {
        let p = *doc.get_pages().get(&1).unwrap();
        lopdf::content::Content::decode(&doc.get_page_content(p))
            .unwrap()
            .operations
            .iter()
            .filter(|o| o.operator == "c")
            .count()
    };
    let n_before = curves(&before);
    e.edit(
        info.id,
        EditRequest::Apply(vec![redact(
            "r",
            0,
            vec![Rect {
                x: 60.0,
                y: 690.0,
                w: 480.0,
                h: 100.0,
            }],
        )]),
    )
    .unwrap();
    e.save(info.id, None).unwrap();
    let after = lopdf::Document::load(&path).unwrap();
    assert!(
        curves(&after) < n_before,
        "courbes sous la zone retirées ({} → {})",
        n_before,
        curves(&after)
    );
    if let Ok(out) = Command::new("qpdf").arg("--check").arg(&path).output() {
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    }
}

#[test]
fn encrypted_document_stays_encrypted() {
    let e = engine();
    let path = scratch("chiffre-aes256.pdf", "crypt");
    let info = e.open(&path, Some("feuillet".into())).unwrap();
    let sel = e
        .text_range(info.id, 0, Point { x: 73.0, y: 125.0 }, Point { x: 400.0, y: 125.0 })
        .unwrap();
    e.edit(info.id, EditRequest::Apply(vec![redact("r", 0, sel.rects)])).unwrap();
    e.save(info.id, None).unwrap();
    e.close(info.id);
    assert_eq!(e.open(&path, None).unwrap_err(), Error::PasswordRequired);
    let info = e.open(&path, Some("feuillet".into())).unwrap();
    let text = page_text(info.id, 0);
    assert!(!text.contains("Ligne 1 "), "ligne caviardée : {text}");
    assert!(text.contains("Ligne 2 : Le vif renard"));
    if let Ok(out) = Command::new("qpdf")
        .args(["--check", "--password=feuillet"])
        .arg(&path)
        .output()
    {
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    }
}

#[test]
fn covered_annotations_are_removed() {
    let e = engine();
    let path = scratch("avec-annotations.pdf", "annots");
    let info = e.open(&path, None).unwrap();
    let sq = e.annotations(info.id).unwrap()[0].clone();
    e.edit(
        info.id,
        EditRequest::Apply(vec![redact(
            "r",
            0,
            vec![Rect {
                x: sq.rect.x + 10.0,
                y: sq.rect.y + 10.0,
                w: 20.0,
                h: 20.0,
            }],
        )]),
    )
    .unwrap();
    e.save(info.id, None).unwrap();
    e.close(info.id);
    let info = e.open(&path, None).unwrap();
    assert!(e.annotations(info.id).unwrap().is_empty(), "carré recouvert supprimé");
    assert_eq!(e.links(info.id, 0).unwrap().len(), 2, "liens hors zone conservés");
}
