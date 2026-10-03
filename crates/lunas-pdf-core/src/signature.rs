//! Vérification des signatures numériques (`/ByteRange` + CMS détaché).
//!
//! Pour chaque champ de signature : plage signée, empreinte du contenu, signature CMS (RSA
//! PKCS#1 v1.5, RSA-PSS, ECDSA P-256 / P-384), chaîne de certificats jusqu'au magasin du
//! système, jeton d'horodatage. Lecture seule : rien n'est écrit.

use std::sync::OnceLock;

use cms::content_info::ContentInfo;
use cms::signed_data::{SignedData, SignerIdentifier, SignerInfo};
use der::asn1::{ObjectIdentifier as Oid, OctetString};
use der::{Decode, Encode, SliceReader};
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use specta::Type;
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::Certificate;

use crate::types::Rect;
use crate::writer::page_transform;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SigStatus {
    /// Intacte et certificat reconnu.
    Valid,
    /// Le contenu signé a été modifié, ou la signature est fausse.
    Invalid,
    /// Intacte, mais le certificat n'est pas reconnu par ce système (ou hors validité).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CertInfo {
    pub subject: String,
    /// Nom commun et organisation du titulaire.
    pub common_name: Option<String>,
    pub organization: Option<String>,
    pub issuer: String,
    pub issuer_name: Option<String>,
    /// Validité (secondes Unix).
    pub not_before: f64,
    pub not_after: f64,
    pub serial: String,
    /// Empreinte SHA-256 (« 4F:2A:… »).
    pub sha256: String,
    /// Certificats de la chaîne, du signataire vers la racine (noms communs).
    pub chain: Vec<String>,
    /// Haut de la chaîne (racine, ou dernier certificat trouvé) : autorité à approuver.
    pub root_name: String,
    pub root_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    /// Nom du champ de signature.
    pub field: String,
    /// Emplacement visible (absent pour une signature invisible).
    pub page: Option<u32>,
    pub rect: Option<Rect>,
    pub status: SigStatus,
    /// Nom du signataire (certificat, sinon `/Name`).
    pub signer: Option<String>,
    pub organization: Option<String>,
    /// Date de signature (secondes Unix) et décalage horaire déclaré, en minutes.
    pub signed_at: Option<f64>,
    pub utc_offset: Option<i32>,
    pub reason: Option<String>,
    pub location: Option<String>,
    /// Empreinte et signature correctes.
    pub intact: bool,
    /// La signature couvre tout le fichier (sinon : révisions ajoutées après signature).
    pub covers_whole: bool,
    /// Chaîne de certificats jusqu'à une autorité reconnue.
    pub trusted: bool,
    /// Origine de cette reconnaissance : « system », « user », « eu:IT », « microsoft ».
    pub trust_source: Option<String>,
    /// Certificat valide à la date de signature.
    pub cert_valid_at_signing: bool,
    /// Horodatage : date (secondes Unix) et vérification de son jeton.
    pub timestamp: Option<f64>,
    pub timestamp_verified: bool,
    /// Autorité d'horodatage reconnue par le système.
    pub timestamp_trusted: bool,
    pub certificate: Option<CertInfo>,
    pub sub_filter: Option<String>,
    /// Cause d'un échec de vérification.
    pub problem: Option<String>,
}

const OID_SIGNED_DATA: Oid = Oid::new_unwrap("1.2.840.113549.1.7.2");
const OID_MESSAGE_DIGEST: Oid = Oid::new_unwrap("1.2.840.113549.1.9.4");
const OID_SIGNING_TIME: Oid = Oid::new_unwrap("1.2.840.113549.1.9.5");
const OID_TIMESTAMP_TOKEN: Oid = Oid::new_unwrap("1.2.840.113549.1.9.16.2.14");
const OID_CN: Oid = Oid::new_unwrap("2.5.4.3");
const OID_O: Oid = Oid::new_unwrap("2.5.4.10");
const OID_SKI: Oid = Oid::new_unwrap("2.5.29.14");

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hash {
    Sha1,
    Sha256,
    Sha384,
    Sha512,
}

impl Hash {
    fn from_digest_oid(o: &Oid) -> Option<Hash> {
        Some(match o.to_string().as_str() {
            "1.3.14.3.2.26" => Hash::Sha1,
            "2.16.840.1.101.3.4.2.1" => Hash::Sha256,
            "2.16.840.1.101.3.4.2.2" => Hash::Sha384,
            "2.16.840.1.101.3.4.2.3" => Hash::Sha512,
            _ => return None,
        })
    }
    fn digest(self, parts: &[&[u8]]) -> Vec<u8> {
        fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
            let mut h = D::new();
            for p in parts {
                h.update(p);
            }
            h.finalize().to_vec()
        }
        match self {
            Hash::Sha1 => run::<sha1::Sha1>(parts),
            Hash::Sha256 => run::<sha2::Sha256>(parts),
            Hash::Sha384 => run::<sha2::Sha384>(parts),
            Hash::Sha512 => run::<sha2::Sha512>(parts),
        }
    }
}

