//! Vérification des signatures numériques sur les fixtures signés (pyHanko, certificat de test).

use std::path::PathBuf;

use der::DecodePem;
use lunas_pdf_core::signature::{SigStatus, TrustStore, verify_document};
use x509_cert::Certificate;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)).unwrap()
}

fn test_root() -> Certificate {
    Certificate::from_pem(fixture("certs/signer.cert.pem")).unwrap()
}

#[test]
fn valid_signature_with_and_without_trust() {
    let bytes = fixture("signe-valide.pdf");
    // Certificat autosigné inconnu du système : intacte mais non vérifiable.
    let s = verify_document(&bytes, None, &TrustStore::default());
    assert_eq!(s.len(), 1);
    let s = &s[0];
    assert_eq!(s.field, "Signature1");
    assert!(s.intact && s.covers_whole && !s.trusted);
    assert_eq!(s.status, SigStatus::Unknown);
    assert_eq!(s.problem.as_deref(), Some("untrusted"));
    assert_eq!(s.signer.as_deref(), Some("Camille Testeur"));
    assert_eq!(s.organization.as_deref(), Some("Feuillet Fixtures"));
    assert_eq!(s.reason.as_deref(), Some("Approbation du contrat"));
    assert_eq!(s.location.as_deref(), Some("Lyon"));
    assert_eq!(s.page, Some(0));
    let r = s.rect.unwrap();
    assert_eq!((r.x.round(), r.w.round(), r.h.round()), (72.0, 200.0, 50.0));
    assert!(s.signed_at.unwrap() > 1.79e9);
    let c = s.certificate.as_ref().unwrap();
    assert_eq!(c.sha256.len(), 32 * 3 - 1);
    assert_eq!(c.chain, ["Camille Testeur"]);

    // Avec le certificat de test comme autorité reconnue : valide.
    let s = &verify_document(&bytes, None, &TrustStore::of(&[test_root()], "user"))[0];
    assert!(s.trusted && s.cert_valid_at_signing);
    assert_eq!(s.trust_source.as_deref(), Some("user"));
    assert_eq!(s.status, SigStatus::Valid);
    assert_eq!(s.problem, None);
}

#[test]
fn altered_document_is_invalid() {
    let s = &verify_document(&fixture("signe-altere.pdf"), None, &TrustStore::of(&[test_root()], "user"))[0];
    assert!(!s.intact);
    assert_eq!(s.status, SigStatus::Invalid);
    assert_eq!(s.problem.as_deref(), Some("modified"));
}

#[test]
fn later_revision_keeps_signature_intact() {
    let s = &verify_document(
        &fixture("signe-puis-annote.pdf"),
        None,
        &TrustStore::of(&[test_root()], "user"),
    )[0];
    assert!(s.intact && !s.covers_whole);
    assert_eq!(s.status, SigStatus::Valid);
}

#[test]
fn system_trust_store_is_loaded() {
    assert!(!lunas_pdf_core::signature::system_roots().is_empty());
}

#[test]
fn unsigned_document_has_no_signature() {
    assert!(verify_document(&fixture("texte-simple.pdf"), None, &TrustStore::default()).is_empty());
    assert!(verify_document(&fixture("formulaire-acroform.pdf"), None, &TrustStore::default()).is_empty());
}

/// Réencode un élément DER en BER, tous les éléments construits en longueur indéfinie.
fn to_indefinite(b: &[u8], pos: &mut usize, out: &mut Vec<u8>) {
    let tag = b[*pos];
    *pos += 1;
    let l = b[*pos] as usize;
    *pos += 1;
    let len = if l & 0x80 == 0 {
        l
    } else {
        let n = l & 0x7f;
        let v = b[*pos..*pos + n].iter().fold(0usize, |a, x| (a << 8) | *x as usize);
        *pos += n;
        v
    };
    let end = *pos + len;
    if tag & 0x20 != 0 {
        out.extend_from_slice(&[tag, 0x80]);
        while *pos < end {
            to_indefinite(b, pos, out);
        }
        out.extend_from_slice(&[0, 0]);
    } else {
        out.push(tag);
        if len < 0x80 {
            out.push(len as u8);
        } else {
            let bytes: Vec<u8> = len.to_be_bytes().into_iter().skip_while(|x| *x == 0).collect();
            out.push(0x80 | bytes.len() as u8);
            out.extend_from_slice(&bytes);
        }
        out.extend_from_slice(&b[*pos..end]);
    }
    *pos = end;
}

