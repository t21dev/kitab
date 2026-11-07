# SPDX-License-Identifier: AGPL-3.0-or-later
"""Check every fixture with MuPDF (the engine Kitab uses) and pyHanko, and write expected.json.

    python tests/fixtures/tools/verify_fixtures.py

expected.json is the oracle the Rust and e2e tests assert against: page counts,
page sizes, search hit counts, outline length, encryption, signature status.
If this script and the app disagree, the app is wrong (or the fixture changed
on purpose and expected.json is regenerated in the same commit).
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import pymupdf as fitz

ROOT = Path(__file__).resolve().parents[1]
fitz.TOOLS.mupdf_display_errors(False)



def md_headings(text: str) -> int:
    """Headings as CommonMark sees them: ATX (`# x`) and setext (a line
    underlined with === or ---), outside code fences, outside frontmatter."""
    lines = text.splitlines()
    i = 0
    if lines and lines[0] == "---":  # frontmatter
        for j in range(1, len(lines)):
            if lines[j] == "---":
                i = j + 1
                break
    n, fence, prev = 0, None, ""
    for line in lines[i:]:
        s = line.lstrip()
        m = re.match(r"(`{3,}|~{3,})", s)
        if m:
            if fence is None:
                fence = m.group(1)[0]
            elif s.startswith(fence * 3) and not s.strip(fence).strip():
                fence = None
            prev = ""
            continue
        if fence is not None:
            continue
        if re.match(r"#{1,6}(\s|$)", s) and len(line) - len(s) < 4:
            n += 1
            prev = ""
            continue
        if re.fullmatch(r"(=+|-+)\s*", line) and prev.strip() and not prev.lstrip().startswith(("-", "*", "+", ">")) and not re.match(r"\d+\.", prev.lstrip()):
            n += 1
            prev = ""
            continue
        prev = line
    return n

def count(doc, needle: str, case: bool = False) -> int:
    n = 0
    for page in doc:
        text = page.get_text()
        n += text.count(needle) if case else text.lower().count(needle.lower())
    return n


def pdf_info(path: Path) -> dict:
    try:
        doc = fitz.open(str(path))
    except Exception as e:  # noqa: BLE001
        return {"opens": False, "error": type(e).__name__}
    info: dict = {"opens": True, "needs_pass": bool(doc.needs_pass), "is_repaired": bool(doc.is_repaired)}
    if doc.needs_pass:
        info["unlocks_with_kitab"] = bool(doc.authenticate("kitab"))
    info["pages"] = doc.page_count
    info["page_sizes_pt"] = [[round(p.rect.width), round(p.rect.height), p.rotation] for p in doc][:12]
    info["outline"] = len(doc.get_toc())
    info["title"] = doc.metadata.get("title") if doc.metadata else None
    info["permissions_print"] = bool(doc.permissions & fitz.PDF_PERM_PRINT)
    info["permissions_copy"] = bool(doc.permissions & fitz.PDF_PERM_COPY)
    info["text_chars_page1"] = len(doc[0].get_text().strip()) if doc.page_count else 0
    info["annots_page1"] = len(list(doc[0].annots())) if doc.page_count else 0
    info["images_page2"] = len(doc[1].get_images()) if doc.page_count > 1 else 0
    info["widgets"] = sum(len(list(p.widgets())) for p in doc)
    info["links_page1"] = len(doc[0].get_links()) if doc.page_count else 0
    return info


def sig_info(path: Path) -> list[dict]:
    try:
        from pyhanko.pdf_utils.reader import PdfFileReader
        from pyhanko.sign.validation import validate_pdf_signature
        from pyhanko_certvalidator import ValidationContext
        from asn1crypto import pem, x509 as ax509
    except ImportError:
        return [{"skipped": "pyhanko not installed"}]
    root_pem = (ROOT / "pdf" / "certs" / "kitab-test-root-ca.pem").read_bytes()
    _, _, der = pem.unarmor(root_pem)
    vc = ValidationContext(trust_roots=[ax509.Certificate.load(der)])
    out = []
    with path.open("rb") as f:
        r = PdfFileReader(f, strict=False)
        for s in r.embedded_signatures:
            try:
                st = validate_pdf_signature(s, vc)
                out.append({
                    "field": s.field_name,
                    "intact": bool(st.intact),
                    "valid": bool(st.valid),
                    "trusted": bool(st.trusted),
                    "coverage": str(st.coverage).split(".")[-1],
                    "modification": str(st.modification_level).split(".")[-1] if st.modification_level else None,
                    "signer": st.signing_cert.subject.human_friendly,
                })
            except Exception as e:  # noqa: BLE001
                out.append({"field": s.field_name, "error": type(e).__name__})
    return out


def main() -> int:
    exp: dict = {"pdf": {}, "epub": {}, "md": {}}
    fails: list[str] = []

    for p in sorted((ROOT / "pdf").glob("*.pdf")):
        exp["pdf"][p.name] = pdf_info(p)
        if p.name.startswith("08") and p.name != "08-lease-unsigned.pdf":
            exp["pdf"][p.name]["signatures"] = sig_info(p)

    t = fitz.open(str(ROOT / "pdf" / "01-text-outline.pdf"))
    exp["pdf"]["01-text-outline.pdf"]["search"] = {
        "lantern (case-insensitive, substring)": count(t, "lantern"),
        "Lantern (case-sensitive)": count(t, "Lantern", case=True),
        "kitab-needle-17": count(t, "kitab-needle-17"),
    }

    # Kitab's MuPDF build leaves out MuPDF's Charis SIL (size), so books set
    # in the default face use Times (URW Nimbus Roman). PyMuPDF ships Charis;
    # count pages with Kitab's face so the numbers are Kitab's.
    fitz.mupdf.fz_set_user_css("body { font-family: Times }")
    for p in sorted((ROOT / "epub").glob("*.epub")):
        try:
            d = fitz.open(str(p))
            d.layout(width=400, height=600, fontsize=11)
            exp["epub"][p.name] = {"opens": True, "pages_at_400x600_11pt": d.page_count, "outline": len(d.get_toc()),
                                   "title": (d.metadata or {}).get("title")}
        except Exception as e:  # noqa: BLE001
            exp["epub"][p.name] = {"opens": False, "error": type(e).__name__}

    # Other formats (other/): same layout and face as EPUB for reflowable ones.
    exp.setdefault("other", {})
    for p in sorted((ROOT / "other").glob("*")):
        try:
            d = fitz.open(str(p))
            if d.is_reflowable:
                d.layout(width=400, height=600, fontsize=11)
            needle = {"txt": "kitab-txt-needle-", "fb2": "kitab-fb2-needle-"}.get(p.suffix[1:])
            info = {"opens": True, "pages": d.page_count, "reflowable": bool(d.is_reflowable),
                    "page_sizes_pt": [[round(d[i].rect.width), round(d[i].rect.height)] for i in range(min(d.page_count, 2))],
                    "outline": len(d.get_toc()), "title": (d.metadata or {}).get("title") or None}
            if needle:
                info["needle"] = needle
                info["needle_hits"] = sum(len(d[i].search_for(needle)) for i in range(d.page_count))
            exp["other"][p.name] = info
        except Exception as e:  # noqa: BLE001
            exp["other"][p.name] = {"opens": False, "error": type(e).__name__}

    for p in sorted((ROOT / "md").glob("*.md")):
        text = p.read_text(encoding="utf-8")
        exp["md"][p.name] = {"bytes": p.stat().st_size, "lines": text.count("\n") + 1,
                             "mermaid_blocks": sum(1 for l in text.splitlines() if l.lstrip().startswith("```mermaid")), "headings": md_headings(text)}

    # Assertions the fixtures are built to satisfy. A failure means the generator is broken.
    P, E = exp["pdf"], exp["epub"]
    checks = [
        (P["01-text-outline.pdf"]["pages"] == 40, "01 has 40 pages"),
        (P["01-text-outline.pdf"]["outline"] >= 8, "01 has an outline"),
        (P["01-text-outline.pdf"]["links_page1"] == 2, "01 page 1 has two links"),
        (P["01-text-outline.pdf"]["search"]["kitab-needle-17"] == 1, "needle 17 occurs once"),
        (len({tuple(s[:2]) for s in P["02-mixed-page-sizes.pdf"]["page_sizes_pt"]}) >= 5, "02 has five distinct sizes"),
        (P["02-mixed-page-sizes.pdf"]["page_sizes_pt"][5][2] == 90, "02 page 6 is rotated 90"),
        (P["03-forms.pdf"]["widgets"] >= 6, "03 has form widgets"),
        (P["04a-encrypted-password-kitab.pdf"]["needs_pass"] and P["04a-encrypted-password-kitab.pdf"]["unlocks_with_kitab"], "04a needs password kitab"),
        (not P["04b-owner-restricted.pdf"]["needs_pass"] and not P["04b-owner-restricted.pdf"]["permissions_print"], "04b opens, print restricted"),
        (P["05c-not-a-pdf.pdf"]["opens"] is False, "05c is rejected"),
        (P["07-unicode-rtl.pdf"]["text_chars_page1"] > 100, "07 has a text layer"),
        (P["09-annotations.pdf"]["annots_page1"] >= 6, "09 has annotations"),
        (P["10-scanned-and-images.pdf"]["text_chars_page1"] == 0, "10 page 1 has no text layer"),
        (P["10-scanned-and-images.pdf"]["images_page2"] == 3, "10 page 2 has three images"),
        (E["01-epub3-basic.epub"]["opens"] and E["01-epub3-basic.epub"]["outline"] == 5, "epub 01 has 5 toc entries"),
        (E["06-large-300-chapters.epub"]["outline"] == 300, "epub 06 has 300 toc entries"),
        (E["07c-not-a-zip.epub"]["opens"] is False, "epub 07c is rejected"),
        (exp["other"]["01-plain.txt"]["needle_hits"] == 3, "txt has three needles"),
        (exp["other"]["02-book.fb2"]["needle_hits"] == 3 and exp["other"]["02-book.fb2"]["title"] == "Kitab fixture: FB2", "fb2 has three needles and a title"),
        (exp["other"]["03-comic.cbz"]["pages"] == 5, "cbz has five pages"),
        (exp["other"]["04-vector.xps"]["page_sizes_pt"] == [[612, 792], [792, 612]], "xps is Letter portrait then landscape"),
        (exp["other"]["05-not-a-zip.cbz"]["opens"] is False, "a non-zip cbz is rejected"),
    ]
    sig = {k: v.get("signatures", []) for k, v in P.items() if k.startswith("08") and "signatures" in v}
    if sig and "skipped" not in json.dumps(sig):
        checks += [
            (sig["08a-signed-valid.pdf"][0].get("valid") and sig["08a-signed-valid.pdf"][0].get("intact"), "08a valid"),
            (len(sig["08b-signed-two-signers.pdf"]) == 2 and all(s.get("intact") for s in sig["08b-signed-two-signers.pdf"]), "08b both intact"),
            (sig["08c-signed-then-modified.pdf"][0].get("intact") and sig["08c-signed-then-modified.pdf"][0].get("coverage") != "ENTIRE_FILE", "08c intact but not whole file"),
            (not sig["08d-signed-tampered.pdf"][0].get("intact", True), "08d broken"),
        ]
    for ok, label in checks:
        print(("PASS " if ok else "FAIL ") + label)
        if not ok:
            fails.append(label)

    (ROOT / "expected.json").write_text(json.dumps(exp, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"wrote {ROOT / 'expected.json'}")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