/// Racines de confiance du système (chargées une fois).
pub fn system_roots() -> &'static [Certificate] {
    static ROOTS: OnceLock<Vec<Certificate>> = OnceLock::new();
    ROOTS.get_or_init(|| {
        rustls_native_certs::load_native_certs()
            .certs
            .iter()
            .filter_map(|c| Certificate::from_der(c.as_ref()).ok())
            .collect()
    })
}

/// Autorité de confiance : certificat, origine, périodes de reconnaissance (vide : toujours).
pub struct TrustAnchor {
    pub cert: Certificate,
    pub source: String,
    pub periods: Vec<(i64, Option<i64>)>,
}

/// Autorités reconnues : magasin du système, autorités approuvées, listes de confiance.
#[derive(Default)]
pub struct TrustStore {
    pub anchors: Vec<TrustAnchor>,
}

impl TrustStore {
    /// Certificats reconnus en permanence, d'une même origine.
    pub fn of(certs: &[Certificate], source: &str) -> TrustStore {
        TrustStore {
            anchors: certs
                .iter()
                .map(|c| TrustAnchor {
                    cert: c.clone(),
                    source: source.into(),
                    periods: vec![],
                })
                .collect(),
        }
    }

    /// Système + autorités approuvées (DER) + listes de confiance.
    pub fn new(user: &[Vec<u8>], lists: &crate::trust_lists::Bundle) -> TrustStore {
        let mut s = TrustStore::of(system_roots(), "system");
        s.anchors.extend(
            user.iter()
                .filter_map(|d| Certificate::from_der(d).ok())
                .map(|cert| TrustAnchor {
                    cert,
                    source: "user".into(),
                    periods: vec![],
                }),
        );
        s.anchors.extend(lists.anchors.iter().filter_map(|a| {
            Some(TrustAnchor {
                cert: Certificate::from_der(&a.der_bytes()?).ok()?,
                source: a.source.clone(),
                periods: a.periods.clone(),
            })
        }));
        s
    }

    fn at(&self, t: i64) -> impl Iterator<Item = &TrustAnchor> {
        self.anchors
            .iter()
            .filter(move |a| a.periods.is_empty() || a.periods.iter().any(|(s, e)| t >= *s && e.is_none_or(|e| t < e)))
    }
}

/// Magasin par défaut (système, autorités approuvées, listes courantes), gardé en cache.
pub fn default_store(user: &[Vec<u8>]) -> std::sync::Arc<TrustStore> {
    use std::sync::{Arc, Mutex};
    static CACHE: Mutex<Option<(u64, Arc<TrustStore>)>> = Mutex::new(None);
    let lists = crate::trust_lists::current();
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        lists.generated.hash(&mut h);
        lists.anchors.len().hash(&mut h);
        user.hash(&mut h);
        h.finish()
    };
    let mut c = CACHE.lock().unwrap();
    if let Some((k, s)) = c.as_ref()
        && *k == key
    {
        return s.clone();
    }
    let s = Arc::new(TrustStore::new(user, &lists));
    *c = Some((key, s.clone()));
    s
}

/// Vérifie toutes les signatures d'un fichier avec les autorités de `store`.
pub fn verify_document(bytes: &[u8], password: Option<&str>, store: &TrustStore) -> Vec<SignatureInfo> {
    let opts = lopdf::LoadOptions {
        password: password.map(str::to_owned),
        ..Default::default()
    };
    let Ok(doc) = Document::load_mem_with_options(bytes, opts) else {
        return vec![];
    };
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let mut out = vec![];
    for (name, field, widget) in signature_fields(&doc) {
        let Some(v) = field.get(b"V").ok().and_then(|v| deref_dict(&doc, v)) else {
            continue; // champ vide (pas encore signé)
        };
        let (page, rect) = widget_place(&doc, &pages, &widget);
        out.push(verify_one(bytes, &name, v, page, rect, store));
    }
    out
}

