# SPDX-License-Identifier: AGPL-3.0-or-later
"""Generate Kitab's deterministic test corpus.

    python tests/fixtures/tools/generate_fixtures.py            # everything except the 20 MB PDF
    python tests/fixtures/tools/generate_fixtures.py --large    # also the 20 MB / 300-page PDF

Writes into tests/fixtures/{pdf,epub,md}. Every file is rebuilt from scratch
with fixed seeds and fixed dates, so a rerun gives the same content (byte-for-
byte for reportlab and zip output; PyMuPDF and pyHanko embed IDs/timestamps).
What each file is for is documented in tests/fixtures/MANIFEST.md.

Needs: reportlab, pypdf, pymupdf, pillow, cryptography, pyhanko (signing only).
See requirements.txt.
"""

from __future__ import annotations

import argparse
import datetime as dt
import io
import random
import shutil
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PDF = ROOT / "pdf"
EPUB = ROOT / "epub"
MD = ROOT / "md"
OTHER = ROOT / "other"
CERTS = PDF / "certs"

FIXED_DATE = dt.datetime(2026, 11, 6, 9, 0, 0, tzinfo=dt.timezone.utc)

WORDS = (
    "reader page margin chapter verse ink paper spine binding folio quire leaf "
    "column gutter serif glyph kerning ledger index note gloss scholia rubric "
    "codex scroll vellum quill press type line measure leading tracking"
).split()


def sentence(rng: random.Random, n: int = 14) -> str:
    words = [rng.choice(WORDS) for _ in range(n)]
    words[0] = words[0].capitalize()
    return " ".join(words) + "."


def paragraph(rng: random.Random, sentences: int = 5) -> str:
    return " ".join(sentence(rng, rng.randint(9, 18)) for _ in range(sentences))


# --------------------------------------------------------------------------- PDF


def pdf_text_outline(path: Path) -> None:
    """40 pages, nested outline, internal + external links, metadata, known search counts."""
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.utils import simpleSplit
    from reportlab.pdfgen import canvas

    rng = random.Random(1)
    c = canvas.Canvas(str(path), pagesize=A4, invariant=1)
    c.setTitle("Kitab fixture: text and outline")
    c.setAuthor("Kitab test suite")
    c.setSubject("Search, outline, links, metadata")
    c.setKeywords("kitab, fixture, outline, search")
    c.setCreator("generate_fixtures.py")
    w, h = A4
    chapters = 8
    pages_per_chapter = 5
    page_no = 0
    for ch in range(1, chapters + 1):
        for p in range(pages_per_chapter):
            page_no += 1
            key = f"p{page_no}"
            c.bookmarkPage(key)
            y = h - 72
            if p == 0:
                c.addOutlineEntry(f"Chapter {ch}", f"ch{ch}", level=0, closed=ch > 2)
                c.bookmarkPage(f"ch{ch}")
                c.setFont("Helvetica-Bold", 22)
                c.drawString(72, y, f"Chapter {ch}")
                y -= 36
            if p in (1, 3):
                c.addOutlineEntry(f"Section {ch}.{p}", key, level=1)
                c.setFont("Helvetica-Bold", 14)
                c.drawString(72, y, f"Section {ch}.{p}")
                y -= 26
            c.setFont("Helvetica", 11)
            for para in range(4):
                text = paragraph(rng)
                # Known search targets (see MANIFEST.md):
                # "lantern" once per chapter, "Lantern" once in chapter 3,
                # "lanterns" once in chapter 5, and a unique needle per page.
                if p == 2 and para == 1:
                    text += " A lantern hangs by the door."
                if ch == 3 and p == 4 and para == 0:
                    text += " Lantern Street is capitalised."
                if ch == 5 and p == 3 and para == 2:
                    text += " Two lanterns are plural."
                if para == 3:
                    text += f" kitab-needle-{page_no:02d}."
                for line in simpleSplit(text, "Helvetica", 11, w - 144):
                    c.drawString(72, y, line)
                    y -= 15
                y -= 8
            if page_no == 1:
                c.setFont("Helvetica", 11)
                c.setFillColorRGB(0.1, 0.3, 0.8)
                c.drawString(72, 120, "Internal link: go to Chapter 6")
                c.linkRect("", "ch6", (72, 116, 260, 130), relative=0)
                c.drawString(72, 100, "External link: https://example.invalid/kitab")
                c.linkURL("https://example.invalid/kitab", (72, 96, 330, 110), relative=0)
                c.setFillColorRGB(0, 0, 0)
            c.setFont("Helvetica", 9)
            c.drawCentredString(w / 2, 36, str(page_no))
            c.showPage()
    c.save()


