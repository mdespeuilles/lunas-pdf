"""Fixtures signés (PAdES via pyHanko, certificat autosigné de test).

- signe-valide.pdf        : signature couvrant tout le fichier
- signe-puis-annote.pdf   : signature + révision incrémentale ultérieure (annotation)
- signe-altere.pdf        : octets modifiés dans la plage signée (doit être invalide)
"""
import datetime as dt
import sys
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.x509.oid import NameOID
from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
from pyhanko.pdf_utils.generic import ArrayObject, DictionaryObject, FloatObject, NameObject, TextStringObject
from pyhanko.sign import signers, fields
from pyhanko.sign.signers.pdf_signer import PdfSignatureMetadata

OUT = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent)
KEYDIR = OUT / "certs"
KEYDIR.mkdir(parents=True, exist_ok=True)


def make_cert():
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([
        x509.NameAttribute(NameOID.COMMON_NAME, "Camille Testeur"),
        x509.NameAttribute(NameOID.ORGANIZATION_NAME, "Feuillet Fixtures"),
        x509.NameAttribute(NameOID.COUNTRY_NAME, "FR"),
    ])
    now = dt.datetime.now(dt.timezone.utc)
    cert = (x509.CertificateBuilder().subject_name(name).issuer_name(name)
            .public_key(key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now - dt.timedelta(days=1)).not_valid_after(now + dt.timedelta(days=3650))
            .add_extension(x509.KeyUsage(True, True, False, False, False, False, False, False, False), True)
            .sign(key, hashes.SHA256()))
    (KEYDIR / "signer.key.pem").write_bytes(key.private_bytes(
        serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()))
    (KEYDIR / "signer.cert.pem").write_bytes(cert.public_bytes(serialization.Encoding.PEM))


make_cert()
signer = signers.SimpleSigner.load(str(KEYDIR / "signer.key.pem"), str(KEYDIR / "signer.cert.pem"))
meta = PdfSignatureMetadata(field_name="Signature1", reason="Approbation du contrat",
                            location="Lyon", name="Camille Testeur")

with open(OUT / "texte-simple.pdf", "rb") as inf:
    w = IncrementalPdfFileWriter(inf)
    fields.append_signature_field(w, fields.SigFieldSpec("Signature1", box=(72, 60, 272, 110)))
    with open(OUT / "signe-valide.pdf", "wb") as out:
        signers.sign_pdf(w, meta, signer=signer, output=out)

# Révision incrémentale après signature : ajout d'une annotation Text.
with open(OUT / "signe-valide.pdf", "rb") as inf:
    w = IncrementalPdfFileWriter(inf)
    page = w.root["/Pages"]["/Kids"][0].get_object()
    annot = w.add_object(DictionaryObject({
        NameObject("/Type"): NameObject("/Annot"), NameObject("/Subtype"): NameObject("/Text"),
        NameObject("/Rect"): ArrayObject([FloatObject(x) for x in (400, 700, 420, 720)]),
        NameObject("/Contents"): TextStringObject("Note ajoutée après signature"),
    }))
    annots = page.get("/Annots", ArrayObject())
    annots.append(annot)
    page[NameObject("/Annots")] = annots
    w.update_container(page)
    with open(OUT / "signe-puis-annote.pdf", "wb") as out:
        w.write(out)

# Altération : on change un octet du flux de contenu (dans la plage signée).
data = bytearray((OUT / "signe-valide.pdf").read_bytes())
i = data.find(b"(Texte simple)") + 1
assert i > 0
data[i:i + 5] = b"TEXTE"
(OUT / "signe-altere.pdf").write_bytes(bytes(data))
print("ok")
