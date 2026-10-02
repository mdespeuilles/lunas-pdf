"""Génère les PDF de test de fixtures/.

Usage (depuis la racine) :
  python fixtures/scripts/gen_fixtures.py fixtures                      # tous
  python fixtures/scripts/gen_fixtures.py fixtures --only scan-lourd    # le scan (70 Mo, non versionné)
"""
import io
import random
import sys
from pathlib import Path

import numpy as np
import pikepdf
from PIL import Image, ImageDraw, ImageFont
from reportlab.lib.colors import HexColor, black
from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
ONLY = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
OUT = Path(ARGS[0] if ARGS and ARGS[0] != ONLY else Path(__file__).resolve().parent.parent)
OUT.mkdir(parents=True, exist_ok=True)
W, H = A4
LOREM = (
    "Le vif renard brun saute par-dessus le chien paresseux. Feuillet affiche "
    "ce paragraphe pour mesurer le rendu du texte, la recherche plein texte et "
    "la sélection. Portez ce vieux whisky au juge blond qui fume. "
).split()


def para(rng, n):
    return " ".join(rng.choice(LOREM) for _ in range(n))


def texte_simple():
    c = canvas.Canvas(str(OUT / "texte-simple.pdf"), pagesize=A4)
    c.setTitle("Texte simple")
    for p in range(3):
        c.setFont("Helvetica-Bold", 20)
        c.drawString(72, H - 90, f"Page {p + 1} — texte simple")
        c.setFont("Helvetica", 11)
        y = H - 130
        for i in range(30):
            c.drawString(72, y, f"Ligne {i + 1} : Le vif renard brun saute par-dessus le chien paresseux.")
            y -= 18
        c.showPage()
    c.save()


def long_document(n=320):
    """320 pages : texte dense, vecteurs, signets à 2 niveaux, liens internes."""
    rng = random.Random(42)
    c = canvas.Canvas(str(OUT / "long-320-pages.pdf"), pagesize=A4)
    c.setTitle("Document long avec signets")
    for p in range(n):
        key = f"p{p}"
        c.bookmarkPage(key)
        if p % 10 == 0:
            c.addOutlineEntry(f"Chapitre {p // 10 + 1}", key, level=0)
        elif p % 5 == 0:
            c.addOutlineEntry(f"Section {p // 10 + 1}.{p % 10 // 5}", key, level=1)
        c.setFont("Helvetica-Bold", 16)
        c.drawString(60, H - 60, f"Chapitre {p // 10 + 1} — page {p + 1}")
        # vecteurs : grille et courbes
        c.setStrokeColor(HexColor("#3466f6"))
        for i in range(12):
            c.bezier(60, 80 + i * 6, 200, 160 + i * 9, 380, 20 + i * 4, 535, 90 + i * 7)
        c.setStrokeColor(black)
        c.setFont("Times-Roman", 9.5)
        y = H - 90
        while y > 200:
            c.drawString(60, y, para(rng, 16))
            y -= 12
        if p + 1 < n:
            c.setFillColor(HexColor("#3466f6"))
            c.drawString(60, 180, "→ Aller à la fin du chapitre")
            c.linkAbsolute("", f"p{min(n - 1, (p // 10) * 10 + 9)}", (60, 176, 200, 190))
            c.setFillColor(black)
        c.showPage()
    c.showOutline()
    c.save()


