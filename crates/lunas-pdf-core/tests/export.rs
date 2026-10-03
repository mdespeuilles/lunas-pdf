//! Export (planche 10) : PDF, PDF aplati, pages, mot de passe, compression, images.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use lunas_pdf_core::export::{ExportFormat, ExportOptions, ExportProtection, Quality};
use lunas_pdf_core::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn engine() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| {
        let dir = std::env::var_os("LUNAS_PDF_PDFIUM_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root().join("src-tauri/pdfium/lib"));
        Engine::start(Some(&dir)).expect("libpdfium : lancer `bun pdfium`")
    })
}

fn out(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lunas-pdf-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn opts(format: ExportFormat) -> ExportOptions {
    ExportOptions {
        format,
        quality: Quality::Max,
        pages: None,
        dpi: 72,
        protection: None,
    }
}

fn qpdf(args: &[&str], path: &Path) -> Option<String> {
    let o = Command::new("qpdf").args(args).arg(path).output().ok()?;
    Some(String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr))
}

fn qpdf_ok(path: &Path, password: Option<&str>) {
    let pw = password.map(|p| format!("--password={p}"));
    let mut args = vec!["--check"];
    if let Some(p) = &pw {
        args.push(p);
    }
    if let Some(o) = qpdf(&args, path) {
        assert!(o.contains("No syntax or stream encoding errors"), "{o}");
    }
}

#[test]
fn pdf_keeps_annotations_and_flattened_merges_them() {
    let e = engine();
    let src = e.open(root().join("fixtures/avec-annotations.pdf"), None).unwrap();
    let before = e.annotations(src.id).unwrap().len();
    assert!(before > 0);

    let p = out("annotations.pdf");
    let r = e.export(src.id, opts(ExportFormat::Pdf), &p).unwrap();
    assert_eq!(r.files, [p.display().to_string()]);
    qpdf_ok(&p, None);
    let x = e.open(&p, None).unwrap();
    assert_eq!(e.annotations(x.id).unwrap().len(), before);

    let f = out("aplati.pdf");
    e.export(src.id, opts(ExportFormat::Flattened), &f).unwrap();
    qpdf_ok(&f, None);
    let y = e.open(&f, None).unwrap();
    assert_eq!(y.pages.len(), src.pages.len());
    assert!(e.annotations(y.id).unwrap().is_empty(), "annotations fusionnées dans la page");
}

#[test]
fn flattened_form_keeps_typed_values() {
    let e = engine();
    let path = out("formulaire-source.pdf");
    std::fs::copy(root().join("fixtures/formulaire-acroform.pdf"), &path).unwrap();
    let src = e.open(&path, None).unwrap();
    e.edit(
        src.id,
        EditRequest::Apply(vec![AnnotOp::SetField {
            id: "nom".into(),
            value: vec!["Durand".into()],
        }]),
    )
    .unwrap();
    let f = out("formulaire-aplati.pdf");
    e.export(src.id, opts(ExportFormat::Flattened), &f).unwrap();
    let y = e.open(&f, None).unwrap();
    assert!(e.edit(y.id, EditRequest::Load).unwrap().fields.is_empty(), "plus de champs");
    let text: String = e.text(y.id, 0).unwrap().runs.iter().map(|r| r.text.clone()).collect();
    assert!(text.contains("Durand"), "{text}");
}

#[test]
fn selected_pages_and_estimate() {
    let e = engine();
    let src = e.open(root().join("fixtures/long-320-pages.pdf"), None).unwrap();
    let mut o = opts(ExportFormat::Pdf);
    o.pages = Some(vec![4, 2]);
    let est = e.export_estimate(src.id, o.clone()).unwrap();
    assert!(est.files.is_empty() && est.size > 0);
    let p = out("pages.pdf");
    let r = e.export(src.id, o, &p).unwrap();
    assert_eq!(r.size, est.size);
    assert_eq!(std::fs::metadata(&p).unwrap().len(), r.size);
    let x = e.open(&p, None).unwrap();
    assert_eq!(x.pages.len(), 2);
    let t = |doc, page| {
        e.text(doc, page)
            .unwrap()
            .runs
            .iter()
            .map(|r| r.text.clone())
            .collect::<String>()
    };
    assert_eq!(t(x.id, 0), t(src.id, 4));
    // Toutes les pages : nettement plus lourd que deux.
    assert!(e.export_estimate(src.id, opts(ExportFormat::Pdf)).unwrap().size > r.size * 10);
}