fn deref_dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match o {
        Object::Reference(r) => doc.get_dictionary(*r).ok(),
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn text(d: &Dictionary, key: &[u8]) -> Option<String> {
    d.get(key)
        .ok()
        .and_then(|o| lopdf::decode_text_string(o).ok())
        .filter(|s| !s.is_empty())
}

/// Champs de signature : (nom complet, dictionnaire du champ, dictionnaire du widget).
fn signature_fields(doc: &Document) -> Vec<(String, Dictionary, Dictionary)> {
    let mut out = vec![];
    let Some(af) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .and_then(|a| deref_dict(doc, a))
    else {
        return out;
    };
    let Ok(Object::Array(fields)) = af.get(b"Fields").map(|f| match f {
        Object::Reference(r) => doc.get_object(*r).unwrap_or(f),
        o => o,
    }) else {
        return out;
    };
    fn walk(
        doc: &Document,
        o: &Object,
        parent: &str,
        ft: Option<Vec<u8>>,
        out: &mut Vec<(String, Dictionary, Dictionary)>,
        depth: u32,
    ) {
        let Some(d) = deref_dict(doc, o) else { return };
        if depth > 32 {
            return;
        }
        let name = match text(d, b"T") {
            Some(t) if parent.is_empty() => t,
            Some(t) => format!("{parent}.{t}"),
            None => parent.to_string(),
        };
        let ft = d.get(b"FT").and_then(|f| f.as_name()).ok().map(<[u8]>::to_vec).or(ft);
        let kids: Vec<Object> = match d.get(b"Kids") {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(r)) => doc
                .get_object(*r)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => vec![],
        };
        let field_kids: Vec<&Object> = kids
            .iter()
            .filter(|k| deref_dict(doc, k).is_some_and(|kd| kd.has(b"T")))
            .collect();
        for k in &field_kids {
            walk(doc, k, &name, ft.clone(), out, depth + 1);
        }
        if ft.as_deref() != Some(b"Sig") || !field_kids.is_empty() {
            return;
        }
        let widget = kids.first().and_then(|k| deref_dict(doc, k)).unwrap_or(d).clone();
        out.push((name, d.clone(), widget));
    }
    for f in fields {
        walk(doc, f, "", None, &mut out, 0);
    }
    out
}

fn widget_place(doc: &Document, pages: &[ObjectId], w: &Dictionary) -> (Option<u32>, Option<Rect>) {
    let page = w
        .get(b"P")
        .and_then(|p| p.as_reference())
        .ok()
        .and_then(|p| pages.iter().position(|id| *id == p))
        .or_else(|| {
            // Sans `/P` : recherche du widget dans les `/Annots` des pages.
            pages.iter().position(|pid| {
                let Ok(pd) = doc.get_dictionary(*pid) else { return false };
                let annots = match pd.get(b"Annots") {
                    Ok(Object::Array(a)) => a.clone(),
                    Ok(Object::Reference(r)) => doc
                        .get_object(*r)
                        .ok()
                        .and_then(|o| o.as_array().ok())
                        .cloned()
                        .unwrap_or_default(),
                    _ => vec![],
                };
                annots.iter().any(|a| deref_dict(doc, a).is_some_and(|d| d == w))
            })
        });
    let r: Vec<f32> = match w.get(b"Rect") {
        Ok(Object::Array(a)) => a.iter().filter_map(|v| v.as_float().ok()).collect(),
        _ => vec![],
    };
    let Some(p) = page else { return (None, None) };
    if r.len() != 4 || (r[2] - r[0]).abs() < 1.0 || (r[3] - r[1]).abs() < 1.0 {
        return (Some(p as u32), None); // signature invisible
    }
    let t = page_transform(doc, pages[p]);
    (
        Some(p as u32),
        Some(t.rect(r[0].min(r[2]), r[1].min(r[3]), r[0].max(r[2]), r[1].max(r[3]))),
    )
}

fn verify_one(
    bytes: &[u8],
    field: &str,
    v: &Dictionary,
    page: Option<u32>,
    rect: Option<Rect>,
    store: &TrustStore,
) -> SignatureInfo {
    let (pdf_time, pdf_offset) = text(v, b"M").and_then(|m| parse_pdf_date(&m)).unzip();
    let mut info = SignatureInfo {
        field: field.into(),
        page,
        rect,
        status: SigStatus::Invalid,
        signer: text(v, b"Name"),
        organization: None,
        signed_at: pdf_time.map(|t| t as f64),
        utc_offset: pdf_offset.flatten(),
        reason: text(v, b"Reason"),
        location: text(v, b"Location"),
        intact: false,
        covers_whole: false,
        trusted: false,
        trust_source: None,
        cert_valid_at_signing: false,
        timestamp: None,
        timestamp_verified: false,
        timestamp_trusted: false,
        certificate: None,
        sub_filter: v
            .get(b"SubFilter")
            .and_then(|s| s.as_name())
            .ok()
            .map(|s| String::from_utf8_lossy(s).into_owned()),
        problem: None,
    };
    if let Err(p) = check(bytes, v, store, &mut info) {
        info.problem = Some(p.into());
    }
    // Invalide seulement si la vérification a pu conclure à une modification ; un échec de
    // lecture ou un algorithme inconnu laisse la signature « non vérifiable ».
    let modified = matches!(info.problem.as_deref(), Some("modified" | "byteRange"));
    info.status = if !info.intact && modified {
        SigStatus::Invalid
    } else if !info.intact {
        SigStatus::Unknown
    } else if info.trusted && info.cert_valid_at_signing {
        SigStatus::Valid
    } else {
        SigStatus::Unknown
    };
    info
}

/// Plage signée `(a, b, c, d)` et données CMS (réencodées en DER) d'une signature.
/// Bornes `/ByteRange` : début et longueur des deux parties signées.
type ByteRange = (usize, usize, usize, usize);