def scan_lourd(n=24, dpi=300):
    """Pages scannées : JPEG couleur A4 300 ppp bruités (~1 Mo/page)."""
    rng = np.random.default_rng(1)
    pw, ph = int(8.27 * dpi), int(11.69 * dpi)
    font = ImageFont.load_default(size=42)
    pdf = pikepdf.new()
    for p in range(n):
        base = np.full((ph, pw, 3), 238, np.uint8)
        noise = rng.normal(0, 14, (ph // 4 + 1, pw // 4 + 1, 3))
        noise = np.kron(noise, np.ones((4, 4, 1)))[:ph, :pw]
        img = Image.fromarray(np.clip(base + noise, 0, 255).astype(np.uint8))
        d = ImageDraw.Draw(img)
        d.text((200, 200), f"Document scanné — page {p + 1}", fill=(30, 30, 30), font=font)
        for i in range(60):
            d.text((200, 320 + i * 50), "Lorem ipsum dolor sit amet, consectetur adipiscing elit " * 2,
                   fill=(40, 40, 50), font=ImageFont.load_default(size=28))
        img = img.rotate(0.6, fillcolor=(238, 238, 238))
        buf = io.BytesIO()
        img.save(buf, "JPEG", quality=88)
        page = pdf.add_blank_page(page_size=(W, H))
        image = pikepdf.Stream(pdf, buf.getvalue())
        image.Type = pikepdf.Name.XObject
        image.Subtype = pikepdf.Name.Image
        image.Width, image.Height = img.width, img.height
        image.ColorSpace = pikepdf.Name.DeviceRGB
        image.BitsPerComponent = 8
        image.Filter = pikepdf.Name.DCTDecode
        page.Resources = pikepdf.Dictionary(XObject=pikepdf.Dictionary(Im0=image))
        page.Contents = pdf.make_stream(f"q {W} 0 0 {H} 0 0 cm /Im0 Do Q".encode())
    pdf.save(OUT / "scan-lourd.pdf")


def formulaire():
    c = canvas.Canvas(str(OUT / "formulaire-acroform.pdf"), pagesize=A4)
    c.setTitle("Formulaire AcroForm")
    f = c.acroForm
    c.setFont("Helvetica-Bold", 16)
    c.drawString(72, H - 72, "Demande d'adhésion")
    c.setFont("Helvetica", 11)
    rows = [("Nom", "nom"), ("Prénom", "prenom"), ("Courriel", "courriel")]
    y = H - 120
    for label, name in rows:
        c.drawString(72, y + 6, label)
        f.textfield(name=name, x=180, y=y, width=300, height=20, borderWidth=1)
        y -= 36
    c.drawString(72, y + 6, "Commentaire")
    f.textfield(name="commentaire", x=180, y=y - 40, width=300, height=60, fieldFlags="multiline")
    y -= 90
    c.drawString(72, y + 4, "J'accepte les conditions")
    f.checkbox(name="accepte", x=250, y=y, size=16, buttonStyle="check")
    y -= 36
    c.drawString(72, y + 4, "Formule")
    for i, v in enumerate(["mensuelle", "annuelle", "a_vie"]):
        f.radio(name="formule", value=v, x=180 + i * 110, y=y, size=16, selected=(i == 0))
        c.drawString(200 + i * 110, y + 4, v)
    y -= 40
    c.drawString(72, y + 6, "Pays")
    f.choice(name="pays", options=["France", "Belgique", "Suisse", "Canada"], value="France",
             x=180, y=y, width=200, height=20)
    y -= 90
    c.drawString(72, y + 60, "Centres d'intérêt")
    f.listbox(name="interets", options=["Lecture", "Musique", "Sport", "Voyage"], value="Lecture",
              x=180, y=y, width=200, height=70)
    c.showPage()
    c.save()


def avec_annotations():
    """PDF source pour l'import de pages : annotations, lien URI, signets."""
    src = OUT / "texte-simple.pdf"
    pdf = pikepdf.open(src)
    page = pdf.pages[0]
    annots = pikepdf.Array()
    annots.append(pdf.make_indirect(pikepdf.Dictionary(
        Type=pikepdf.Name.Annot, Subtype=pikepdf.Name.Square, Rect=[70, 600, 300, 700],
        C=[1, 0, 0], Border=[0, 0, 2], Contents=pikepdf.String("Carré existant"))))
    annots.append(pdf.make_indirect(pikepdf.Dictionary(
        Type=pikepdf.Name.Annot, Subtype=pikepdf.Name.Link, Rect=[70, 740, 400, 760],
        Border=[0, 0, 0], A=pikepdf.Dictionary(S=pikepdf.Name.URI, URI=pikepdf.String("https://example.org")))))
    annots.append(pdf.make_indirect(pikepdf.Dictionary(
        Type=pikepdf.Name.Annot, Subtype=pikepdf.Name.Link, Rect=[70, 560, 300, 580], Border=[0, 0, 0],
        Dest=[pdf.pages[2].obj, pikepdf.Name.Fit])))
    page.Annots = annots
    with pdf.open_outline() as ol:
        ol.root.extend([pikepdf.OutlineItem(f"Signet page {i + 1}", i) for i in range(3)])
    pdf.save(OUT / "avec-annotations.pdf")


def chiffre():
    pdf = pikepdf.open(OUT / "texte-simple.pdf")
    pdf.save(OUT / "chiffre-aes256.pdf", encryption=pikepdf.Encryption(
        user="feuillet", owner="proprietaire", R=6,
        allow=pikepdf.Permissions(extract=False, print_highres=False, print_lowres=False)))


if __name__ == "__main__":
    steps = {
        "texte-simple": texte_simple,
        "long-320-pages": long_document,
        "scan-lourd": scan_lourd,
        "formulaire-acroform": formulaire,
        "avec-annotations": avec_annotations,
        "chiffre-aes256": chiffre,
    }
    for name, step in steps.items():
        if ONLY is None or ONLY == name:
            step()
    for f in sorted(OUT.glob("*.pdf")):
        print(f"{f.name:32} {f.stat().st_size / 1e6:8.2f} Mo")
