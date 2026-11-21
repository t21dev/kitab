#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Check, start and fill in translations in assets/locales/ (GitHub #12).

The text to translate is the English Kitab shows through `tr(...)`: literal
`tr("...")` calls in crates/kitab-ui, the menu bar's titles and rows, the
command palette's labels and groups, and Settings' sections and groups.

    python tools/check_i18n.py                    # how complete each language is
    python tools/check_i18n.py zh-CN              # every text zh-CN is missing
    python tools/check_i18n.py --fill zh-CN       # add the missing texts to zh-CN.json, blank
    python tools/check_i18n.py --new ja "日本語"   # start a language: every text, blank
    python tools/check_i18n.py --strict           # exit 1 on unused keys (CI)

A blank translation shows the English, so a language can be finished a few
texts at a time. Keys starting with "@" describe the file ("@language", the
language's name in itself; "@translators") and are never shown as text.

Keys a catalog has that Kitab no longer shows are listed as "unused": the
English changed, so the translation needs moving to the new text.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "crates" / "kitab-ui" / "src"
LOCALES = ROOT / "assets" / "locales"
# Languages written right to left: --new marks their file "@direction": "rtl".
RTL = {"ar", "ur", "fa", "he", "ps", "sd", "ug", "yi", "dv", "ckb"}


def unescape(s: str) -> str:
    """A Rust string literal's text (the escapes the UI code uses)."""
    return (
        s.replace('\\"', '"')
        .replace("\\n", "\n")
        .replace("\\u{201c}", "“")
        .replace("\\u{201d}", "”")
        .replace("\\u{2019}", "’")
        .replace("\\\\", "\\")
    )


def texts() -> list[str]:
    found: list[str] = []

    def add(t: str) -> None:
        t = unescape(t)
        if t not in found:
            found.append(t)

    lit = r'"((?:[^"\\]|\\.)*)"'
    for path in sorted(UI.rglob("*.rs")):
        src = path.read_text(encoding="utf-8")
        # Text only the tests use isn't shown.
        src = src.split("#[cfg(test)]\nmod tests")[0]
        for m in re.finditer(r"\btrf?\(\s*" + lit, src):
            add(m.group(1))
    menu = (UI / "shell" / "menu_bar.rs").read_text(encoding="utf-8")
    menu = menu[menu.index("const MENUS") :]
    menu = menu[: menu.index("\n];")]
    for m in re.finditer(r"title: " + lit, menu):
        add(m.group(1))
    for m in re.finditer(r"row!\(\s*" + lit, menu):
        add(m.group(1))
    palette = (UI / "palette.rs").read_text(encoding="utf-8")
    for m in re.finditer(r"cmd!\(\s*" + lit + r",\s*" + lit, palette):
        add(m.group(1))
        add(m.group(2))
    settings = (UI / "settings_view.rs").read_text(encoding="utf-8")
    # Settings groups: their titles and notes are shown through tr_any.
    group = r"\bgroup\(\s*" + lit + r",\s*(?:Some\(\s*" + lit + r"\s*\)|None)"
    # Groups can live in settings_view.rs or any file in settings_view/.
    every = settings + "".join(
        p.read_text(encoding="utf-8") for p in sorted((UI / "settings_view").glob("*.rs"))
    )
    for m in re.finditer(group, every):
        add(m.group(1))
        if m.group(2) is not None:
            add(m.group(2))
    english = settings[settings.index("fn english(self)") :]
    english = english[: english.index("\n    }")]
    for m in re.finditer(r"=> " + lit, english):
        add(m.group(1))
    return found


def write(path: Path, catalog: dict) -> None:
    text = json.dumps(catalog, ensure_ascii=False, indent=2) + "\n"
    path.write_text(text, encoding="utf-8", newline="\n")


def ordered(have: dict, want: list[str]) -> dict:
    """The file's details first, then every text in the order Kitab has it
    (blank where untranslated), then translations no longer used."""
    out = {k: v for k, v in have.items() if k.startswith("@")}
    for t in want:
        out[t] = have.get(t, "")
    for k, v in have.items():
        if k not in out:
            out[k] = v
    return out


def main() -> int:
    # Language names and translations print on any console (Windows' is cp1252).
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    argv = sys.argv[1:]
    want = texts()
    if argv[:1] == ["--new"]:
        if len(argv) != 3:
            print('usage: check_i18n.py --new CODE "Name in the language"')
            return 2
        code, name = argv[1], argv[2]
        path = LOCALES / f"{code}.json"
        if path.exists():
            print(f"{path.name} already exists: use --fill {code}")
            return 1
        meta = {"@language": name, "@translators": ""}
        if code.split("-")[0].lower() in RTL:
            meta["@direction"] = "rtl"
        write(path, ordered(meta, want))
        print(f"wrote {path.relative_to(ROOT)}: {len(want)} texts to translate")
        return 0
    if argv[:1] == ["--fill"]:
        for code in argv[1:]:
            path = LOCALES / f"{code}.json"
            have = json.loads(path.read_text(encoding="utf-8"))
            added = [t for t in want if t not in have]
            write(path, ordered(have, want))
            print(f"{path.name}: added {len(added)} blank texts")
        return 0
    args = [a for a in argv if not a.startswith("--")]
    strict = "--strict" in argv
    bad = False
    for cat in sorted(LOCALES.glob("*.json")):
        code = cat.stem
        if args and code not in args:
            continue
        have = json.loads(cat.read_text(encoding="utf-8"))
        have = {k: v for k, v in have.items() if not k.startswith("@")}
        done = [t for t in want if have.get(t, "").strip()]
        missing = [t for t in want if t not in done]
        unused = [k for k in have if k not in want]
        pct = 100 * len(done) // max(len(want), 1)
        name = json.loads(cat.read_text(encoding="utf-8")).get("@language", "?")
        print(f"{code} ({name}): {len(done)}/{len(want)} ({pct} %), {len(missing)} missing, {len(unused)} unused")
        if args:
            for t in missing:
                print(f"  missing: {t!r}")
        for t in unused:
            print(f"  unused:  {t!r}")
        bad |= strict and bool(unused)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