fn signed_range(bytes: &[u8], v: &Dictionary) -> Result<(ByteRange, SignedData), &'static str> {
    let br: Vec<i64> = match v.get(b"ByteRange") {
        Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_i64().ok()).collect(),
        _ => return Err("byteRange"),
    };
    let [a, b, c, d] = br[..] else { return Err("byteRange") };
    let (a, b, c, d) = (a as usize, b as usize, c as usize, d as usize);
    if a != 0 || a + b >= c || c + d > bytes.len() || bytes.get(a + b) != Some(&b'<') || bytes.get(c - 1) != Some(&b'>') {
        return Err("byteRange");
    }
    // `/Contents` lu directement dans le fichier (jamais chiffré, voir ISO 32000 § 7.6.1).
    let hex: Vec<u8> = bytes[a + b + 1..c - 1]
        .iter()
        .copied()
        .filter(|x| x.is_ascii_hexdigit())
        .collect();
    let der: Vec<u8> = hex
        .chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap_or("00"), 16).unwrap_or(0))
        .collect();
    // CMS autorise le BER (longueurs indéfinies : Dropbox Sign, certains outils Java) ; le
    // décodeur n'accepte que le DER.
    let der = ber_to_der(&der).ok_or("cms")?;
    let mut reader = SliceReader::new(&der).map_err(|_| "cms")?;
    let ci = ContentInfo::decode(&mut reader).map_err(|_| "cms")?;
    if ci.content_type != OID_SIGNED_DATA {
        return Err("cms");
    }
    let sd: SignedData = ci.content.decode_as().map_err(|_| "cms")?;
    Ok(((a, b, c, d), sd))
}

fn embedded_certs(sd: &SignedData) -> Vec<Certificate> {
    sd.certificates
        .iter()
        .flat_map(|s| s.0.iter())
        .filter_map(|c| match c {
            cms::cert::CertificateChoices::Certificate(c) => Some(c.clone()),
            _ => None,
        })
        .collect()
}

fn check(bytes: &[u8], v: &Dictionary, store: &TrustStore, info: &mut SignatureInfo) -> Result<(), &'static str> {
    let ((a, b, c, d), sd) = signed_range(bytes, v)?;
    info.covers_whole = c + d == bytes.len() || bytes[c + d..].iter().all(|x| x.is_ascii_whitespace());
    let si = sd.signer_infos.0.iter().next().ok_or("cms")?;
    let certs = embedded_certs(&sd);
    let signer = find_signer(si, &certs).ok_or("noCertificate")?;

    // Date de signature : attribut signé, sinon `/M`.
    if let Some(t) = attr(si.signed_attrs.as_ref(), OID_SIGNING_TIME)
        .and_then(|a| a.to_der().ok().and_then(|d| x509_cert::time::Time::from_der(&d).ok()))
    {
        info.signed_at = Some(t.to_unix_duration().as_secs() as f64);
    }
    let cert_info = cert_info(signer);
    info.signer = cert_info.common_name.clone().or(info.signer.take());
    info.organization = cert_info.organization.clone();

    let hash = Hash::from_digest_oid(&si.digest_alg.oid).ok_or("algorithm")?;
    let content = hash.digest(&[&bytes[a..a + b], &bytes[c..c + d]]);
    if info.sub_filter.as_deref() == Some("adbe.pkcs7.sha1") {
        return Err("algorithm");
    }
    info.intact = verify_signer(si, &content, hash, signer)?;
    if !info.intact {
        return Err("modified");
    }

    // Horodatage (attribut non signé) : empreinte de la valeur de signature.
    if let Some(tok) = attr(si.unsigned_attrs.as_ref(), OID_TIMESTAMP_TOKEN)
        && let Some((time, ok, trusted)) = check_timestamp(&tok.to_der().unwrap_or_default(), si.signature.as_bytes(), store)
    {
        info.timestamp = Some(time as f64);
        info.timestamp_verified = ok;
        info.timestamp_trusted = ok && trusted;
    }

    // Confiance évaluée à la date de signature (horodatage reconnu, sinon date déclarée).
    let at = info
        .timestamp
        .filter(|_| info.timestamp_trusted)
        .or(info.signed_at)
        .unwrap_or_else(|| now() as f64);
    let (chain, source) = build_chain(signer, &certs, Some((store, at as i64)));
    let trusted = source.is_some();
    info.trusted = trusted;
    info.trust_source = source;
    info.cert_valid_at_signing = (cert_info.not_before..=cert_info.not_after).contains(&at);
    let top = chain.last().unwrap_or(signer);
    info.certificate = Some(CertInfo {
        chain: chain
            .iter()
            .map(|c| common_name(&c.tbs_certificate.subject).unwrap_or_else(|| c.tbs_certificate.subject.to_string()))
            .collect(),
        root_name: common_name(&top.tbs_certificate.subject).unwrap_or_else(|| top.tbs_certificate.subject.to_string()),
        root_sha256: cert_info_fingerprint(top),
        ..cert_info
    });
    if !trusted {
        info.problem = Some("untrusted".into());
    } else if !info.cert_valid_at_signing {
        info.problem = Some("certificateExpired".into());
    }
    Ok(())
}

