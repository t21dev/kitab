# SPDX-License-Identifier: AGPL-3.0-or-later
"""Download a few real-world files into tests/fixtures/real/ (gitignored) and check their hashes.

    python tests/fixtures/tools/fetch_real_world.py          # fetch and verify
    python tests/fixtures/tools/fetch_real_world.py --pin    # print hashes for new entries

Synthetic fixtures prove behaviour; these prove Kitab copes with files made by
other tools. Every source is public domain or openly licensed (see LICENSE column).
"""

from __future__ import annotations

import hashlib
import sys
import urllib.request
from pathlib import Path

REAL = Path(__file__).resolve().parents[1] / "real"
UA = "kitab-fixtures/1.0"

# name, url, sha256 (None = not pinned yet), licence / origin
SOURCES = [
    ("meditations-gutenberg-2680.epub", "https://www.gutenberg.org/ebooks/2680.epub3.images", "62c71326bea242893ce8c3ce54bdf5fad3989a827a1462ec03f7dadf8bf5bc89",
     "Public domain (Project Gutenberg #2680, Marcus Aurelius, tr. George Long)"),
    ("alice-gutenberg-11.epub", "https://www.gutenberg.org/ebooks/11.epub3.images", "4f81245d31380ac1d6eafb9a321e3691369bc70985736ae4bccf849a4f784036",
     "Public domain (Project Gutenberg #11)"),
    ("alice-gutenberg-11.azw3", "https://www.gutenberg.org/ebooks/11.kf8.images", "d8211269aed55ded8302f289950936db8368b4d4d384c463b722034152f8f1d2",
     "Public domain (Project Gutenberg #11, Kindle KF8 edition)"),
    ("alice-gutenberg-11.mobi", "https://www.gutenberg.org/ebooks/11.kindle.images", "1b4222ffdf1862a39d0fae6d42c588f28a34c12ad73cc21c2849fe2646cac3e1",
     "Public domain (Project Gutenberg #11, older Kindle MOBI edition)"),
    ("tracemonkey-pldi-09.pdf", "https://raw.githubusercontent.com/mozilla/pdf.js/master/test/pdfs/tracemonkey.pdf", "3662ff519e485810520552bf301d8c3b2b917fd2f83303f4965d7abed367e113",
     "pdf.js test corpus (Apache-2.0 repository); two-column academic paper"),
    ("irs-f1040-2024.pdf", "https://www.irs.gov/pub/irs-prior/f1040--2024.pdf", "0a7a54354283044cb41373c6faabfa50955d44bdf91b4b76fa1ca2bf13f6d718",
     "US government work, public domain; real AcroForm"),
    ("commonmark-spec.md", "https://raw.githubusercontent.com/commonmark/commonmark-spec/master/spec.txt", "695b4b2137292a32ab2bf1f009fc0ff2097687c7a8ce0b1b2ffebbd87caaccad",
     "CommonMark spec, CC BY-SA 4.0; large real Markdown"),
    ("mermaid-readme.md", "https://raw.githubusercontent.com/mermaid-js/mermaid/develop/README.md", "59ad779fc9c0971be567e3b5f4a4257f5519d3078c3a9e1da5be9b14c9c12b23",
     "Mermaid README, MIT; real-world Mermaid blocks"),
]


def sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main() -> int:
    pin = "--pin" in sys.argv
    REAL.mkdir(parents=True, exist_ok=True)
    bad = 0
    for name, url, want, lic in SOURCES:
        dst = REAL / name
        if not dst.exists():
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as r:
                dst.write_bytes(r.read())
        got = sha256(dst)
        if want and got != want:
            # Upstream sometimes regenerates files (Gutenberg, master branches).
            # Delete the file and rerun with --pin after checking the new copy.
            print(f"HASH MISMATCH {name}: {got}")
            bad += 1
        else:
            print(f"{'ok ' if want else 'new'} {name:36} {dst.stat().st_size:>10,}  {got if pin or not want else ''}")
    (REAL / "SOURCES.md").write_text(
        "# Real-world fixtures\n\nFetched by `tools/fetch_real_world.py`. Not committed.\n\n| File | Source | Licence |\n|---|---|---|\n"
        + "".join(f"| {n} | {u} | {l} |\n" for n, u, _, l in SOURCES),
        encoding="utf-8",
    )
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
