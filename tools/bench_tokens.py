#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""How many tokens an AI agent spends reading a document, with and without
Kitab (GitHub: Kitab CLI).

For each file it asks Claude's token counter (the free, rate-limited
`messages.count_tokens` endpoint) how many input tokens these cost:

    pdf        the PDF itself, sent as a document block: what Claude Code and
               other agents send when they read a PDF (each page's text plus
               an image of the page)
    markdown   `kitab markdown FILE`: headings, lists and paragraphs as text
    text       `kitab text FILE`: plain text

and prints the saving. EPUB, MOBI and other formats have no "pdf" column:
agents can't read them directly at all, which is the other half of the point.

    pip install anthropic
    python tools/bench_tokens.py [--kitab target/release/kitab-cli] [--model claude-opus-5-5]
                                 [--json out.json] FILE...

Needs Claude credentials (ANTHROPIC_API_KEY, or `ant auth login`). Only the
documents named are sent, to Anthropic's token counter; nothing is generated.
"""

import argparse
import base64
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Claude's document limits: larger PDFs can't be sent whole.
MAX_PDF_BYTES = 32 * 1024 * 1024
MAX_PDF_PAGES = 600


def kitab(cli: str, *args: str) -> str:
    out = subprocess.run([cli, *args], capture_output=True, check=True)
    return out.stdout.decode("utf-8")


def page_count(cli: str, path: pathlib.Path) -> int:
    info = json.loads(kitab(cli, "info", str(path), "--json"))
    return int(info.get("pages") or info.get("page_count") or 0)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("files", nargs="+", type=pathlib.Path)
    exe = "kitab-cli.exe" if sys.platform == "win32" else "kitab-cli"
    ap.add_argument("--kitab", default=str(ROOT / "target" / "release" / exe))
    ap.add_argument("--model", default="claude-opus-5-5")
    ap.add_argument("--json", type=pathlib.Path)
    args = ap.parse_args()

    from anthropic import Anthropic  # imported late: --help works without it

    client = Anthropic()

    def count(content) -> int:
        return client.messages.count_tokens(
            model=args.model, messages=[{"role": "user", "content": content}]
        ).input_tokens

    # The empty message's own overhead, taken off every count so each column
    # is the document alone.
    base = count("x")
    rows = []
    for path in args.files:
        pages = page_count(args.kitab, path)
        md = kitab(args.kitab, "markdown", str(path))
        txt = kitab(args.kitab, "text", str(path))
        row = {
            "file": path.name,
            "pages": pages,
            "pdf": None,
            "markdown": count(md or " ") - base,
            "text": count(txt or " ") - base,
        }
        data = path.read_bytes()
        if path.suffix.lower() == ".pdf" and len(data) <= MAX_PDF_BYTES and pages <= MAX_PDF_PAGES:
            doc = {
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": "application/pdf",
                    "data": base64.standard_b64encode(data).decode("ascii"),
                },
            }
            row["pdf"] = count([doc, {"type": "text", "text": "x"}]) - base
        rows.append(row)

    print(f"model: {args.model}\n")
    head = f"{'file':34} {'pages':>5} {'PDF read directly':>18} {'kitab markdown':>15} {'kitab text':>11} {'saving':>8}"
    print(head)
    print("-" * len(head))
    tot_pdf = tot_md = 0
    for r in rows:
        if r["pdf"]:
            saving = f"{100 - 100 * r['markdown'] / r['pdf']:.0f} %"
            tot_pdf += r["pdf"]
            tot_md += r["markdown"]
        else:
            saving = "n/a"
        pdf = f"{r['pdf']:,}" if r["pdf"] else "can't read"
        print(f"{r['file'][:34]:34} {r['pages']:>5} {pdf:>18} {r['markdown']:>15,} {r['text']:>11,} {saving:>8}")
    if tot_pdf:
        print(f"\nPDFs together: {tot_pdf:,} tokens read directly, {tot_md:,} as Kitab Markdown "
              f"({100 - 100 * tot_md / tot_pdf:.0f} % fewer, {tot_pdf / max(tot_md, 1):.1f}x).")
    if args.json:
        args.json.write_text(json.dumps({"model": args.model, "rows": rows}, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