#[test]
fn password_and_permissions() {
    let e = engine();
    let src = e.open(root().join("fixtures/texte-simple.pdf"), None).unwrap();
    let mut o = opts(ExportFormat::Pdf);
    o.protection = Some(ExportProtection {
        password: "s3cret-Feuillet".into(),
        allow_print: true,
        allow_copy: false,
    });
    let p = out("protege.pdf");
    e.export(src.id, o, &p).unwrap();
    assert!(matches!(e.open(&p, None), Err(Error::PasswordRequired)));
    let x = e.open(&p, Some("s3cret-Feuillet".into())).unwrap();
    assert_eq!(x.pages.len(), src.pages.len());
    qpdf_ok(&p, Some("s3cret-Feuillet"));
    if let Some(enc) = qpdf(&["--show-encryption", "--password=s3cret-Feuillet"], &p) {
        assert!(enc.contains("R = 6"), "{enc}");
        assert!(enc.contains("extract for any purpose: not allowed"), "{enc}");
        assert!(enc.contains("print high resolution: allowed"), "{enc}");
    }
    // Mot de passe vide refusé.
    let mut o = opts(ExportFormat::Pdf);
    o.protection = Some(ExportProtection {
        password: String::new(),
        allow_print: true,
        allow_copy: true,
    });
    assert!(e.export(src.id, o, out("vide.pdf")).is_err());
}

#[test]
fn encrypted_source_exports_in_clear_unless_protected() {
    let e = engine();
    let src = e
        .open(root().join("fixtures/chiffre-aes256.pdf"), Some("feuillet".into()))
        .unwrap();
    let p = out("dechiffre.pdf");
    e.export(src.id, opts(ExportFormat::Pdf), &p).unwrap();
    let x = e.open(&p, None).unwrap();
    assert_eq!(x.pages.len(), src.pages.len());
    qpdf_ok(&p, None);
}

/// PDF d'une page avec une grande image RVB non compressée par JPEG.
fn image_pdf(path: &Path) {
    use lopdf::{Document, Object, Stream, dictionary};
    let (w, h) = (2400u32, 1600u32);
    let mut raw = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            raw.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, ((x ^ y) & 0xff) as u8]);
        }
    }
    let mut doc = Document::with_version("1.7");
    let mut img = Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64, "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8 },
        raw,
    );
    img.compress().unwrap();
    let img_id = doc.add_object(img);
    let content = doc.add_object(Stream::new(dictionary! {}, b"q 540 0 0 360 36 400 cm /Im0 Do Q".to_vec()));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content, "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => img_id } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", cat);
    doc.save(path).unwrap();
}

#[test]
fn image_quality_shrinks_the_file() {
    let e = engine();
    let src_path = out("image-source.pdf");
    image_pdf(&src_path);
    let src = e.open(&src_path, None).unwrap();
    let max = e.export_estimate(src.id, opts(ExportFormat::Pdf)).unwrap().size;
    let mut o = opts(ExportFormat::Pdf);
    o.quality = Quality::Balanced;
    let balanced = e.export_estimate(src.id, o.clone()).unwrap().size;
    o.quality = Quality::Light;
    let p = out("leger.pdf");
    let light = e.export(src.id, o, &p).unwrap().size;
    assert!(light < balanced && balanced < max, "{light} < {balanced} < {max}");
    qpdf_ok(&p, None);
    let doc = lopdf::Document::load(&p).unwrap();
    let widths: Vec<i64> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter(|s| s.dict.get(b"Subtype").and_then(|v| v.as_name()).ok() == Some(b"Image".as_slice()))
        .map(|s| s.dict.get(b"Width").unwrap().as_i64().unwrap())
        .collect();
    assert_eq!(widths, [1400]);
}

#[test]
fn one_image_per_page() {
    let e = engine();
    let src = e.open(root().join("fixtures/texte-simple.pdf"), None).unwrap();
    let mut o = opts(ExportFormat::Png);
    o.dpi = 144;
    o.pages = Some(vec![0, 2]);
    let base = out("pages.png");
    let r = e.export(src.id, o.clone(), &base).unwrap();
    assert_eq!(r.files.len(), 2);
    assert!(r.files[1].ends_with("pages-02.png"));
    let img = image::open(&r.files[0]).unwrap();
    assert_eq!(img.width(), (src.pages[0].width * 2.0).round() as u32);

    o.format = ExportFormat::Jpg;
    o.pages = Some(vec![1]);
    let r = e.export(src.id, o, out("seule.jpg")).unwrap();
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].ends_with("seule.jpg"));
    assert!(image::open(&r.files[0]).is_ok());
}
