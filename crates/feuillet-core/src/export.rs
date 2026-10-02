//! Export (planche 10) : PDF ou PDF aplati, pages choisies, images recompressées, mot de passe et
//! autorisations ; ou une image par page. Le fichier exporté est réécrit en entier (objets
//! inutilisés retirés), sans toucher au document ouvert.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    /// Annotations et champs restent modifiables.
    Pdf,
    /// Annotations et champs fusionnés dans la page.
    Flattened,
    Png,
    Jpg,
}

/// Qualité des images : recompression des images du PDF, ou qualité JPEG d'un export en images.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    /// Fichier le plus léger.
    Light,
    Balanced,
    /// Images d'origine.
    Max,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportProtection {
    pub password: String,
    pub allow_print: bool,
    pub allow_copy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub quality: Quality,
    /// Pages exportées (indices) ; `None` : toutes.
    pub pages: Option<Vec<u32>>,
    /// Résolution d'un export en images (points par pouce).
    pub dpi: u32,
    pub protection: Option<ExportProtection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    /// Fichiers écrits (vide pour une estimation).
    pub files: Vec<String>,
    /// Taille totale en octets.
    #[specta(type = f64)]
    pub size: u64,
}

/// Plus grand côté (px) et qualité JPEG des images recompressées.
fn image_limits(q: Quality) -> Option<(u32, u8)> {
    match q {
        Quality::Light => Some((1400, 60)),
        Quality::Balanced => Some((2200, 78)),
        Quality::Max => None,
    }
}

/// Qualité JPEG d'un export en images.
pub fn jpeg_quality(q: Quality) -> u8 {
    match q {
        Quality::Light => 60,
        Quality::Balanced => 82,
        Quality::Max => 95,
    }
}

fn invalid(e: impl std::fmt::Display) -> Error {
    Error::Invalid(e.to_string())
}

/// Finalise un PDF exporté : objets inutilisés retirés, images recompressées, flux compressés,
/// chiffrement éventuel. `bytes` : document en clair (ou chiffré par `password`).
/// `flattened` : les champs ont été fusionnés dans les pages, l'AcroForm est retiré.
pub fn finish_pdf(
    bytes: &[u8],
    password: Option<&str>,
    quality: Quality,
    protection: Option<&ExportProtection>,
    flattened: bool,
) -> Result<Vec<u8>> {
    let opts = lopdf::LoadOptions {
        password: password.map(str::to_owned),
        ..Default::default()
    };
    let mut doc = Document::load_mem_with_options(bytes, opts).map_err(invalid)?;
    // Document chargé déchiffré : on l'écrit en clair (ou rechiffré ci-dessous).
    doc.encryption_state = None;
    doc.trailer.remove(b"Encrypt");
    if flattened && let Ok(cat) = doc.catalog_mut() {
        cat.remove(b"AcroForm");
    }
    doc.prune_objects();
    doc.renumber_objects();
    if let Some((max, q)) = image_limits(quality) {
        recompress_images(&mut doc, max, q);
    }
    doc.compress();
    if let Some(p) = protection {
        encrypt(&mut doc, p)?;
    }
    let mut out = vec![];
    doc.save_to(&mut out).map_err(invalid)?;
    Ok(out)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    let _ = getrandom::getrandom(&mut b);
    b
}

/// Chiffrement AES-256 (révision 6) : mot de passe d'ouverture, mot de passe propriétaire
/// aléatoire pour que les autorisations s'appliquent.
fn encrypt(doc: &mut Document, p: &ExportProtection) -> Result<()> {
    if p.password.is_empty() {
        return Err(Error::Invalid("mot de passe vide".into()));
    }
    if !doc.trailer.has(b"ID") {
        let id = Object::String(random_bytes::<16>().to_vec(), lopdf::StringFormat::Hexadecimal);
        doc.trailer.set("ID", vec![id.clone(), id]);
    }
    // L'extraction pour l'accessibilité (lecteurs d'écran) reste toujours permise.
    let mut perms = Permissions::MODIFIABLE
        | Permissions::ANNOTABLE
        | Permissions::FILLABLE
        | Permissions::ASSEMBLABLE
        | Permissions::COPYABLE_FOR_ACCESSIBILITY;
    if p.allow_print {
        perms |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
    }
    if p.allow_copy {
        perms |= Permissions::COPYABLE;
    }
    let owner: String = random_bytes::<24>().iter().map(|b| format!("{b:02x}")).collect();
    let key = random_bytes::<32>();
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: &owner,
        user_password: &p.password,
        permissions: perms,
    };
    let state = EncryptionState::try_from(version).map_err(invalid)?;
    doc.encrypt(&state).map_err(invalid)?;
    // Longueur de clé attendue par les lecteurs (lopdf ne l'écrit pas en V5).
    if let Ok(id) = doc.trailer.get(b"Encrypt").and_then(|e| e.as_reference())
        && let Ok(d) = doc.get_dictionary_mut(id)
    {
        d.set("Length", 256);
    }
    // Le chiffrement allonge les flux (vecteur d'initialisation, bourrage) sans mettre à jour
    // leur /Length.
    for obj in doc.objects.values_mut() {
        if let Object::Stream(s) = obj {
            let len = s.content.len() as i64;
            s.dict.set("Length", len);
        }
    }
    Ok(())
}

