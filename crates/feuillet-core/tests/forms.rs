//! Formulaires AcroForm : lecture des champs, saisie, apparences, enregistrement.

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
        Engine::start(Some(&dir)).expect("libpdfium : lancer `pnpm pdfium`")
    })
}

fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("feuillet-form-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join("formulaire.pdf");
    std::fs::copy(root().join("fixtures/formulaire-acroform.pdf"), &dst).unwrap();
    dst
}

fn field<'a>(fields: &'a [FormField], id: &str) -> &'a FormField {
    fields.iter().find(|f| f.id == id).unwrap_or_else(|| panic!("champ {id}"))
}

fn set(id: &str, value: &[&str]) -> AnnotOp {
    AnnotOp::SetField {
        id: id.into(),
        value: value.iter().map(|s| s.to_string()).collect(),
    }
}

/// Pixels sombres à l'intérieur d'un rectangle (bordure exclue), rendu PDFium à 1 px/pt.
fn dark_inside(doc: DocId, info: &DocInfo, r: Rect) -> usize {
    let g = &info.pages[0];
    let (w, h) = (g.width.round() as u32, g.height.round() as u32);
    let b = engine()
        .render(RenderRequest {
            doc,
            page: 0,
            width: w,
            height: h,
            tile: None,
            priority: 10,
            epoch: u32::MAX,
        })
        .unwrap();
    let mut n = 0;
    for y in (r.y + 3.0) as u32..(r.y + r.h - 3.0) as u32 {
        for x in (r.x + 3.0) as u32..(r.x + r.w - 3.0) as u32 {
            let i = ((y * w + x) * 4) as usize;
            if b.rgba[i] < 100 && b.rgba[i + 1] < 100 && b.rgba[i + 2] < 100 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn reads_fields_of_every_kind() {
    let info = engine().open(scratch("read"), None).unwrap();
    assert_eq!(info.form, FormKind::AcroForm);
    let st = engine().edit(info.id, EditRequest::Load).unwrap();
    let f = &st.fields;
    assert_eq!(f.len(), 8, "{:?}", f.iter().map(|f| &f.id).collect::<Vec<_>>());
    assert!(matches!(field(f, "nom").kind, FieldKind::Text { multiline: false, .. }));
    assert!(matches!(
        field(f, "commentaire").kind,
        FieldKind::Text { multiline: true, .. }
    ));
    assert_eq!(field(f, "accepte").kind, FieldKind::Checkbox);
    assert!(field(f, "accepte").value.is_empty());
    let radio = field(f, "formule");
    assert_eq!(radio.kind, FieldKind::Radio);
    assert_eq!(radio.value, ["mensuelle"]);
    assert_eq!(radio.widgets.len(), 3);
    assert_eq!(radio.widgets[1].on_state.as_deref(), Some("annuelle"));
    assert!(matches!(&field(f, "pays").kind, FieldKind::Combo { options, editable: false } if options.len() == 4));
    assert_eq!(field(f, "pays").value, ["France"]);
    assert!(matches!(field(f, "interets").kind, FieldKind::List { multi: false, .. }));
    // Géométrie : « nom » à x = 180, 300 × 20 pt, en haut de la page.
    let w = &field(f, "nom").widgets[0];
    assert_eq!(
        (w.page, w.rect.x.round(), w.rect.w.round(), w.rect.h.round()),
        (0, 180.0, 300.0, 20.0)
    );
    assert!(w.rect.y < 150.0);
}

#[test]
fn fill_undo_save_and_reopen() {
    let e = engine();
    let path = scratch("fill");
    let info = e.open(&path, None).unwrap();
    let st = e.edit(info.id, EditRequest::Load).unwrap();
    let nom = field(&st.fields, "nom").widgets[0].rect;
    let on = field(&st.fields, "accepte").widgets[0].on_state.clone().unwrap();
    let blank = dark_inside(info.id, &info, nom);

    let st = e
        .edit(
            info.id,
            EditRequest::Apply(vec![
                set("nom", &["Élodie Durand"]),
                set("commentaire", &["Première ligne\nDeuxième ligne"]),
                set("accepte", &[&on]),
                set("formule", &["annuelle"]),
                set("pays", &["Belgique"]),
                set("interets", &["Musique"]),
            ]),
        )
        .unwrap();
    assert!(st.dirty && st.can_undo);
    assert_eq!(st.unsaved_count, 6);
    assert_eq!(st.changed_pages, [0]);
    assert!(dark_inside(info.id, &info, nom) > blank + 40, "texte du champ rendu");

    // Annuler / rétablir : un seul pas pour le lot.
    let st = e.edit(info.id, EditRequest::Undo).unwrap();
    assert_eq!(field(&st.fields, "formule").value, ["mensuelle"]);
    assert!(!st.dirty);
    let st = e.edit(info.id, EditRequest::Redo).unwrap();
    assert_eq!(field(&st.fields, "nom").value, ["Élodie Durand"]);

    e.save(info.id, None).unwrap();
    qpdf_check(&path);
    let again = e.open(&path, None).unwrap();
    let f = e.edit(again.id, EditRequest::Load).unwrap().fields;
    assert_eq!(field(&f, "nom").value, ["Élodie Durand"]);
    assert_eq!(field(&f, "commentaire").value, ["Première ligne\nDeuxième ligne"]);
    assert_eq!(field(&f, "accepte").value, [on]);
    assert_eq!(field(&f, "formule").value, ["annuelle"]);
    assert_eq!(field(&f, "pays").value, ["Belgique"]);
    assert_eq!(field(&f, "interets").value, ["Musique"]);
    assert!(dark_inside(again.id, &again, nom) > blank + 40);
    poppler_shows_text(&path, nom);
}

#[test]
fn read_only_fields_are_not_changed() {
    let e = engine();
    let info = e.open(scratch("ro"), None).unwrap();
    let st = e.edit(info.id, EditRequest::Apply(vec![set("inconnu", &["x"])])).unwrap();
    assert!(!st.can_undo && !st.dirty);
}

fn qpdf_check(path: &Path) {
    match Command::new("qpdf").arg("--check").arg(path).output() {
        Ok(out) => assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout)),
        Err(_) => eprintln!("qpdf absent : vérification ignorée"),
    }
}

/// Vérification indépendante : Poppler affiche le texte saisi (apparence générée).
fn poppler_shows_text(path: &Path, r: Rect) {
    let out = path.with_file_name("poppler");
    let Ok(st) = Command::new("pdftoppm")
        .args(["-r", "72", "-f", "1", "-l", "1", "-gray", "-singlefile"])
        .arg(path)
        .arg(&out)
        .status()
    else {
        eprintln!("pdftoppm absent : vérification ignorée");
        return;
    };
    assert!(st.success());
    let pgm = std::fs::read(out.with_extension("pgm")).unwrap();
    // En-tête « P5\n<w> <h>\n255\n ».
    let mut fields = vec![];
    let mut i = 0;
    while fields.len() < 4 {
        while pgm[i].is_ascii_whitespace() {
            i += 1;
        }
        let s = i;
        while !pgm[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(String::from_utf8_lossy(&pgm[s..i]).into_owned());
    }
    let w: usize = fields[1].parse().unwrap();
    let data = &pgm[i + 1..];
    let mut dark = 0;
    for y in (r.y + 3.0) as usize..(r.y + r.h - 3.0) as usize {
        for x in (r.x + 3.0) as usize..(r.x + r.w - 3.0) as usize {
            if data[y * w + x] < 100 {
                dark += 1;
            }
        }
    }
    assert!(dark > 40, "Poppler : {dark} pixels sombres dans le champ");
}

// --- Scripts standard d'Acrobat -----------------------------------------------------------------

/// Nom, ordonnée, actions (clé, script) et indicateurs `/Ff` d'un champ ajouté.
type FieldSpec<'a> = (&'a str, f32, &'a [(&'a str, &'a str)], i64);

/// Ajoute au fixture des champs avec scripts (formats, plage, calcul, script personnalisé).
fn scripted_form(dst: &Path) {
    use lopdf::{Dictionary, Object, dictionary};
    let mut doc = lopdf::Document::load(root().join("fixtures/formulaire-acroform.pdf")).unwrap();
    let page = *doc.get_pages().get(&1).unwrap();
    let specs: [FieldSpec; 7] = [
        (
            "prix",
            280.0,
            &[
                ("K", "AFNumber_Keystroke(2, 2, 0, 0, \" €\", false);"),
                ("F", "AFNumber_Format(2, 2, 0, 0, \" €\", false);"),
                ("V", "AFRange_Validate(true, 0, true, 10000);"),
            ],
            0,
        ),
        ("quantite", 250.0, &[("F", "AFNumber_Format(0, 1, 0, 0, \"\", false);")], 0),
        (
            "total",
            220.0,
            &[
                ("F", "AFNumber_Format(2, 2, 0, 0, \" €\", false);"),
                ("C", "AFSimple_Calculate(\"PRD\", new Array (\"prix\", \"quantite\"));"),
            ],
            1,
        ),
        ("remise", 190.0, &[("F", "AFPercent_Format(1, 3);")], 0),
        ("date", 160.0, &[("F", "AFDate_FormatEx(\"dd/mm/yyyy\");")], 0),
        ("tel", 130.0, &[("F", "AFSpecial_Format(2);")], 0),
        (
            "perso",
            100.0,
            &[("C", "event.value = this.getField(\"prix\").value * 2;")],
            0,
        ),
    ];
    let mut ids = vec![];
    for (name, y, actions, ff) in specs {
        let mut aa = Dictionary::new();
        for (k, js) in actions {
            aa.set(*k, dictionary! { "S" => "JavaScript", "JS" => lopdf::text_string(js) });
        }
        // Apparence initiale (champ vide) : fond et bordure, comme les autres champs du fixture.
        let ap = doc.add_object(lopdf::Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 20.into()] },
            b"0.8 0.85 1 rg 0 0 200 20 re f 0 G 0.5 0.5 199 19 re S /Tx BMC EMC".to_vec(),
        ));
        let d = dictionary! {
            "AP" => dictionary! { "N" => ap }, "MK" => dictionary! { "BC" => vec![0.into()], "BG" => vec![0.8.into(), 0.85.into(), 1.into()] },
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal(name),
            "Rect" => vec![300.into(), y.into(), 500.into(), (y + 20.0).into()],
            "DA" => Object::string_literal("/Helv 11 Tf 0 g"), "F" => 4, "Ff" => ff, "P" => page,
            "AA" => aa,
        };
        ids.push(doc.add_object(d));
    }
    let annots = doc.get_dictionary(page).unwrap().get(b"Annots").unwrap().clone();
    let arr = match annots {
        Object::Reference(r) => doc.get_object_mut(r).unwrap().as_array_mut().unwrap(),
        _ => doc
            .get_dictionary_mut(page)
            .unwrap()
            .get_mut(b"Annots")
            .unwrap()
            .as_array_mut()
            .unwrap(),
    };
    arr.extend(ids.iter().map(|id| Object::Reference(*id)));
    let af = match doc.catalog().unwrap().get(b"AcroForm").unwrap().clone() {
        Object::Reference(r) => r,
        _ => panic!("AcroForm direct"),
    };
    let fields = match doc.get_dictionary(af).unwrap().get(b"Fields").unwrap().clone() {
        Object::Reference(r) => doc.get_object_mut(r).unwrap().as_array_mut().unwrap(),
        _ => doc
            .get_dictionary_mut(af)
            .unwrap()
            .get_mut(b"Fields")
            .unwrap()
            .as_array_mut()
            .unwrap(),
    };
    fields.extend(ids.iter().map(|id| Object::Reference(*id)));
    doc.get_dictionary_mut(af).unwrap().set("CO", vec![Object::Reference(ids[2])]);
    // Libellés à gauche des champs.
    let labels = [
        "Prix (0 à 10 000 €)",
        "Quantité",
        "Total = prix × quantité",
        "Remise (%)",
        "Date (jj/mm/aaaa)",
        "Téléphone",
        "Script personnalisé",
    ];
    let mut content = String::new();
    for ((_, y, _, _), label) in specs.iter().zip(labels) {
        let bytes: Vec<u8> = label
            .chars()
            .map(|c| match c {
                'à' => 0xe0,
                'é' => 0xe9,
                '×' => 0xd7,
                '€' => 0x80,
                c => c as u8,
            })
            .collect();
        let lit: String = bytes.iter().map(|b| format!("\\{b:03o}")).collect();
        content.push_str(&format!("BT /FLbl 11 Tf 72 {} Td ({lit}) Tj ET\n", y + 6.0));
    }
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" },
    );
    let stream = doc.add_object(lopdf::Stream::new(Dictionary::new(), content.into_bytes()));
    let pd = doc.get_dictionary(page).unwrap().clone();
    let mut res = match pd.get(b"Resources").unwrap() {
        Object::Reference(r) => doc.get_dictionary(*r).unwrap().clone(),
        o => o.as_dict().unwrap().clone(),
    };
    let mut fonts = match res.get(b"Font") {
        Ok(Object::Reference(r)) => doc.get_dictionary(*r).unwrap().clone(),
        Ok(o) => o.as_dict().unwrap().clone(),
        Err(_) => Dictionary::new(),
    };
    fonts.set("FLbl", font);
    res.set("Font", fonts);
    let mut contents = match pd.get(b"Contents").unwrap() {
        Object::Array(a) => a.clone(),
        o => vec![o.clone()],
    };
    contents.push(Object::Reference(stream));
    let pdm = doc.get_dictionary_mut(page).unwrap();
    pdm.set("Resources", res);
    pdm.set("Contents", contents);
    doc.save(dst).unwrap();
}