def pdf_mixed_sizes(path: Path) -> None:
    """Six pages: A4, A4 landscape, A5, Letter, an 80 x 400 mm receipt, and a /Rotate 90 page."""
    from pypdf import PdfReader, PdfWriter
    from reportlab.lib.pagesizes import A4, A5, letter, landscape
    from reportlab.lib.units import mm
    from reportlab.pdfgen import canvas

    sizes = [
        ("A4 portrait 210 x 297 mm", A4),
        ("A4 landscape 297 x 210 mm", landscape(A4)),
        ("A5 148 x 210 mm", A5),
        ("US Letter 8.5 x 11 in", letter),
        ("Receipt 80 x 400 mm", (80 * mm, 400 * mm)),
        ("A4 portrait with /Rotate 90 (shows landscape)", A4),
    ]
    buf = io.BytesIO()
    c = canvas.Canvas(buf, invariant=1)
    c.setTitle("Kitab fixture: mixed page sizes")
    for i, (label, size) in enumerate(sizes, 1):
        c.setPageSize(size)
        w, h = size
        c.setLineWidth(2)
        c.rect(12, 12, w - 24, h - 24)
        c.setFont("Helvetica-Bold", 16 if w > 300 else 10)
        c.drawCentredString(w / 2, h / 2 + 10, f"Page {i}")
        c.setFont("Helvetica", 11 if w > 300 else 7)
        c.drawCentredString(w / 2, h / 2 - 10, label)
        c.drawString(20, h - 30, "TOP LEFT")
        c.showPage()
    c.save()
    reader = PdfReader(io.BytesIO(buf.getvalue()))
    writer = PdfWriter()
    for i, page in enumerate(reader.pages):
        if i == 5:
            page.rotate(90)
        writer.add_page(page)
    writer.add_metadata({"/Title": "Kitab fixture: mixed page sizes"})
    with path.open("wb") as f:
        writer.write(f)


def pdf_forms(path: Path) -> None:
    """AcroForm: text, prefilled text, checkbox, radio group, dropdown, list box."""
    from reportlab.lib.pagesizes import A4
    from reportlab.pdfgen import canvas

    c = canvas.Canvas(str(path), pagesize=A4, invariant=1)
    c.setTitle("Kitab fixture: form fields")
    form = c.acroForm
    y = 760
    c.setFont("Helvetica-Bold", 18)
    c.drawString(72, y, "Library card application")
    c.setFont("Helvetica", 11)
    y -= 50
    c.drawString(72, y + 4, "Full name")
    form.textfield(name="full_name", tooltip="Full name", x=180, y=y - 4, width=300, height=20)
    y -= 40
    c.drawString(72, y + 4, "City (prefilled)")
    form.textfield(name="city", value="Dhaka", x=180, y=y - 4, width=300, height=20)
    y -= 40
    c.drawString(72, y + 4, "Newsletter")
    form.checkbox(name="newsletter", x=180, y=y - 2, size=16, checked=True, buttonStyle="check")
    y -= 40
    c.drawString(72, y + 4, "Card size")
    for i, (val, label) in enumerate([("small", "Small"), ("large", "Large")]):
        form.radio(name="card_size", value=val, selected=(i == 0), x=180 + i * 90, y=y - 2, size=16)
        c.drawString(200 + i * 90, y + 2, label)
    y -= 40
    c.drawString(72, y + 4, "Branch")
    form.choice(name="branch", value="Central", options=["Central", "North", "South"], x=180, y=y - 4, width=160, height=20)
    y -= 70
    c.drawString(72, y + 40, "Interests")
    form.listbox(name="interests", value="History", options=["History", "Science", "Poetry", "Law"], x=180, y=y - 10, width=160, height=64)
    c.showPage()
    c.save()


def pdf_form_two_pages(path: Path) -> None:
    """A field shown on two pages (one parent, two widgets), a multi-line box
    and a read-only box. Written by hand so the shared field is a real parent
    with kids, as forms made in Acrobat have."""
    W, H = 595, 842

    def label(lines):
        ops = ["BT /F1 11 Tf"]
        for x, y, text in lines:
            ops.append(f"1 0 0 1 {x} {y} Tm ({text}) Tj")
        ops.append("ET")
        return "\n".join(ops).encode("latin-1")

    c0 = label([(72, 770, "Two-page form"), (72, 724, "Address"), (72, 674, "Notes"), (72, 574, "Locked")])
    c1 = label([(72, 770, "Page two"), (72, 724, "Address (same field)")])
    objs = {
        1: b"<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [10 0 R 11 0 R 12 0 R] "
           b"/DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 20 0 R >> >> >> >>",
        2: b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        3: f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Contents 5 0 R "
           f"/Resources << /Font << /F1 21 0 R >> >> /Annots [30 0 R 11 0 R 12 0 R] >>".encode(),
        4: f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Contents 6 0 R "
           f"/Resources << /Font << /F1 21 0 R >> >> /Annots [31 0 R] >>".encode(),
        5: b"<< /Length %d >>\nstream\n" % len(c0) + c0 + b"\nendstream",
        6: b"<< /Length %d >>\nstream\n" % len(c1) + c1 + b"\nendstream",
        10: b"<< /FT /Tx /T (address) /Kids [30 0 R 31 0 R] /DA (/Helv 10 Tf 0 g) >>",
        30: b"<< /Type /Annot /Subtype /Widget /Parent 10 0 R /P 3 0 R /F 4 /Rect [180 716 480 736] >>",
        31: b"<< /Type /Annot /Subtype /Widget /Parent 10 0 R /P 4 0 R /F 4 /Rect [200 716 480 736] >>",
        11: b"<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 4096 /T (notes) /P 3 0 R /F 4 "
            b"/Rect [180 610 480 686] /DA (/Helv 10 Tf 0 g) >>",
        12: b"<< /Type /Annot /Subtype /Widget /FT /Tx /Ff 1 /T (locked) /V (Fixed) /P 3 0 R /F 4 "
            b"/Rect [180 566 480 586] /DA (/Helv 10 Tf 0 g) >>",
        20: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        21: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    }
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    for num in sorted(objs):
        offsets[num] = len(out)
        out += b"%d 0 obj\n" % num + objs[num] + b"\nendobj\n"
    size = max(objs) + 1
    xref = len(out)
    out += b"xref\n0 %d\n" % size
    for num in range(size):
        if num in offsets:
            out += b"%010d 00000 n \n" % offsets[num]
        else:
            out += b"0000000000 65535 f \n"
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (size, xref)
    path.write_bytes(bytes(out))