/// `/ByteRange` du fichier (première signature).
fn byte_range(bytes: &[u8]) -> Vec<usize> {
    let at = bytes.windows(10).position(|w| w == b"/ByteRange").unwrap();
    let open = at + bytes[at..].iter().position(|b| *b == b'[').unwrap();
    let close = open + bytes[open..].iter().position(|b| *b == b']').unwrap();
    std::str::from_utf8(&bytes[open + 1..close])
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect()
}

#[test]
fn ber_encoded_signature_is_read() {
    // Comme Dropbox Sign : CMS en BER à longueurs indéfinies dans `/Contents`.
    let mut bytes = fixture("signe-valide.pdf");
    let nums = byte_range(&bytes);
    let (start, end) = (nums[0] + nums[1] + 1, nums[2] - 1);
    let hex = std::str::from_utf8(&bytes[start..end]).unwrap();
    let der: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let mut ber = vec![];
    to_indefinite(&der, &mut 0, &mut ber);
    assert_eq!(&ber[..2], [0x30, 0x80]);
    let mut new_hex: String = ber.iter().map(|b| format!("{b:02x}")).collect();
    assert!(new_hex.len() <= end - start);
    new_hex.extend(std::iter::repeat_n('0', end - start - new_hex.len()));
    bytes[start..end].copy_from_slice(new_hex.as_bytes());

    let s = &verify_document(&bytes, None, &TrustStore::of(&[test_root()], "user"))[0];
    assert!(s.intact, "{:?}", s.problem);
    assert_eq!(s.status, SigStatus::Valid);
}

#[test]
fn unreadable_signature_is_unknown_not_invalid() {
    let mut bytes = fixture("signe-valide.pdf");
    let nums = byte_range(&bytes);
    let start = nums[0] + nums[1] + 1;
    bytes[start..start + 8].copy_from_slice(b"ffffffff");
    let s = &verify_document(&bytes, None, &TrustStore::of(&[test_root()], "user"))[0];
    assert_eq!(s.problem.as_deref(), Some("cms"));
    assert_eq!(s.status, SigStatus::Unknown);
}

#[test]
fn approving_the_chain_root_makes_the_signature_valid() {
    use der::Encode;
    use lunas_pdf_core::signature::{chain_root, roots_with};
    let bytes = fixture("signe-valide.pdf");
    let s = &verify_document(&bytes, None, &roots_with(&[]))[0];
    assert_eq!(s.status, SigStatus::Unknown);
    assert_eq!(s.certificate.as_ref().unwrap().root_name, "Camille Testeur");
    let (der, name) = chain_root(&bytes, None, "Signature1").unwrap();
    assert_eq!(name, "Camille Testeur");
    assert_eq!(der, test_root().to_der().unwrap());
    let s = &verify_document(&bytes, None, &roots_with(&[der]))[0];
    assert_eq!(s.status, SigStatus::Valid);
    assert!(chain_root(&bytes, None, "Inconnu").is_none());
}

#[test]
fn trust_list_anchor_counts_only_during_its_approval_period() {
    use lunas_pdf_core::signature::TrustAnchor;
    let bytes = fixture("signe-valide.pdf");
    let signed_at = verify_document(&bytes, None, &TrustStore::default())[0].signed_at.unwrap() as i64;
    let store = |periods| TrustStore {
        anchors: vec![TrustAnchor {
            cert: test_root(),
            source: "eu:FR".into(),
            periods,
        }],
    };
    let s = &verify_document(&bytes, None, &store(vec![(signed_at - 10, None)]))[0];
    assert_eq!((s.status, s.trust_source.as_deref()), (SigStatus::Valid, Some("eu:FR")));
    // Agrément retiré avant la signature : non reconnue.
    let s = &verify_document(&bytes, None, &store(vec![(0, Some(signed_at - 10))]))[0];
    assert_eq!(s.status, SigStatus::Unknown);
}

#[test]
fn bundled_trust_lists_are_present() {
    let b = lunas_pdf_core::trust_lists::bundled();
    assert!(b.generated > 0);
    assert!(b.anchors.iter().filter(|a| a.source.starts_with("eu:")).count() > 1000);
    assert!(b.anchors.iter().filter(|a| a.source == "microsoft").count() > 100);
    // Autorités des services courants (Adobe Acrobat Sign, Dropbox Sign, DocuSign).
    for name in [
        "Intesi Group EU Qualified Electronic Seal CA G2",
        "Notarius Root Certificate Authority",
    ] {
        assert!(b.anchors.iter().any(|a| a.name == name), "{name}");
    }
}