/// Images RVB ou en niveaux de gris (JPEG, ou Flate 8 bits) réduites et réencodées en JPEG
/// quand le résultat est plus léger. Les autres (masques, CMJN, palettes, JBIG2, JPX) restent.
fn recompress_images(doc: &mut Document, max: u32, quality: u8) {
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| matches!(o, Object::Stream(s) if s.dict.get(b"Subtype").and_then(|v| v.as_name()).ok() == Some(b"Image".as_slice())))
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        let Some(Object::Stream(s)) = doc.objects.get(&id) else {
            continue;
        };
        if let Some(new) = recompress_one(doc, s, max, quality) {
            doc.objects.insert(id, Object::Stream(new));
        }
    }
}

fn recompress_one(doc: &Document, s: &Stream, max: u32, quality: u8) -> Option<Stream> {
    let d = &s.dict;
    if d.get(b"ImageMask").and_then(|v| v.as_bool()).unwrap_or(false) || d.has(b"Decode") {
        return None;
    }
    let filters: Vec<Vec<u8>> = match d.get(b"Filter") {
        Ok(Object::Name(n)) => vec![n.clone()],
        Ok(Object::Array(a)) => a.iter().filter_map(|f| f.as_name().ok().map(<[u8]>::to_vec)).collect(),
        Err(_) => vec![],
        _ => return None,
    };
    let cs = match d.get(b"ColorSpace") {
        Ok(Object::Reference(r)) => doc.get_object(*r).ok()?.clone(),
        Ok(o) => o.clone(),
        Err(_) => return None,
    };
    let gray = match cs.as_name().ok()? {
        b"DeviceRGB" => false,
        b"DeviceGray" => true,
        _ => return None,
    };
    let (w, h) = (
        d.get(b"Width").ok()?.as_i64().ok()? as u32,
        d.get(b"Height").ok()?.as_i64().ok()? as u32,
    );
    let img: image::DynamicImage = match filters.as_slice() {
        [f] if f == b"DCTDecode" => image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?,
        [f] if f == b"FlateDecode" => {
            if d.get(b"BitsPerComponent").ok()?.as_i64().ok()? != 8 || d.has(b"DecodeParms") {
                return None;
            }
            let raw = s.decompressed_content().ok()?;
            if gray {
                image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, raw)?)
            } else {
                image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, raw)?)
            }
        }
        _ => return None,
    };
    let scaled = if w.max(h) > max {
        img.resize(max, max, image::imageops::FilterType::Lanczos3)
    } else if filters.first().is_some_and(|f| f == b"DCTDecode") {
        return None; // JPEG déjà assez petit : pas de perte supplémentaire
    } else {
        img
    };
    let scaled = if gray {
        image::DynamicImage::ImageLuma8(scaled.to_luma8())
    } else {
        image::DynamicImage::ImageRgb8(scaled.to_rgb8())
    };
    let mut jpeg = vec![];
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), quality);
    scaled.write_with_encoder(enc).ok()?;
    if jpeg.len() >= s.content.len() {
        return None;
    }
    let mut nd = d.clone();
    nd.set("Filter", "DCTDecode");
    nd.set("Width", scaled.width() as i64);
    nd.set("Height", scaled.height() as i64);
    nd.set("BitsPerComponent", 8);
    nd.remove(b"DecodeParms");
    let mut ns = Stream::new(nd, jpeg);
    ns.allows_compression = false;
    Some(ns)
}

/// Pages « 1-3, 5, 8- » → indices (0…n-1), dans l'ordre, sans doublon ; `None` si invalide.
pub fn parse_range(spec: &str, count: u32) -> Option<Vec<u32>> {
    let mut out: Vec<u32> = vec![];
    for part in spec.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once(['-', '–']) {
            Some((a, b)) => {
                let a = if a.trim().is_empty() { 1 } else { a.trim().parse().ok()? };
                let b = if b.trim().is_empty() { count } else { b.trim().parse().ok()? };
                (a, b)
            }
            None => {
                let n: u32 = part.parse().ok()?;
                (n, n)
            }
        };
        if a == 0 || b == 0 || a > b || b > count {
            return None;
        }
        for p in a..=b {
            if !out.contains(&(p - 1)) {
                out.push(p - 1);
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Nom du fichier de la page `i` d'un export en images : `base` pour une seule page, sinon
/// « base-01.png ».
pub fn image_path(base: &std::path::Path, i: usize, total: usize, ext: &str) -> std::path::PathBuf {
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "page".into());
    let dir = base.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    if total == 1 {
        return dir.join(format!("{stem}.{ext}"));
    }
    let width = total.to_string().len().max(2);
    dir.join(format!("{stem}-{:0width$}.{ext}", i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_ranges() {
        assert_eq!(parse_range("1-3, 5", 10), Some(vec![0, 1, 2, 4]));
        assert_eq!(parse_range("8-", 10), Some(vec![7, 8, 9]));
        assert_eq!(parse_range("-2", 10), Some(vec![0, 1]));
        assert_eq!(parse_range("3,3,1", 10), Some(vec![2, 0]));
        assert_eq!(parse_range("0", 10), None);
        assert_eq!(parse_range("4-2", 10), None);
        assert_eq!(parse_range("11", 10), None);
        assert_eq!(parse_range("a", 10), None);
        assert_eq!(parse_range(" ", 10), None);
    }

    #[test]
    fn image_names() {
        let b = std::path::Path::new("/tmp/Contrat.png");
        assert_eq!(image_path(b, 0, 1, "png"), std::path::PathBuf::from("/tmp/Contrat.png"));
        assert_eq!(image_path(b, 2, 12, "png"), std::path::PathBuf::from("/tmp/Contrat-03.png"));
        assert_eq!(image_path(b, 4, 320, "jpg"), std::path::PathBuf::from("/tmp/Contrat-005.jpg"));
    }
}