def pdf_encrypted(src: Path, out_user: Path, out_owner: Path) -> None:
    from pypdf import PdfReader, PdfWriter
    from pypdf.constants import UserAccessPermissions as P

    reader = PdfReader(str(src))
    w = PdfWriter()
    for page in reader.pages[:5]:
        w.add_page(page)
    w.add_metadata({"/Title": "Kitab fixture: password kitab"})
    w.encrypt(user_password="kitab", owner_password="kitab-owner", algorithm="AES-256")
    with out_user.open("wb") as f:
        w.write(f)

    w = PdfWriter()
    for page in reader.pages[:5]:
        w.add_page(page)
    w.add_metadata({"/Title": "Kitab fixture: opens without password, print and copy restricted"})
    perms = P.all() & ~P.PRINT & ~P.PRINT_TO_REPRESENTATION & ~P.EXTRACT & ~P.EXTRACT_TEXT_AND_GRAPHICS
    w.encrypt(user_password="", owner_password="kitab-owner", algorithm="AES-256", permissions_flag=perms)
    with out_owner.open("wb") as f:
        w.write(f)


def pdf_damaged(src: Path, out_truncated: Path, out_bad_xref: Path, out_not_pdf: Path) -> None:
    data = src.read_bytes()
    out_truncated.write_bytes(data[: int(len(data) * 0.6)])
    i = data.rfind(b"startxref")
    j = data.find(b"\n", i + 10)
    bad = data[: i + 10] + b"999999" + data[j:]
    out_bad_xref.write_bytes(bad)
    out_not_pdf.write_bytes(b"This is a text file with a .pdf extension.\n")


def pdf_unicode(path: Path) -> None:
    """Bangla, Arabic (RTL), Hebrew, CJK via PyMuPDF's HTML layout (HarfBuzz shaping)."""
    import fitz

    html = """
    <h1>Scripts</h1>
    <p lang="bn">কিতাব একটি দ্রুত, হালকা পাঠক। যুক্তাক্ষর: ক্ষ, জ্ঞ, ন্ত্র, স্ক্র।</p>
    <p dir="rtl" lang="ar">كتاب قارئ سريع وخفيف.</p>
    <p dir="rtl" lang="he">ספר הוא קורא מהיר.</p>
    <p lang="zh">中文：快速、轻量的阅读器。</p>
    <p lang="ja">日本語：速くて軽いリーダーです。</p>
    <p lang="ko">한국어: 빠르고 가벼운 리더입니다.</p>
    <p>Latin with diacritics: café, naïve, Ångström, Dvořák.</p>
    """
    doc = fitz.open()
    page = doc.new_page(width=595, height=842)
    page.insert_htmlbox(fitz.Rect(56, 56, 539, 786), html, css="* { font-size: 14pt; } h1 { font-size: 22pt; }")
    doc.set_metadata({"title": "Kitab fixture: Unicode and RTL", "author": "Kitab test suite"})
    doc.save(str(path), garbage=3, deflate=True)


def pdf_annotations(src: Path, path: Path) -> None:
    import fitz

    doc = fitz.open(str(src))
    doc.select([0, 1, 2])
    page = doc[0]
    hits = page.search_for("kitab-needle-01")
    if hits:
        a = page.add_highlight_annot(hits[0]); a.set_colors(stroke=(1, 0.8, 0.2)); a.update()
    words = page.get_text("words")
    if len(words) > 40:
        r = fitz.Rect(words[30][:4]) | fitz.Rect(words[34][:4])
        page.add_underline_annot(r).update()
        r2 = fitz.Rect(words[38][:4]) | fitz.Rect(words[40][:4])
        page.add_strikeout_annot(r2).update()
    page.add_text_annot(fitz.Point(500, 700), "A sticky note from the fixture.").update()
    page.add_freetext_annot(fitz.Rect(360, 160, 540, 200), "Free text box", fontsize=11).update()
    ink = page.add_ink_annot([[(100, 300), (140, 330), (180, 300), (220, 330), (260, 300)]])
    ink.set_border(width=2); ink.set_colors(stroke=(0.2, 0.4, 0.9)); ink.update()
    page.add_rect_annot(fitz.Rect(70, 420, 300, 470)).update()
    doc.set_metadata({"title": "Kitab fixture: existing annotations"})
    doc.save(str(path), garbage=3, deflate=True)


