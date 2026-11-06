# SPDX-License-Identifier: AGPL-3.0-or-later
"""Source checks CI runs on every push (AGENTS.md rules 16 and 24).

1. Every Rust and Python source file starts with the SPDX licence line
   (a shebang or encoding line may come first).
2. No colour literals in Rust outside `crates/kitab-theme`: colours come from
   theme tokens only.

Usage: python tools/check_source.py   (exit code 1 on any finding)
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPDX = "SPDX-License-Identifier: AGPL-3.0-or-later"
SKIP_DIRS = {"target", ".git", ".venv", "node_modules", "__pycache__", "real"}
THEME_CRATE = ROOT / "crates" / "kitab-theme"

# Hex colours in string literals, and the colour constructors GPUI and our
# own theme crate offer. Tests in kitab-theme are exempt by path.
COLOUR_PATTERNS = [
    re.compile(r'"#[0-9A-Fa-f]{3}(?:[0-9A-Fa-f]{3})?(?:[0-9A-Fa-f]{2})?"'),
    re.compile(r"\b(?:rgb|rgba|hsla|hsl)\s*\(\s*(?:0x|[0-9])"),
    re.compile(r"\bRgba::rgb\s*\("),
    re.compile(r'"rgba?\('),
]


def source_files() -> list[Path]:
    out = []
    for p in ROOT.rglob("*"):
        if p.suffix not in {".rs", ".py"} or not p.is_file():
            continue
        if any(part in SKIP_DIRS for part in p.relative_to(ROOT).parts):
            continue
        out.append(p)
    return sorted(out)


def has_spdx(path: Path) -> bool:
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()[:3]
    return any(SPDX in line for line in lines)


def colour_literals(path: Path) -> list[str]:
    hits = []
    for n, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
        code = line.split("//", 1)[0]
        if any(p.search(code) for p in COLOUR_PATTERNS):
            hits.append(f"{path.relative_to(ROOT)}:{n}: colour literal outside kitab-theme: {line.strip()}")
    return hits


def main() -> int:
    problems = []
    for path in source_files():
        if not has_spdx(path):
            problems.append(f"{path.relative_to(ROOT)}: missing '{SPDX}' in the first lines")
        if path.suffix == ".rs" and THEME_CRATE not in path.parents:
            problems.extend(colour_literals(path))
    for p in problems:
        print(p)
    if problems:
        print(f"\n{len(problems)} problem(s). See AGENTS.md rules 16 and 24.")
        return 1
    print("source checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