/// Réencode un élément BER en DER : longueurs définies, chaînes d'octets construites
/// fusionnées. Un élément déjà en DER ressort à l'identique ; les octets après le premier
/// élément (bourrage de `/Contents`) sont ignorés.
pub fn ber_to_der(input: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;
    let mut out = Vec::with_capacity(input.len());
    ber_element(input, &mut pos, &mut out, 0)?;
    Some(out)
}

fn ber_element(b: &[u8], pos: &mut usize, out: &mut Vec<u8>, depth: u32) -> Option<()> {
    if depth > 64 {
        return None;
    }
    let tag_start = *pos;
    let first = *b.get(*pos)?;
    *pos += 1;
    if first & 0x1f == 0x1f {
        while b.get(*pos)? & 0x80 != 0 {
            *pos += 1;
        }
        *pos += 1;
    }
    let tag = &b[tag_start..*pos];
    let constructed = first & 0x20 != 0;
    let len_byte = *b.get(*pos)?;
    *pos += 1;
    let mut content = vec![];
    if len_byte == 0x80 {
        // Longueur indéfinie : éléments jusqu'au marqueur de fin 00 00.
        if !constructed {
            return None;
        }
        while b.get(*pos..*pos + 2)? != [0, 0] {
            ber_element(b, pos, &mut content, depth + 1)?;
        }
        *pos += 2;
    } else {
        let len = if len_byte & 0x80 == 0 {
            len_byte as usize
        } else {
            let n = (len_byte & 0x7f) as usize;
            if n == 0 || n > 4 {
                return None;
            }
            let mut l = 0usize;
            for _ in 0..n {
                l = (l << 8) | *b.get(*pos)? as usize;
                *pos += 1;
            }
            l
        };
        let end = pos.checked_add(len)?;
        let body = b.get(*pos..end)?;
        if constructed {
            let mut p = 0;
            while p < body.len() {
                ber_element(body, &mut p, &mut content, depth + 1)?;
            }
        } else {
            content.extend_from_slice(body);
        }
        *pos = end;
    }
    if first == 0x24 {
        // OCTET STRING construite : concaténation des morceaux (déjà réencodés).
        let mut data = vec![];
        let mut p = 0;
        while p < content.len() {
            let mut piece = vec![];
            ber_element(&content, &mut p, &mut piece, depth + 1)?;
            let (hdr, _) = header_len(&piece)?;
            data.extend_from_slice(&piece[hdr..]);
        }
        out.push(0x04);
        push_len(out, data.len());
        out.extend_from_slice(&data);
        return Some(());
    }
    out.extend_from_slice(tag);
    push_len(out, content.len());
    out.extend_from_slice(&content);
    Some(())
}

/// Taille de l'en-tête (étiquette courte + longueur) d'un élément DER.
fn header_len(e: &[u8]) -> Option<(usize, usize)> {
    let l = *e.get(1)?;
    if l & 0x80 == 0 {
        Some((2, l as usize))
    } else {
        let n = (l & 0x7f) as usize;
        let len = e.get(2..2 + n)?.iter().fold(0usize, |a, x| (a << 8) | *x as usize);
        Some((2 + n, len))
    }
}

fn push_len(out: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len.to_be_bytes().into_iter().skip_while(|x| *x == 0).collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend_from_slice(&bytes);
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn attr(attrs: Option<&x509_cert::attr::Attributes>, oid: Oid) -> Option<der::Any> {
    attrs?.iter().find(|a| a.oid == oid)?.values.iter().next().cloned()
}

fn find_signer<'a>(si: &SignerInfo, certs: &'a [Certificate]) -> Option<&'a Certificate> {
    certs.iter().find(|c| match &si.sid {
        SignerIdentifier::IssuerAndSerialNumber(ias) => {
            c.tbs_certificate.issuer == ias.issuer && c.tbs_certificate.serial_number == ias.serial_number
        }
        SignerIdentifier::SubjectKeyIdentifier(ski) => subject_key_id(c).as_deref() == Some(ski.0.as_bytes()),
    })
}

fn subject_key_id(c: &Certificate) -> Option<Vec<u8>> {
    let ext = c.tbs_certificate.extensions.as_ref()?.iter().find(|e| e.extn_id == OID_SKI)?;
    let ski = OctetString::from_der(ext.extn_value.as_bytes()).ok()?;
    Some(ski.as_bytes().to_vec())
}

