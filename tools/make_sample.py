#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Print assets/sample/welcome.html to assets/sample/welcome.pdf with
Microsoft Edge (headless), the guide a new Home offers. Headings become the
PDF's outline (bookmarks). Run after editing the HTML; the PDF is committed
and embedded in kitab.exe, so builds don't need Edge.

    python tools/make_sample.py
"""
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "assets" / "sample" / "welcome.html"
OUT = ROOT / "assets" / "sample" / "welcome.pdf"


def edge() -> str:
    for base in (os.environ.get("ProgramFiles(x86)"), os.environ.get("ProgramFiles")):
        if base:
            p = pathlib.Path(base) / "Microsoft" / "Edge" / "Application" / "msedge.exe"
            if p.exists():
                return str(p)
    found = shutil.which("msedge") or shutil.which("microsoft-edge")
    if not found:
        sys.exit("Microsoft Edge not found")
    return found


def main() -> None:
    profile = tempfile.mkdtemp(prefix="kitab-sample-")
    subprocess.run(
        [
            edge(),
            "--headless",
            "--disable-gpu",
            "--no-pdf-header-footer",
            "--generate-pdf-document-outline",
            f"--user-data-dir={profile}",
            f"--print-to-pdf={OUT}",
            SRC.as_uri(),
        ],
        check=True,
        timeout=120,
    )
    shutil.rmtree(profile, ignore_errors=True)
    print(f"{OUT} ({OUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
