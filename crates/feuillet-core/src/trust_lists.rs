//! Listes de confiance pour la vérification des signatures, en plus du magasin du système :
//!
//! - **Listes de confiance de l'UE** (eIDAS) : autorités de certification qualifiées et services
//!   d'horodatage qualifiés des listes nationales, avec leurs périodes d'agrément.
//! - **Programme de racines de Microsoft** (publié par la CCADB) : racines actives admises pour
//!   signer des documents, des courriels ou authentifier des personnes.
//!
//! Un instantané est embarqué (`data/trust-anchors.json.gz`, régénéré par
//! `cargo run -p feuillet-core --features trust-fetch --example trust_lists`) ; l'application le
//! rafraîchit périodiquement (voir `fetch`).

use std::io::Read;
use std::sync::OnceLock;

use base64::Engine as _;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};

pub const LOTL_URL: &str = "https://ec.europa.eu/tools/lotl/eu-lotl.xml";
pub const CCADB_MICROSOFT_URL: &str = "https://ccadb.my.salesforce-sites.com/microsoft/IncludedCACertificateReportForMSFTCSVPEM";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// « eu:IT », « microsoft ».
    pub source: String,
    pub name: String,
    /// Certificat DER, encodé en base64.
    pub der: String,
    /// Périodes d'agrément (secondes Unix, fin exclue ; `None` : en cours). Vide : toujours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub periods: Vec<(i64, Option<i64>)>,
}