def pdf_scanned_and_images(path: Path) -> None:
    """Page 1: image-only 'scan', no text layer. Page 2: three embedded images for Extract images."""
    import fitz
    from PIL import Image, ImageDraw, ImageFilter

    rng = random.Random(10)
    img = Image.new("L", (1240, 1754), 245)
    d = ImageDraw.Draw(img)
    y = 140
    for _ in range(40):
        x = 120
        while x < 1100:
            wlen = rng.randint(30, 110)
            d.rectangle((x, y, x + wlen, y + 14), fill=rng.randint(40, 70))
            x += wlen + 14
        y += 36
    img = img.rotate(0.7, fillcolor=245).filter(ImageFilter.GaussianBlur(0.6))
    scan = io.BytesIO(); img.save(scan, "PNG")

    doc = fitz.open()
    p1 = doc.new_page(width=595, height=842)
    p1.insert_image(p1.rect, stream=scan.getvalue())
    p2 = doc.new_page(width=595, height=842)
    p2.insert_text((56, 70), "Three embedded images: PNG with alpha, JPEG, and a 1-bit image.", fontsize=12)
    a = Image.new("RGBA", (300, 200), (0, 0, 0, 0)); ImageDraw.Draw(a).ellipse((20, 20, 280, 180), fill=(230, 180, 80, 200))
    b = Image.new("RGB", (300, 200)); ImageDraw.Draw(b).rectangle((0, 0, 300, 200), fill=(60, 120, 200)); ImageDraw.Draw(b).text((20, 90), "JPEG", fill=(255, 255, 255))
    c1 = Image.new("1", (300, 200), 1); ImageDraw.Draw(c1).line((0, 0, 300, 200), fill=0, width=6)
    for i, (im, fmt) in enumerate([(a, "PNG"), (b, "JPEG"), (c1, "PNG")]):
        s = io.BytesIO(); im.save(s, fmt)
        p2.insert_image(fitz.Rect(56, 100 + i * 220, 356, 300 + i * 220), stream=s.getvalue())
    doc.set_metadata({"title": "Kitab fixture: scanned page and embedded images"})
    doc.save(str(path), garbage=3, deflate=True)


def pdf_large(path: Path) -> None:
    """300 pages, each with its own incompressible JPEG: about 20 MB. Not committed."""
    from PIL import Image
    from reportlab.lib.pagesizes import A4
    from reportlab.pdfgen import canvas

    rng = random.Random(6)
    tmp = Path(tempfile.mkdtemp(prefix="kitab-large-"))
    try:
        c = canvas.Canvas(str(path), pagesize=A4, invariant=1)
        c.setTitle("Kitab fixture: 300 pages, about 20 MB")
        for i in range(1, 301):
            px = bytes(rng.getrandbits(8) for _ in range(260 * 260 * 3))
            im = Image.frombytes("RGB", (260, 260), px)
            f = tmp / f"{i}.jpg"; im.save(f, "JPEG", quality=88)
            c.setFont("Helvetica-Bold", 18); c.drawString(72, 770, f"Page {i} of 300")
            c.setFont("Helvetica", 10); c.drawString(72, 750, f"kitab-large-needle-{i:03d}")
            c.drawImage(str(f), 72, 380, 450, 350)
            c.showPage()
        c.save()
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def make_test_pki() -> dict:
    """A throwaway root CA plus two signer certificates. Test keys only, password 'kitab'."""
    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import ec
    from cryptography.hazmat.primitives.serialization import pkcs12
    from cryptography.x509.oid import NameOID, ExtendedKeyUsageOID

    CERTS.mkdir(parents=True, exist_ok=True)
    not_before = dt.datetime(2026, 1, 1, tzinfo=dt.timezone.utc)
    not_after = dt.datetime(2036, 1, 1, tzinfo=dt.timezone.utc)

    def name(cn: str) -> x509.Name:
        return x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, cn), x509.NameAttribute(NameOID.ORGANIZATION_NAME, "Kitab Test Suite")])

    root_key = ec.generate_private_key(ec.SECP256R1())
    root = (
        x509.CertificateBuilder().subject_name(name("Kitab Test Root CA")).issuer_name(name("Kitab Test Root CA"))
        .public_key(root_key.public_key()).serial_number(1).not_valid_before(not_before).not_valid_after(not_after)
        .add_extension(x509.BasicConstraints(ca=True, path_length=None), critical=True)
        .add_extension(x509.KeyUsage(True, False, False, False, False, True, True, False, False), critical=True)
        .sign(root_key, hashes.SHA256())
    )
    (CERTS / "kitab-test-root-ca.pem").write_bytes(root.public_bytes(serialization.Encoding.PEM))
    out = {"root": root}
    for serial, cn, file in [(2, "R. Haque (test landlord)", "landlord.p12"), (3, "A. Rahman (test tenant)", "tenant.p12")]:
        key = ec.generate_private_key(ec.SECP256R1())
        cert = (
            x509.CertificateBuilder().subject_name(name(cn)).issuer_name(root.subject).public_key(key.public_key())
            .serial_number(serial).not_valid_before(not_before).not_valid_after(not_after)
            .add_extension(x509.KeyUsage(True, True, False, False, False, False, False, False, False), critical=True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.EMAIL_PROTECTION]), critical=False)
            .sign(root_key, hashes.SHA256())
        )
        p12 = pkcs12.serialize_key_and_certificates(cn.encode(), key, cert, [root], serialization.BestAvailableEncryption(b"kitab"))
        (CERTS / file).write_bytes(p12)
        out[file] = CERTS / file
    return out