/// Empreinte (attribut `messageDigest`) puis signature des attributs signés.
fn verify_signer(si: &SignerInfo, content_digest: &[u8], hash: Hash, cert: &Certificate) -> Result<bool, &'static str> {
    let sig = si.signature.as_bytes();
    match &si.signed_attrs {
        Some(attrs) => {
            let md = attr(Some(attrs), OID_MESSAGE_DIGEST)
                .and_then(|a| a.decode_as::<OctetString>().ok())
                .ok_or("cms")?;
            if md.as_bytes() != content_digest {
                return Ok(false);
            }
            let signed = attrs.to_der().map_err(|_| "cms")?;
            verify_raw(
                &cert.tbs_certificate.subject_public_key_info,
                &si.signature_algorithm,
                Some(hash),
                &signed,
                sig,
            )
        }
        None => verify_prehashed(
            &cert.tbs_certificate.subject_public_key_info,
            &si.signature_algorithm,
            hash,
            content_digest,
            sig,
        ),
    }
}

/// Signature `sig` sur `message` (haché selon l'algorithme).
fn verify_raw(
    key: &SubjectPublicKeyInfoOwned,
    alg: &AlgorithmIdentifierOwned,
    default_hash: Option<Hash>,
    message: &[u8],
    sig: &[u8],
) -> Result<bool, &'static str> {
    let hash = sig_hash(alg).or(default_hash).ok_or("algorithm")?;
    verify_prehashed(key, alg, hash, &hash.digest(&[message]), sig)
}

/// Hachage imposé par l'algorithme de signature (`sha256WithRSAEncryption`…).
fn sig_hash(alg: &AlgorithmIdentifierOwned) -> Option<Hash> {
    Some(match alg.oid.to_string().as_str() {
        "1.2.840.113549.1.1.5" | "1.2.840.10045.4.1" => Hash::Sha1,
        "1.2.840.113549.1.1.11" | "1.2.840.10045.4.3.2" => Hash::Sha256,
        "1.2.840.113549.1.1.12" | "1.2.840.10045.4.3.3" => Hash::Sha384,
        "1.2.840.113549.1.1.13" | "1.2.840.10045.4.3.4" => Hash::Sha512,
        _ => return None,
    })
}

fn verify_prehashed(
    key: &SubjectPublicKeyInfoOwned,
    alg: &AlgorithmIdentifierOwned,
    hash: Hash,
    hashed: &[u8],
    sig: &[u8],
) -> Result<bool, &'static str> {
    use p256::ecdsa::signature::hazmat::PrehashVerifier;
    use rsa::pkcs8::DecodePublicKey;
    let spki = key.to_der().map_err(|_| "certificate")?;
    let alg_oid = alg.oid.to_string();
    match key.algorithm.oid.to_string().as_str() {
        "1.2.840.113549.1.1.1" | "1.2.840.113549.1.1.10" => {
            let pk = rsa::RsaPublicKey::from_public_key_der(&spki).map_err(|_| "certificate")?;
            let ok = if alg_oid == "1.2.840.113549.1.1.10" {
                match hash {
                    Hash::Sha1 => pk.verify(rsa::Pss::new::<sha1::Sha1>(), hashed, sig),
                    Hash::Sha256 => pk.verify(rsa::Pss::new::<sha2::Sha256>(), hashed, sig),
                    Hash::Sha384 => pk.verify(rsa::Pss::new::<sha2::Sha384>(), hashed, sig),
                    Hash::Sha512 => pk.verify(rsa::Pss::new::<sha2::Sha512>(), hashed, sig),
                }
            } else {
                let scheme = match hash {
                    Hash::Sha1 => rsa::Pkcs1v15Sign::new::<sha1::Sha1>(),
                    Hash::Sha256 => rsa::Pkcs1v15Sign::new::<sha2::Sha256>(),
                    Hash::Sha384 => rsa::Pkcs1v15Sign::new::<sha2::Sha384>(),
                    Hash::Sha512 => rsa::Pkcs1v15Sign::new::<sha2::Sha512>(),
                };
                pk.verify(scheme, hashed, sig)
            };
            Ok(ok.is_ok())
        }
        "1.2.840.10045.2.1" => {
            if let Ok(vk) = p256::ecdsa::VerifyingKey::from_public_key_der(&spki) {
                let s = p256::ecdsa::Signature::from_der(sig).map_err(|_| "cms")?;
                return Ok(vk.verify_prehash(hashed, &s).is_ok());
            }
            if let Ok(vk) = p384::ecdsa::VerifyingKey::from_public_key_der(&spki) {
                let s = p384::ecdsa::Signature::from_der(sig).map_err(|_| "cms")?;
                return Ok(vk.verify_prehash(hashed, &s).is_ok());
            }
            Err("algorithm")
        }
        _ => Err("algorithm"),
    }
}

fn cert_signed_by(cert: &Certificate, issuer: &Certificate) -> bool {
    cert.tbs_certificate.issuer == issuer.tbs_certificate.subject
        && cert.tbs_certificate.to_der().ok().is_some_and(|tbs| {
            verify_raw(
                &issuer.tbs_certificate.subject_public_key_info,
                &cert.signature_algorithm,
                None,
                &tbs,
                cert.signature.raw_bytes(),
            )
            .unwrap_or(false)
        })
}