/// Contenu décompressé de l'apparence d'un champ dans un fichier enregistré.
fn appearance_of(path: &Path, name: &str) -> Vec<u8> {
    let doc = lopdf::Document::load(path).unwrap();
    for obj in doc.objects.values() {
        let Ok(d) = obj.as_dict() else { continue };
        if d.get(b"T").ok().and_then(|t| t.as_str().ok()) != Some(name.as_bytes()) {
            continue;
        }
        let n = d
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap()
            .as_reference()
            .unwrap();
        let s = doc.get_object(n).unwrap().as_stream().unwrap();
        return s.decompressed_content().unwrap_or_else(|_| s.content.clone());
    }
    panic!("champ {name}");
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
#[ignore = "régénère fixtures/formulaire-scripts.pdf"]
fn generate_scripted_fixture() {
    scripted_form(&root().join("fixtures/formulaire-scripts.pdf"));
}

#[test]
fn reads_standard_scripts() {
    let path = scratch("scripts-read").with_file_name("scripts.pdf");
    scripted_form(&path);
    let info = engine().open(&path, None).unwrap();
    let f = engine().edit(info.id, EditRequest::Load).unwrap().fields;
    assert!(
        matches!(&field(&f, "prix").format, Some(FieldFormat::Number { decimals: 2, sep_style: 2, currency, .. }) if currency == " €")
    );
    assert_eq!(
        field(&f, "prix").range,
        Some(RangeRule {
            min: Some(0.0),
            max: Some(10000.0)
        })
    );
    let total = field(&f, "total");
    assert!(total.read_only);
    assert_eq!(
        total.calc.as_ref().map(|c| (c.op, c.fields.clone())),
        Some((CalcOp::Product, vec!["prix".into(), "quantite".into()]))
    );
    assert_eq!(
        field(&f, "remise").format,
        Some(FieldFormat::Percent {
            decimals: 1,
            sep_style: 3
        })
    );
    assert_eq!(
        field(&f, "date").format,
        Some(FieldFormat::Date {
            format: "dd/mm/yyyy".into()
        })
    );
    assert_eq!(field(&f, "tel").format, Some(FieldFormat::Special { kind: 2 }));
    assert!(field(&f, "perso").custom_script && field(&f, "perso").calc.is_none());
    assert!(!field(&f, "prix").custom_script);
}

#[test]
fn calculation_and_formatted_appearance() {
    let e = engine();
    let path = scratch("scripts-calc").with_file_name("scripts.pdf");
    scripted_form(&path);
    let info = e.open(&path, None).unwrap();
    e.edit(info.id, EditRequest::Load).unwrap();
    e.edit(info.id, EditRequest::Apply(vec![set("prix", &["12.5"])])).unwrap();
    let st = e.edit(info.id, EditRequest::Apply(vec![set("quantite", &["3"])])).unwrap();
    assert_eq!(field(&st.fields, "total").value, ["37.5"]);
    // Le calcul fait partie du même pas d'annulation que la saisie.
    let st = e.edit(info.id, EditRequest::Undo).unwrap();
    assert_eq!(field(&st.fields, "quantite").value, [""]);
    assert_eq!(field(&st.fields, "total").value, ["0"]);
    let st = e.edit(info.id, EditRequest::Redo).unwrap();
    assert_eq!(field(&st.fields, "total").value, ["37.5"]);
    // Un champ calculé en lecture seule refuse la saisie directe.
    let st = e.edit(info.id, EditRequest::Apply(vec![set("total", &["1"])])).unwrap();
    assert_eq!(field(&st.fields, "total").value, ["37.5"]);

    e.edit(
        info.id,
        EditRequest::Apply(vec![
            set("remise", &["0.155"]),
            set("tel", &["0612345678"]),
            set("date", &["02/10/2026"]),
        ]),
    )
    .unwrap();
    e.save(info.id, None).unwrap();
    qpdf_check(&path);
    // Apparences mises en forme (« € » = 0x80 en WinAnsi).
    assert!(contains(&appearance_of(&path, "total"), b"(37,50 \x80)"));
    assert!(contains(&appearance_of(&path, "prix"), b"(12,50 \x80)"));
    assert!(contains(&appearance_of(&path, "remise"), b"(15,5%)"));
    assert!(contains(&appearance_of(&path, "tel"), b"(\\(061\\) 234-5678)"));
    assert!(contains(&appearance_of(&path, "date"), b"(02/10/2026)"));
    // Valeurs brutes dans /V.
    let again = e.open(&path, None).unwrap();
    let f = e.edit(again.id, EditRequest::Load).unwrap().fields;
    assert_eq!(field(&f, "prix").value, ["12.5"]);
    assert_eq!(field(&f, "total").value, ["37.5"]);
}