impl Anchor {
    pub fn trusted_at(&self, t: i64) -> bool {
        self.periods.is_empty() || self.periods.iter().any(|(a, b)| t >= *a && b.is_none_or(|b| t < b))
    }
    pub fn der_bytes(&self) -> Option<Vec<u8>> {
        base64::engine::general_purpose::STANDARD.decode(&self.der).ok()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bundle {
    /// Date de génération (secondes Unix).
    pub generated: i64,
    pub anchors: Vec<Anchor>,
}

impl Bundle {
    pub fn to_gz(&self) -> Vec<u8> {
        let json = serde_json::to_vec(self).unwrap_or_default();
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        let _ = std::io::Write::write_all(&mut enc, &json);
        enc.finish().unwrap_or_default()
    }
    pub fn from_gz(gz: &[u8]) -> Option<Bundle> {
        let mut json = vec![];
        flate2::read::GzDecoder::new(gz).read_to_end(&mut json).ok()?;
        serde_json::from_slice(&json).ok()
    }
}

/// Listes en vigueur : l'instantané embarqué, ou une version plus récente installée par
/// `set_current` (rafraîchissement de l'application).
pub fn current() -> std::sync::Arc<Bundle> {
    let cur = CURRENT.read().unwrap().clone();
    cur.unwrap_or_else(|| std::sync::Arc::new(bundled().clone()))
}

/// Installe des listes plus récentes que celles en vigueur ; faux sinon.
pub fn set_current(b: Bundle) -> bool {
    if b.anchors.is_empty() || b.generated <= current().generated {
        return false;
    }
    *CURRENT.write().unwrap() = Some(std::sync::Arc::new(b));
    true
}

static CURRENT: std::sync::RwLock<Option<std::sync::Arc<Bundle>>> = std::sync::RwLock::new(None);

/// Instantané embarqué.
pub fn bundled() -> &'static Bundle {
    static B: OnceLock<Bundle> = OnceLock::new();
    B.get_or_init(|| Bundle::from_gz(include_bytes!("../data/trust-anchors.json.gz")).unwrap_or_default())
}

// --- Listes de l'UE -----------------------------------------------------------------------------

/// Adresses des listes nationales (XML) citées par la liste des listes : (pays, URL).
pub fn parse_lotl(xml: &[u8]) -> Vec<(String, String)> {
    let mut out = vec![];
    let (mut location, mut territory, mut mime) = (String::new(), String::new(), String::new());
    walk_xml(xml, |path, ev| match ev {
        XmlEv::Start if path.last().map(String::as_str) == Some("OtherTSLPointer") => {
            (location, territory, mime) = (String::new(), String::new(), String::new());
        }
        XmlEv::End(text) => match path.last().map(String::as_str) {
            Some("TSLLocation") => location = text.to_string(),
            Some("SchemeTerritory") => territory = text.to_string(),
            Some("MimeType") => mime = text.to_string(),
            Some("OtherTSLPointer") if mime.ends_with("tsl+xml") && !location.is_empty() && territory != "EU" => {
                out.push((territory.clone(), location.clone()));
            }
            _ => {}
        },
        _ => {}
    });
    out
}

enum XmlEv<'a> {
    Start,
    /// Fin d'élément, avec son texte (entités résolues, espaces de bord retirés).
    End(&'a str),
}

/// Parcours SAX : `f(chemin des noms locaux, événement)`, le chemin incluant l'élément courant.
fn walk_xml(xml: &[u8], mut f: impl FnMut(&[String], XmlEv)) {
    let mut r = quick_xml::Reader::from_reader(xml);
    let mut path: Vec<String> = vec![];
    let mut text = String::new();
    let mut buf = vec![];
    loop {
        match r.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                path.push(local(e.name().as_ref()));
                text.clear();
                f(&path, XmlEv::Start);
            }
            Ok(Event::Empty(e)) => {
                path.push(local(e.name().as_ref()));
                f(&path, XmlEv::Start);
                f(&path, XmlEv::End(""));
                path.pop();
            }
            Ok(Event::Text(t)) => text.push_str(&t.decode().unwrap_or_default()),
            Ok(Event::CData(t)) => text.push_str(&String::from_utf8_lossy(&t)),
            Ok(Event::GeneralRef(e)) => {
                let name = e.decode().unwrap_or_default();
                match e.resolve_char_ref() {
                    Ok(Some(c)) => text.push(c),
                    _ => text.push_str(quick_xml::escape::resolve_predefined_entity(&name).unwrap_or("")),
                }
            }
            Ok(Event::End(_)) => {
                f(&path, XmlEv::End(text.trim()));
                text.clear();
                path.pop();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
}

/// Types de services retenus : autorités qualifiées et horodatage qualifié.
const SERVICE_TYPES: [&str; 5] = [
    "http://uri.etsi.org/TrstSvc/Svctype/CA/QC",
    "http://uri.etsi.org/TrstSvc/Svctype/NationalRootCA-QC",
    "http://uri.etsi.org/TrstSvc/Svctype/TSA/QTST",
    "http://uri.etsi.org/TrstSvc/Svctype/TSA/TSS-QC",
    "http://uri.etsi.org/TrstSvc/Svctype/TSA/TSS-AdESQCandQES",
];

/// Statuts valant agrément (eIDAS, et anciens statuts de la directive 1999/93).
fn status_trusted(s: &str) -> bool {
    [
        "/granted",
        "/recognisedatnationallevel",
        "/accredited",
        "/undersupervision",
        "/setbynationallaw",
    ]
    .iter()
    .any(|x| s.ends_with(x))
}

#[derive(Default)]
struct Service {
    kind: String,
    name: String,
    certs: Vec<String>,
    /// (début, statut) de l'état courant et de l'historique.
    states: Vec<(i64, String)>,
    cur_status: String,
    cur_start: i64,
}

/// Autorités d'une liste nationale.
pub fn parse_tsl(xml: &[u8], country: &str) -> Vec<Anchor> {
    let mut out = vec![];
    let mut svc: Option<Service> = None;
    walk_xml(xml, |path, ev| {
        let here = path.last().map(String::as_str).unwrap_or("");
        match ev {
            XmlEv::Start if here == "TSPService" => svc = Some(Service::default()),
            XmlEv::Start => {}
            XmlEv::End(text) => {
                let Some(s) = svc.as_mut() else { return };
                let in_history = path.iter().any(|p| p == "ServiceHistoryInstance");
                match here {
                    "ServiceTypeIdentifier" if !in_history => s.kind = text.to_string(),
                    "Name" if !in_history && path.iter().any(|p| p == "ServiceName") && s.name.is_empty() => {
                        s.name = text.to_string()
                    }
                    "X509Certificate" if path.iter().any(|p| p == "ServiceDigitalIdentity") => {
                        let b64: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                        if !b64.is_empty() {
                            s.certs.push(b64);
                        }
                    }
                    "ServiceStatus" => s.cur_status = text.to_string(),
                    "StatusStartingTime" => s.cur_start = parse_iso(text).unwrap_or(0),
                    // Fin d'un état (courant ou historique) : statut et date de début.
                    "ServiceInformation" | "ServiceHistoryInstance" if !s.cur_status.is_empty() => {
                        s.states.push((s.cur_start, std::mem::take(&mut s.cur_status)));
                    }
                    "TSPService" => {
                        let s = svc.take().unwrap();
                        if SERVICE_TYPES.contains(&s.kind.as_str()) {
                            let periods = periods(&s.states);
                            if !periods.is_empty() {
                                for der in s.certs.iter().collect::<std::collections::BTreeSet<_>>() {
                                    out.push(Anchor {
                                        source: format!("eu:{country}"),
                                        name: short_name(&s.name),
                                        der: der.clone(),
                                        periods: periods.clone(),
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    });
    out
}

/// Périodes d'agrément à partir des états datés (un état court jusqu'au suivant).
fn periods(states: &[(i64, String)]) -> Vec<(i64, Option<i64>)> {
    let mut s = states.to_vec();
    s.sort_by_key(|x| x.0);
    let mut out: Vec<(i64, Option<i64>)> = vec![];
    for (i, (start, status)) in s.iter().enumerate() {
        if !status_trusted(status) {
            continue;
        }
        let end = s.get(i + 1).map(|n| n.0);
        match out.last_mut() {
            Some(last) if last.1 == Some(*start) => last.1 = end,
            _ => out.push((*start, end)),
        }
    }
    out
}

/// « CN=Intesi …, OU=…, C=IT » → « Intesi … ».
fn short_name(n: &str) -> String {
    n.split(',')
        .find_map(|p| p.trim().strip_prefix("CN="))
        .map(str::to_string)
        .unwrap_or_else(|| n.to_string())
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or("").to_string()
}

/// « 2018-01-19T15:00:00Z » → secondes Unix.
fn parse_iso(s: &str) -> Option<i64> {
    let d: String = s.chars().filter(char::is_ascii_digit).take(14).collect();
    crate::signature::parse_pdf_date(&format!("{d}Z")).map(|x| x.0)
}

// --- Programme de racines de Microsoft (CCADB) ---------------------------------------------------

/// Usages qui font une racine utile aux signatures de documents.
const MS_EKUS: [&str; 3] = ["Document Signing", "Secure Email", "Client Authentication"];

/// Racines actives (« Included ») du rapport CSV avec PEM de la CCADB.
pub fn parse_ccadb_csv(csv: &[u8]) -> Vec<Anchor> {
    let rows = csv_rows(&String::from_utf8_lossy(csv));
    let Some(header) = rows.first() else { return vec![] };
    let col = |name: &str| header.iter().position(|h| h == name);
    let (Some(status), Some(cn), Some(ekus), Some(pem)) = (
        col("Microsoft Status"),
        col("CA Common Name or Certificate Name"),
        col("Microsoft EKUs"),
        col("PEM Info"),
    ) else {
        return vec![];
    };
    rows.iter()
        .skip(1)
        .filter(|r| r.get(status).map(String::as_str) == Some("Included"))
        .filter(|r| r.get(ekus).is_some_and(|e| MS_EKUS.iter().any(|k| e.contains(k))))
        .filter_map(|r| {
            let p = r.get(pem)?;
            let b64: String = p
                .lines()
                .filter(|l| !l.contains("-----"))
                .flat_map(|l| l.chars())
                .filter(|c| !c.is_whitespace() && *c != '\'')
                .collect();
            (!b64.is_empty()).then(|| Anchor {
                source: "microsoft".into(),
                name: r.get(cn).cloned().unwrap_or_default(),
                der: b64,
                periods: vec![],
            })
        })
        .collect()
}

/// Lignes d'un CSV (champs entre guillemets, éventuellement sur plusieurs lignes).
fn csv_rows(s: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            ('\r', false) => {}
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// Écarte les doublons (même certificat et même source).
pub fn dedup(mut anchors: Vec<Anchor>) -> Vec<Anchor> {
    let mut seen = std::collections::HashSet::new();
    anchors.retain(|a| seen.insert((a.source.clone(), a.der.clone())));
    anchors
}

// --- Téléchargement (application, régénération de l'instantané) ---------------------------------

/// Télécharge et assemble les listes. Une liste nationale indisponible est ignorée ; échec si
/// la liste de l'UE ou celle de Microsoft manque.
#[cfg(feature = "trust-fetch")]
pub fn fetch(now: i64) -> Result<Bundle, String> {
    let get = |url: &str| -> Result<Vec<u8>, String> {
        let resp = ureq::get(url)
            .timeout(std::time::Duration::from_secs(60))
            .call()
            .map_err(|e| format!("{url} : {e}"))?;
        let mut body = vec![];
        resp.into_reader()
            .take(64 * 1024 * 1024)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        Ok(body)
    };
    let lotl = get(LOTL_URL)?;
    let mut anchors = vec![];
    for (country, url) in parse_lotl(&lotl) {
        if let Ok(xml) = get(&url) {
            anchors.extend(parse_tsl(&xml, &country));
        }
    }
    if anchors.is_empty() {
        return Err("listes de l'UE vides".into());
    }
    let ms = parse_ccadb_csv(&get(CCADB_MICROSOFT_URL)?);
    if ms.is_empty() {
        return Err("liste Microsoft vide".into());
    }
    anchors.extend(ms);
    Ok(Bundle {
        generated: now,
        anchors: dedup(anchors),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_periods() {
        let g = "http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/granted".to_string();
        let w = "http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/withdrawn".to_string();
        assert_eq!(periods(&[(100, g.clone())]), [(100, None)]);
        assert_eq!(periods(&[(300, w.clone()), (100, g.clone())]), [(100, Some(300))]);
        // Deux états accordés consécutifs fusionnent.
        let acc = "http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/accredited".to_string();
        assert_eq!(periods(&[(50, acc), (100, g.clone()), (300, w)]), [(50, Some(300))]);
        let a = Anchor {
            source: "eu:IT".into(),
            name: "x".into(),
            der: String::new(),
            periods: vec![(100, Some(300))],
        };
        assert!(a.trusted_at(100) && a.trusted_at(299) && !a.trusted_at(300) && !a.trusted_at(99));
    }

    #[test]
    fn parses_lists() {
        let lotl = br#"<TrustServiceStatusList xmlns:ns3="x"><OtherTSLPointer><TSLLocation>https://fr/tl.xml</TSLLocation>
            <AdditionalInformation><OtherInformation><SchemeTerritory>FR</SchemeTerritory></OtherInformation>
            <OtherInformation><ns3:MimeType>application/vnd.etsi.tsl+xml</ns3:MimeType></OtherInformation></AdditionalInformation></OtherTSLPointer>
            <OtherTSLPointer><TSLLocation>https://fr/tl.pdf</TSLLocation><AdditionalInformation><OtherInformation><SchemeTerritory>FR</SchemeTerritory></OtherInformation>
            <OtherInformation><ns3:MimeType>application/pdf</ns3:MimeType></OtherInformation></AdditionalInformation></OtherTSLPointer></TrustServiceStatusList>"#;
        assert_eq!(parse_lotl(lotl), [("FR".to_string(), "https://fr/tl.xml".to_string())]);

        let tsl = br#"<TrustServiceStatusList><TrustServiceProvider><TSPServices>
            <TSPService><ServiceInformation><ServiceTypeIdentifier>http://uri.etsi.org/TrstSvc/Svctype/CA/QC</ServiceTypeIdentifier>
              <ServiceName><Name xml:lang="en">CN=Test CA, O=Test, C=IT</Name></ServiceName>
              <ServiceDigitalIdentity><DigitalId><X509Certificate>QUJD
              REVG</X509Certificate></DigitalId></ServiceDigitalIdentity>
              <ServiceStatus>http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/withdrawn</ServiceStatus>
              <StatusStartingTime>2020-01-01T00:00:00Z</StatusStartingTime></ServiceInformation>
              <ServiceHistory><ServiceHistoryInstance><ServiceTypeIdentifier>http://uri.etsi.org/TrstSvc/Svctype/CA/QC</ServiceTypeIdentifier>
              <ServiceStatus>http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/granted</ServiceStatus>
              <StatusStartingTime>2016-07-01T00:00:00Z</StatusStartingTime></ServiceHistoryInstance></ServiceHistory></TSPService>
            <TSPService><ServiceInformation><ServiceTypeIdentifier>http://uri.etsi.org/TrstSvc/Svctype/CertStatus/OCSP/QC</ServiceTypeIdentifier>
              <ServiceDigitalIdentity><DigitalId><X509Certificate>WFla</X509Certificate></DigitalId></ServiceDigitalIdentity>
              <ServiceStatus>http://uri.etsi.org/TrstSvc/TrustedList/Svcstatus/granted</ServiceStatus><StatusStartingTime>2016-07-01T00:00:00Z</StatusStartingTime></ServiceInformation></TSPService>
            </TSPServices></TrustServiceProvider></TrustServiceStatusList>"#;
        let a = parse_tsl(tsl, "IT");
        assert_eq!(a.len(), 1);
        assert_eq!(
            (a[0].name.as_str(), a[0].der.as_str(), a[0].source.as_str()),
            ("Test CA", "QUJDREVG", "eu:IT")
        );
        assert_eq!(a[0].periods, [(1_467_331_200, Some(1_577_836_800))]);

        let csv = b"\"Microsoft Status\",\"CA Common Name or Certificate Name\",\"Microsoft EKUs\",\"PEM Info\"\n\"Included\",\"Doc Root\",\"Document Signing;Time Stamping\",\"'-----BEGIN CERTIFICATE-----\nQUJD\n-----END CERTIFICATE-----'\"\n\"Included\",\"Web Root\",\"Server Authentication\",\"'-----BEGIN CERTIFICATE-----\nREVG\n-----END CERTIFICATE-----'\"\n\"Disabled\",\"Old\",\"Document Signing\",\"x\"\n";
        let m = parse_ccadb_csv(csv);
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].name.as_str(), m[0].der.as_str()), ("Doc Root", "QUJD"));
    }
}