/// Chaîne du signataire vers une racine reconnue ; vrai si elle aboutit.
/// Même autorité : même titulaire et même clé (un certificat réémis compte).
fn same_cert(a: &Certificate, b: &Certificate) -> bool {
    a.tbs_certificate.subject == b.tbs_certificate.subject
        && a.tbs_certificate.subject_public_key_info == b.tbs_certificate.subject_public_key_info
}

/// Chaîne du signataire ; `Some(origine)` si elle aboutit à une autorité reconnue à la date `t`.
fn build_chain(
    signer: &Certificate,
    pool: &[Certificate],
    store: Option<(&TrustStore, i64)>,
) -> (Vec<Certificate>, Option<String>) {
    let mut chain = vec![signer.clone()];
    for _ in 0..10 {
        let cur = chain.last().unwrap().clone();
        if let Some((store, t)) = store {
            if let Some(a) = store.at(t).find(|a| same_cert(&a.cert, &cur)) {
                return (chain, Some(a.source.clone()));
            }
            if let Some(a) = store.at(t).find(|a| cert_signed_by(&cur, &a.cert)) {
                chain.push(a.cert.clone());
                return (chain, Some(a.source.clone()));
            }
        }
        match pool
            .iter()
            .find(|c| *c != &cur && !chain.contains(c) && cert_signed_by(&cur, c))
        {
            Some(next) => chain.push(next.clone()),
            None => break,
        }
    }
    (chain, None)
}

/// Jeton d'horodatage (RFC 3161) : (date, jeton correct et lié à cette signature, autorité
/// d'horodatage reconnue).
fn check_timestamp(token: &[u8], signature: &[u8], store: &TrustStore) -> Option<(i64, bool, bool)> {
    let ci = ContentInfo::from_der(token).ok()?;
    let sd: SignedData = ci.content.decode_as().ok()?;
    let econtent = sd.encap_content_info.econtent.as_ref()?;
    let tst_der = econtent.decode_as::<OctetString>().ok()?.as_bytes().to_vec();
    let tst = TstInfo::from_der(&tst_der).ok()?;
    let time = tst.gen_time.to_unix_duration().as_secs() as i64;
    let Some(h) = Hash::from_digest_oid(&tst.message_imprint.hash_algorithm.oid) else {
        return Some((time, false, false));
    };
    let imprint_ok = tst.message_imprint.hashed_message.as_bytes() == h.digest(&[signature]);
    let certs = embedded_certs(&sd);
    let ok = (|| {
        let si = sd.signer_infos.0.iter().next()?;
        let cert = find_signer(si, &certs)?;
        let hash = Hash::from_digest_oid(&si.digest_alg.oid)?;
        let sig_ok = verify_signer(si, &hash.digest(&[&tst_der]), hash, cert).ok()?;
        Some((sig_ok, build_chain(cert, &certs, Some((store, time))).1.is_some()))
    })()
    .unwrap_or((false, false));
    Some((time, imprint_ok && ok.0, ok.1))
}

/// `TSTInfo` (RFC 3161), champs utiles seulement.
#[derive(der::Sequence)]
struct TstInfo {
    version: u8,
    policy: Oid,
    message_imprint: MessageImprint,
    serial_number: der::asn1::Int,
    gen_time: der::asn1::GeneralizedTime,
    #[asn1(optional = "true")]
    accuracy: Option<der::Any>,
    #[asn1(optional = "true")]
    ordering: Option<bool>,
    #[asn1(optional = "true")]
    nonce: Option<der::asn1::Int>,
    #[asn1(context_specific = "0", optional = "true", tag_mode = "EXPLICIT")]
    tsa: Option<der::Any>,
    #[asn1(context_specific = "1", optional = "true", tag_mode = "IMPLICIT")]
    extensions: Option<der::Any>,
}

#[derive(der::Sequence)]
struct MessageImprint {
    hash_algorithm: AlgorithmIdentifierOwned,
    hashed_message: OctetString,
}

fn name_attr(n: &x509_cert::name::Name, oid: Oid) -> Option<String> {
    n.0.iter().flat_map(|rdn| rdn.0.iter()).find(|a| a.oid == oid).and_then(|a| {
        a.value
            .decode_as::<der::asn1::Utf8StringRef>()
            .map(|s| s.to_string())
            .or_else(|_| a.value.decode_as::<der::asn1::PrintableStringRef>().map(|s| s.to_string()))
            .or_else(|_| a.value.decode_as::<der::asn1::Ia5StringRef>().map(|s| s.to_string()))
            .or_else(|_| a.value.decode_as::<der::asn1::TeletexStringRef>().map(|s| s.to_string()))
            .ok()
    })
}

fn common_name(n: &x509_cert::name::Name) -> Option<String> {
    name_attr(n, OID_CN)
}