def pdf_lease_base(path: Path) -> None:
    """Uncompressed two-page lease with two signature boxes, the base for the signing fixtures."""
    from reportlab.lib.pagesizes import A4
    from reportlab.pdfgen import canvas

    c = canvas.Canvas(str(path), pagesize=A4, invariant=1, pageCompression=0)
    c.setTitle("Kitab fixture: lease agreement (test)")
    c.setFont("Helvetica-Bold", 18); c.drawString(72, 770, "Lease agreement (test document)")
    c.setFont("Helvetica", 11)
    lines = [
        "4.1 The term begins on the start date and continues for twelve months.",
        "4.2 Either party may renew by written notice sixty days before the term ends.",
        "4.3 Either party may end this agreement if the other breaches a material term.",
        "Rent: TAMPER-TARGET 25000 per month.",
    ]
    for i, line in enumerate(lines):
        c.drawString(72, 730 - i * 22, line)
    c.drawString(72, 520, "Landlord"); c.rect(72, 540, 200, 60)
    c.drawString(320, 520, "Tenant"); c.rect(320, 540, 200, 60)
    c.showPage()
    c.drawString(72, 770, "Schedule A: inventory (page 2).")
    c.showPage()
    c.save()


def pdf_signed(base: Path, pki: dict) -> list[str]:
    try:
        from pyhanko.pdf_utils.incremental_writer import IncrementalPdfFileWriter
        from pyhanko.sign import signers
        from pyhanko.sign.fields import SigFieldSpec, append_signature_field
    except ImportError:
        return ["skipped signing fixtures: pip install pyhanko"]

    def sign(src: Path, dst: Path, p12: Path, field: str, add_fields: list[tuple[str, tuple]] | None = None) -> None:
        signer = signers.SimpleSigner.load_pkcs12(str(p12), passphrase=b"kitab")
        with src.open("rb") as inf:
            w = IncrementalPdfFileWriter(inf)
            for fname, box in add_fields or []:
                append_signature_field(w, SigFieldSpec(sig_field_name=fname, on_page=0, box=box))
            meta = signers.PdfSignatureMetadata(field_name=field, reason="Kitab test signature", location="Dhaka")
            with dst.open("wb") as outf:
                signers.sign_pdf(w, meta, signer=signer, output=outf)

    one = PDF / "08a-signed-valid.pdf"
    sign(base, one, pki["landlord.p12"], "Landlord", [("Landlord", (72, 540, 272, 600)), ("Tenant", (320, 540, 520, 600))])

    two = PDF / "08b-signed-two-signers.pdf"
    sign(one, two, pki["tenant.p12"], "Tenant")

    import fitz
    modified = PDF / "08c-signed-then-modified.pdf"
    shutil.copy(one, modified)
    doc = fitz.open(str(modified))
    doc[0].add_text_annot(fitz.Point(500, 700), "Added after signing").update()
    doc.saveIncr()
    doc.close()

    tampered = PDF / "08d-signed-tampered.pdf"
    data = bytearray(one.read_bytes())
    i = data.find(b"25000")
    if i < 0:
        return ["could not find tamper target"]
    data[i:i + 5] = b"95000"
    tampered.write_bytes(bytes(data))
    return []


# -------------------------------------------------------------------------- EPUB


CONTAINER = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""

CSS = "body { font-family: serif; line-height: 1.5; margin: 0 5%; } h1 { font-size: 1.6em; } .note { font-size: 0.9em; }"


def xhtml(title: str, body: str, lang: str = "en", dir_: str = "ltr", head: str = "") -> str:
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="{lang}" xml:lang="{lang}" dir="{dir_}">
<head><meta charset="UTF-8"/><title>{title}</title><link rel="stylesheet" type="text/css" href="style.css"/>{head}</head>
<body>{body}</body>
</html>"""


def write_epub(path: Path, *, title: str, ident: str, chapters: list[tuple[str, str]], version: str = "3.0",
               lang: str = "en", dir_: str = "ltr", ppd: str | None = None, extra: dict[str, tuple[bytes, str]] | None = None,
               fixed: bool = False, nav: bool = True, ncx: bool = False, cover: bytes | None = None,
               skip_container: bool = False, broken_href: bool = False) -> None:
    extra = dict(extra or {})
    if cover:
        extra["cover.png"] = (cover, "image/png")
    items, spine, navpoints = [], [], []
    files: dict[str, bytes] = {"OEBPS/style.css": CSS.encode()}
    for i, (ctitle, body) in enumerate(chapters, 1):
        fname = f"ch{i:03d}.xhtml"
        head = '<meta name="viewport" content="width=600, height=800"/>' if fixed else ""
        files[f"OEBPS/{fname}"] = xhtml(ctitle, body, lang, dir_, head).encode()
        href = fname if not (broken_href and i == 2) else "missing-chapter.xhtml"
        items.append(f'<item id="c{i}" href="{href}" media-type="application/xhtml+xml"/>')
        spine.append(f'<itemref idref="c{i}"/>')
        navpoints.append((fname, ctitle))
    for name, (data, mt) in extra.items():
        files[f"OEBPS/{name}"] = data
        props = ' properties="cover-image"' if name == "cover.png" and version.startswith("3") else ""
        items.append(f'<item id="x-{name.replace(".", "-").replace("/", "-")}" href="{name}" media-type="{mt}"{props}/>')
    items.append('<item id="css" href="style.css" media-type="text/css"/>')
    if nav and version.startswith("3"):
        lis = "".join(f'<li><a href="{h}">{t}</a></li>' for h, t in navpoints)
        files["OEBPS/nav.xhtml"] = xhtml("Contents", f'<nav epub:type="toc" id="toc"><h1>Contents</h1><ol>{lis}</ol></nav>', lang, dir_).encode()
        items.append('<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>')
    if ncx:
        pts = "".join(
            f'<navPoint id="n{i}" playOrder="{i}"><navLabel><text>{t}</text></navLabel><content src="{h}"/></navPoint>'
            for i, (h, t) in enumerate(navpoints, 1)
        )
        files["OEBPS/toc.ncx"] = f"""<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="{ident}"/></head>