fn cert_info(c: &Certificate) -> CertInfo {
    let t = &c.tbs_certificate;
    let fp = sha2::Sha256::digest(c.to_der().unwrap_or_default());
    CertInfo {
        subject: t.subject.to_string(),
        common_name: common_name(&t.subject),
        organization: name_attr(&t.subject, OID_O),
        issuer: t.issuer.to_string(),
        issuer_name: common_name(&t.issuer).or_else(|| name_attr(&t.issuer, OID_O)),
        not_before: t.validity.not_before.to_unix_duration().as_secs() as f64,
        not_after: t.validity.not_after.to_unix_duration().as_secs() as f64,
        serial: t
            .serial_number
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
        sha256: fp.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":"),
        chain: vec![],
        root_name: String::new(),
        root_sha256: String::new(),
    }
}

fn cert_info_fingerprint(c: &Certificate) -> String {
    sha2::Sha256::digest(c.to_der().unwrap_or_default())
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Racines du système et autorités approuvées par l'utilisateur (DER), sans les listes.
pub fn roots_with(extra: &[Vec<u8>]) -> TrustStore {
    TrustStore::new(extra, &crate::trust_lists::Bundle::default())
}

/// Haut de la chaîne de la signature `field` : (DER, nom), pour l'approuver.
pub fn chain_root(bytes: &[u8], password: Option<&str>, field: &str) -> Option<(Vec<u8>, String)> {
    let opts = lopdf::LoadOptions {
        password: password.map(str::to_owned),
        ..Default::default()
    };
    let doc = Document::load_mem_with_options(bytes, opts).ok()?;
    let (_, f, _) = signature_fields(&doc).into_iter().find(|(n, _, _)| n == field)?;
    let v = f.get(b"V").ok().and_then(|v| deref_dict(&doc, v))?;
    let (_, sd) = signed_range(bytes, v).ok()?;
    let certs = embedded_certs(&sd);
    let signer = find_signer(sd.signer_infos.0.iter().next()?, &certs)?;
    let (chain, _) = build_chain(signer, &certs, None);
    let top = chain.last()?;
    let name = common_name(&top.tbs_certificate.subject).unwrap_or_else(|| top.tbs_certificate.subject.to_string());
    Some((top.to_der().ok()?, name))
}

/// Date PDF « D:AAAAMMJJHHmmSS+HH'mm' » → (secondes Unix, décalage en minutes).
pub fn parse_pdf_date(s: &str) -> Option<(i64, Option<i32>)> {
    let s = s.trim_start_matches("D:");
    let num = |r: std::ops::Range<usize>, def: i64| s.get(r).and_then(|x| x.parse::<i64>().ok()).unwrap_or(def);
    let y = s.get(0..4)?.parse::<i64>().ok()?;
    let (mo, d, h, mi, se) = (num(4..6, 1), num(6..8, 1), num(8..10, 0), num(10..12, 0), num(12..14, 0));
    let tz = s.get(14..).unwrap_or("");
    let offset = match tz.chars().next() {
        Some('Z') => Some(0),
        Some(c @ ('+' | '-')) => {
            let digits: String = tz[1..].chars().filter(char::is_ascii_digit).collect();
            let oh: i32 = digits.get(0..2)?.parse().ok()?;
            let om: i32 = digits.get(2..4).and_then(|m| m.parse().ok()).unwrap_or(0);
            Some(if c == '-' { -(oh * 60 + om) } else { oh * 60 + om })
        }
        _ => None,
    };
    // Jours depuis 1970 (algorithme de Howard Hinnant).
    let (yy, mm) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let local = days * 86_400 + h * 3600 + mi * 60 + se;
    Some((local - offset.unwrap_or(0) as i64 * 60, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ber_indefinite_lengths_become_der() {
        // SEQUENCE indéfinie { INTEGER 5, OCTET STRING construite { "ab", "c" } } + bourrage.
        let ber = [
            0x30, 0x80, 0x02, 0x01, 0x05, 0x24, 0x80, 0x04, 0x02, b'a', b'b', 0x04, 0x01, b'c', 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(
            ber_to_der(&ber).unwrap(),
            [0x30, 0x08, 0x02, 0x01, 0x05, 0x04, 0x03, b'a', b'b', b'c']
        );
        // Du DER ressort identique.
        let der = [0x30, 0x03, 0x02, 0x01, 0x07];
        assert_eq!(ber_to_der(&der).unwrap(), der);
        assert!(ber_to_der(&[0x30, 0x80, 0x02]).is_none());
    }

    #[test]
    fn pdf_dates() {
        assert_eq!(parse_pdf_date("D:20261002093748+02'00'"), Some((1_790_926_668, Some(120))));
        assert_eq!(parse_pdf_date("D:19700101000000Z"), Some((0, Some(0))));
        assert_eq!(parse_pdf_date("D:2026").map(|d| d.1), Some(None));
        assert_eq!(parse_pdf_date("x"), None);
    }
}