<docTitle><text>{title}</text></docTitle><navMap>{pts}</navMap></ncx>""".encode()
        items.append('<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>')
    meta_extra = ""
    if version.startswith("3"):
        meta_extra += '<meta property="dcterms:modified">2026-11-06T09:00:00Z</meta>'
        if fixed:
            meta_extra += '<meta property="rendition:layout">pre-paginated</meta><meta property="rendition:spread">none</meta>'
    if cover and not version.startswith("3"):
        meta_extra += '<meta name="cover" content="x-cover-png"/>'
    spine_attrs = ' toc="ncx"' if ncx else ""
    if ppd:
        spine_attrs += f' page-progression-direction="{ppd}"'
    opf = f"""<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="uid" xml:lang="{lang}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">{ident}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>Kitab test suite</dc:creator>
    <dc:language>{lang}</dc:language>
    {meta_extra}
  </metadata>
  <manifest>{''.join(items)}</manifest>
  <spine{spine_attrs}>{''.join(spine)}</spine>
</package>"""
    files["OEBPS/content.opf"] = opf.encode()
    if not skip_container:
        files["META-INF/container.xml"] = CONTAINER.encode()

    with zipfile.ZipFile(path, "w") as z:
        zi = zipfile.ZipInfo("mimetype", date_time=(2026, 11, 6, 9, 0, 0))
        z.writestr(zi, "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        for name in sorted(files):
            zi = zipfile.ZipInfo(name, date_time=(2026, 11, 6, 9, 0, 0))
            z.writestr(zi, files[name], compress_type=zipfile.ZIP_DEFLATED)


def png_bytes(w: int, h: int, color: tuple, label: str) -> bytes:
    from PIL import Image, ImageDraw
    im = Image.new("RGB", (w, h), color)
    ImageDraw.Draw(im).text((12, 12), label, fill=(255, 255, 255))
    b = io.BytesIO(); im.save(b, "PNG"); return b.getvalue()


def epubs() -> None:
    rng = random.Random(3)

    def chap(n: int, needle: str = "") -> str:
        return "".join(f"<p>{paragraph(rng)}</p>" for _ in range(n)) + (f"<p>{needle}</p>" if needle else "")

    cover = png_bytes(600, 900, (40, 40, 48), "Kitab EPUB 3 cover")
    write_epub(EPUB / "01-epub3-basic.epub", title="Kitab fixture: EPUB 3 basic", ident="urn:uuid:kitab-epub-01",
               chapters=[(f"Chapter {i}", f"<h1>Chapter {i}</h1>" + chap(12, f"kitab-epub-needle-{i}")) for i in range(1, 6)],
               cover=cover)

    write_epub(EPUB / "02-epub2-ncx.epub", title="Kitab fixture: EPUB 2 with NCX", ident="kitab-epub-02", version="2.0",
               nav=False, ncx=True, cover=png_bytes(600, 900, (120, 60, 30), "EPUB 2 cover"),
               chapters=[(f"Part {i}", f"<h1>Part {i}</h1>" + chap(8)) for i in range(1, 5)])

    svg = b'<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120"><rect width="200" height="120" rx="12" fill="#E6B450"/><text x="20" y="66" font-size="20">SVG image</text></svg>'
    body = (
        '<h1>Notes and images</h1>'
        '<p>A sentence with a footnote<a epub:type="noteref" href="#fn1" id="r1">1</a> and another<a epub:type="noteref" href="#fn2" id="r2">2</a>.</p>'
        '<figure><img src="photo.png" alt="Blue test image"/><figcaption>PNG figure with caption.</figcaption></figure>'
        '<p><img src="diagram.svg" alt="SVG test image"/></p>'
        '<table><thead><tr><th>Format</th><th>Pages</th></tr></thead><tbody><tr><td>PDF</td><td>40</td></tr><tr><td>EPUB</td><td>5</td></tr></tbody></table>'
        + chap(4) +
        '<aside epub:type="footnote" id="fn1"><p class="note">1. The first footnote, shown as a popover.</p></aside>'
        '<aside epub:type="footnote" id="fn2"><p class="note">2. The second footnote.</p></aside>'
    )
    write_epub(EPUB / "03-footnotes-images.epub", title="Kitab fixture: footnotes, images, table", ident="urn:uuid:kitab-epub-03",
               chapters=[("Notes and images", body), ("After", "<h1>After</h1>" + chap(6))],
               extra={"photo.png": (png_bytes(480, 300, (50, 100, 200), "PNG"), "image/png"), "diagram.svg": (svg, "image/svg+xml")})

    ar = "<p>كتاب قارئ سريع وخفيف. يجب أن تبدأ الصفحات من اليمين.</p>" * 12
    write_epub(EPUB / "04-rtl-arabic.epub", title="كتاب: اختبار من اليمين إلى اليسار", ident="urn:uuid:kitab-epub-04", lang="ar", dir_="rtl", ppd="rtl",
               chapters=[(f"الفصل {i}", f"<h1>الفصل {i}</h1>{ar}") for i in range(1, 4)])

    bn = "<p>কিতাব একটি দ্রুত, হালকা পাঠক। যুক্তাক্ষর: ক্ষ, জ্ঞ, ন্ত্র, স্ক্র।</p>" * 10
    cjk_v = '<div style="writing-mode: vertical-rl; height: 20em">' + "<p>縦書きのテスト。速くて軽いリーダーです。</p>" * 8 + "</div>"
    write_epub(EPUB / "05-bangla-cjk.epub", title="Kitab fixture: Bangla and CJK", ident="urn:uuid:kitab-epub-05", lang="bn",
               chapters=[("বাংলা", f"<h1>বাংলা</h1>{bn}"), ("中文", "<h1 lang='zh'>中文</h1>" + "<p lang='zh'>快速、轻量的阅读器。</p>" * 12),
                         ("縦書き", f"<h1 lang='ja'>縦書き</h1>{cjk_v}")])

    big = [(f"Chapter {i}", f"<h1>Chapter {i}</h1>" + chap(14, f"kitab-epub-large-needle-{i:03d}")) for i in range(1, 301)]
    write_epub(EPUB / "06-large-300-chapters.epub", title="Kitab fixture: 300 chapters", ident="urn:uuid:kitab-epub-06", chapters=big)

    write_epub(EPUB / "07a-broken-no-container.epub", title="Broken: no container.xml", ident="kitab-epub-07a",
               chapters=[("One", "<h1>One</h1>" + chap(3))], skip_container=True)
    write_epub(EPUB / "07b-broken-missing-chapter.epub", title="Broken: spine points at a missing file", ident="kitab-epub-07b",
               chapters=[("One", "<h1>One</h1>" + chap(3)), ("Two", "<h1>Two</h1>" + chap(3)), ("Three", "<h1>Three</h1>" + chap(3))],
               broken_href=True)
    (EPUB / "07c-not-a-zip.epub").write_bytes(b"This is not a zip archive.\n")

    pages = []
    for i in range(1, 7):
        pages.append((f"Page {i}", f'<div style="width:600px;height:800px;background:#{(i * 37) % 255:02x}6080;color:#fff;font-size:48px;display:block">'
                      f'<p style="padding:40px">Fixed page {i}</p></div>'))
    write_epub(EPUB / "08-fixed-layout.epub", title="Kitab fixture: fixed layout", ident="urn:uuid:kitab-epub-08", chapters=pages, fixed=True)

    write_epub(EPUB / "09-no-toc.epub", title="Kitab fixture: no table of contents", ident="urn:uuid:kitab-epub-09", nav=False,
               chapters=[(f"Untitled {i}", chap(6)) for i in range(1, 5)])


# ---------------------------------------------------------------------------- MD


def md_assets_and_large() -> None:
    (MD / "assets").mkdir(parents=True, exist_ok=True)
    (MD / "assets" / "sample.png").write_bytes(png_bytes(640, 360, (230, 180, 80), "md/assets/sample.png"))
    rng = random.Random(4)
    out = ["# Large document", "", "About 2 MB and 20,000 lines. Opening, scrolling, search and the outline must stay smooth.", ""]
    for section in range(1, 401):
        out += [f"## Section {section}", ""]
        for _ in range(8):
            out += [paragraph(rng, 4), ""]
        if section % 25 == 0:
            out += ["```rust", f"fn section_{section}() -> u32 {{ {section} }}", "```", ""]
        if section % 50 == 0:
            out += ["| col | value |", "| --- | ----- |"] + [f"| {i} | {rng.choice(WORDS)} |" for i in range(10)] + [""]
        if section % 100 == 0:
            out += ["```mermaid", "flowchart LR", f"  S{section}[Section {section}] --> N[Next]", "```", ""]
        out += [f"kitab-md-needle-{section:03d}", ""]
    (MD / "10-large.md").write_text("\n".join(out), encoding="utf-8")


# ------------------------------------------------------------------- OTHER


def other_formats() -> None:
    """Phase 4 formats besides EPUB: TXT, FB2, CBZ, XPS, and broken files.
    MOBI/AZW3 can't be written without Calibre; fetch_real_world.py brings a
    Project Gutenberg copy instead."""
    OTHER.mkdir(parents=True, exist_ok=True)
    rng = random.Random(5)

    lines = ["Kitab plain text fixture", ""]
    for i in range(1, 61):
        lines += [paragraph(rng, 4), ""]
        if i % 20 == 0:
            lines += [f"kitab-txt-needle-{i // 20}", ""]
    lines += ["Bangla: কিতাব একটি দ্রুত পাঠক।", "Arabic: كتاب قارئ سريع.", "Chinese: 快速的阅读器。", ""]
    (OTHER / "01-plain.txt").write_text("\n".join(lines), encoding="utf-8")

    def fb2_section(title: str, n: int, needle: str = "") -> str:
        paras = "".join(f"<p>{paragraph(rng)}</p>" for _ in range(n))
        return f"<section><title><p>{title}</p></title>{paras}" + (f"<p>{needle}</p>" if needle else "") + "</section>"
    fb2 = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">'
        "<description><title-info><genre>prose</genre><author><first-name>Kitab</first-name><last-name>Fixture</last-name></author>"
        "<book-title>Kitab fixture: FB2</book-title><lang>en</lang></title-info>"
        "<document-info><id>kitab-fb2-02</id><version>1.0</version></document-info></description>"
        "<body>" + "".join(fb2_section(f"Section {i}", 10, f"kitab-fb2-needle-{i}") for i in range(1, 4))
        + "<section><title><p>বাংলা</p></title><p>কিতাব একটি দ্রুত, হালকা পাঠক। যুক্তাক্ষর: ক্ষ, জ্ঞ।</p></section>"
        "</body></FictionBook>\n"
    )
    (OTHER / "02-book.fb2").write_text(fb2, encoding="utf-8")

    with zipfile.ZipFile(OTHER / "03-comic.cbz", "w") as z:
        for i in range(1, 6):
            zi = zipfile.ZipInfo(f"page-{i:02d}.png", date_time=(2026, 11, 6, 9, 0, 0))
            z.writestr(zi, png_bytes(800, 1200, ((i * 47) % 255, 90, 140), f"Comic page {i}"), compress_type=zipfile.ZIP_STORED)

    # XPS: two fixed pages of vector shapes (Letter portrait, then landscape),
    # at 96 units per inch. No fonts, so no font files to license.
    def page(w: int, h: int, color: str) -> str:
        return (f'<FixedPage xmlns="http://schemas.microsoft.com/xps/2005/06" Width="{w}" Height="{h}" xml:lang="en">'
                f'<Path Data="M 48,48 L {w - 48},48 L {w - 48},{h - 48} L 48,{h - 48} Z" Fill="{color}"/>'
                f'<Path Data="M 96,96 L {w // 2},96 L {w // 2},{h // 2} Z" Fill="#FFFFFFFF"/></FixedPage>')
    ct = ('<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
          '<Default Extension="fpage" ContentType="application/vnd.ms-package.xps-fixedpage+xml"/>'
          '<Default Extension="fdoc" ContentType="application/vnd.ms-package.xps-fixeddocument+xml"/>'
          '<Default Extension="fdseq" ContentType="application/vnd.ms-package.xps-fixeddocumentsequence+xml"/>'
          '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>')
    rels = ('<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="R0" Type="http://schemas.microsoft.com/xps/2005/06/fixedrepresentation" Target="/FixedDocSeq.fdseq"/></Relationships>')
    seq = ('<FixedDocumentSequence xmlns="http://schemas.microsoft.com/xps/2005/06">'
           '<DocumentReference Source="/Documents/1/FixedDoc.fdoc"/></FixedDocumentSequence>')
    doc = ('<FixedDocument xmlns="http://schemas.microsoft.com/xps/2005/06">'
           '<PageContent Source="/Documents/1/Pages/1.fpage"/><PageContent Source="/Documents/1/Pages/2.fpage"/></FixedDocument>')
    with zipfile.ZipFile(OTHER / "04-vector.xps", "w") as z:
        for name, data in [("[Content_Types].xml", ct), ("_rels/.rels", rels), ("FixedDocSeq.fdseq", seq),
                           ("Documents/1/FixedDoc.fdoc", doc), ("Documents/1/Pages/1.fpage", page(816, 1056, "#FF2E6FB0")),
                           ("Documents/1/Pages/2.fpage", page(1056, 816, "#FFC8402A"))]:
            zi = zipfile.ZipInfo(name, date_time=(2026, 11, 6, 9, 0, 0))
            z.writestr(zi, data, compress_type=zipfile.ZIP_DEFLATED)

    (OTHER / "05-not-a-zip.cbz").write_bytes(b"This is not a comic archive.\n")
    (OTHER / "06-broken.fb2").write_text("<FictionBook><body><section><p>never closed", encoding="utf-8")


# ------------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--large", action="store_true", help="also build pdf/06-large-20mb.pdf (about 20 MB, not committed)")
    ap.add_argument("--only-other", action="store_true", help="build only other/ (TXT, FB2, CBZ, XPS)")
    args = ap.parse_args()
    if args.only_other:
        other_formats()
        for f in sorted(OTHER.glob("*")):
            print(f"{f.relative_to(ROOT)}	{f.stat().st_size:>10,}")
        return 0
    for d in (PDF, EPUB, MD):
        d.mkdir(parents=True, exist_ok=True)
    notes: list[str] = []

    pdf_text_outline(PDF / "01-text-outline.pdf")
    pdf_mixed_sizes(PDF / "02-mixed-page-sizes.pdf")
    pdf_forms(PDF / "03-forms.pdf")
    pdf_form_two_pages(PDF / "11-form-two-pages.pdf")
    pdf_encrypted(PDF / "01-text-outline.pdf", PDF / "04a-encrypted-password-kitab.pdf", PDF / "04b-owner-restricted.pdf")
    pdf_damaged(PDF / "01-text-outline.pdf", PDF / "05a-damaged-truncated.pdf", PDF / "05b-damaged-bad-xref.pdf", PDF / "05c-not-a-pdf.pdf")
    if args.large:
        pdf_large(PDF / "06-large-20mb.pdf")
    pdf_unicode(PDF / "07-unicode-rtl.pdf")
    base = PDF / "08-lease-unsigned.pdf"
    pdf_lease_base(base)
    pki = make_test_pki()
    notes += pdf_signed(base, pki)
    pdf_annotations(PDF / "01-text-outline.pdf", PDF / "09-annotations.pdf")
    pdf_scanned_and_images(PDF / "10-scanned-and-images.pdf")

    epubs()
    md_assets_and_large()
    other_formats()

    for n in notes:
        print("note:", n, file=sys.stderr)
    for d in (PDF, EPUB, MD, OTHER):
        for f in sorted(d.rglob("*")):
            if f.is_file():
                print(f"{f.relative_to(ROOT)}\t{f.stat().st_size:>10,}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
