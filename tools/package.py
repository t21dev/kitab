# SPDX-License-Identifier: AGPL-3.0-or-later
"""Lay out Kitab's downloads from a release build (release.yml runs this).

  Windows  dist/Kitab/ (for the Inno Setup installer, packaging/windows/kitab.iss)
           and dist/Kitab_<v>_x64_portable.zip (data kept next to the exe).
  macOS    dist/Kitab.app (Info.plist with the version, icons, ad-hoc signed)
           and dist/Kitab_<v>_<arch>.dmg.
  Linux    dist/Kitab_<v>_x86_64.tar.gz: the binary, desktop entry, MIME
           package, thumbnailer and icons, with install.sh for ~/.local.

Run after tools/build_release.py: python tools/package.py <version>
"""
import pathlib
import platform
import shutil
import subprocess
import sys
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
REL = ROOT / "target" / "release"
DIST = ROOT / "dist"
PKG = ROOT / "packaging"


def fresh(p: pathlib.Path) -> pathlib.Path:
    if p.exists():
        shutil.rmtree(p) if p.is_dir() else p.unlink()
    p.mkdir(parents=True)
    return p


def windows(v: str) -> None:
    app = fresh(DIST / "Kitab")
    for f in ["kitab.exe", "kitab_shell.dll"]:
        shutil.copy2(REL / f, app / f)
    shutil.copy2(ROOT / "LICENSE", app / "LICENSE.txt")
    shutil.copy2(ROOT / "NOTICE", app / "NOTICE.txt")
    # Portable: the same files plus `portable`, which keeps data in data\.
    zip_path = DIST / f"Kitab_{v}_x64_portable.zip"
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for f in sorted(app.iterdir()):
            z.write(f, f"Kitab/{f.name}")
        z.writestr(
            "Kitab/portable",
            "Kitab keeps its data in the data folder next to it while this file is here.\n"
            "Delete this file to use the normal per-user data folder instead.\n",
        )
    print(zip_path)


def macos(v: str) -> None:
    app = DIST / "Kitab.app"
    if app.exists():
        shutil.rmtree(app)
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True)
    res = contents / "Resources"
    res.mkdir()
    shutil.copy2(REL / "kitab", contents / "MacOS" / "kitab")
    plist = (PKG / "macos" / "Info.plist").read_text(encoding="utf8").replace("@VERSION@", v)
    (contents / "Info.plist").write_text(plist, encoding="utf8")
    for icns in (PKG / "macos").glob("*.icns"):
        shutil.copy2(icns, res / icns.name)
    shutil.copy2(ROOT / "LICENSE", res / "LICENSE.txt")
    shutil.copy2(ROOT / "NOTICE", res / "NOTICE.txt")
    # Ad-hoc signature: Apple silicon refuses unsigned code; Gatekeeper still
    # asks the first time (no Developer ID yet).
    subprocess.check_call(["codesign", "--force", "--deep", "--sign", "-", str(app)])
    arch = "aarch64" if platform.machine() == "arm64" else "x86_64"
    dmg = DIST / f"Kitab_{v}_{arch}.dmg"
    stage = fresh(DIST / "dmg")
    shutil.copytree(app, stage / "Kitab.app", symlinks=True)
    (stage / "Applications").symlink_to("/Applications")
    if dmg.exists():
        dmg.unlink()
    subprocess.check_call([
        "hdiutil", "create", "-volname", "Kitab Reader", "-srcfolder", str(stage),
        "-ov", "-format", "UDZO", str(dmg),
    ])
    shutil.rmtree(stage)
    print(dmg)


INSTALL_SH = """#!/bin/sh
# Install Kitab Reader for this user (~/.local). Run ./install.sh again to
# update; ./install.sh --remove takes it away.
set -e
here=$(cd "$(dirname "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
if [ "$1" = "--remove" ]; then
  rm -f "$bin/kitab" "$data/applications/dev.t21.kitab.desktop" \\
        "$data/thumbnailers/dev.t21.kitab.thumbnailer" "$data/mime/packages/dev.t21.kitab.xml"
  find "$data/icons/hicolor" -name 'dev.t21.kitab*.png' -delete 2>/dev/null || true
  echo "Kitab Reader removed. Your library stays in ${XDG_DATA_HOME:-$HOME/.local/share}/kitab."
  exit 0
fi
mkdir -p "$bin" "$data/applications" "$data/thumbnailers" "$data/mime/packages"
install -m 755 "$here/kitab" "$bin/kitab"
sed "s|^Exec=kitab|Exec=$bin/kitab|" "$here/share/applications/dev.t21.kitab.desktop" > "$data/applications/dev.t21.kitab.desktop"
sed "s|kitab --thumbnail|$bin/kitab --thumbnail|; s|^TryExec=kitab|TryExec=$bin/kitab|" \\
    "$here/share/thumbnailers/dev.t21.kitab.thumbnailer" > "$data/thumbnailers/dev.t21.kitab.thumbnailer"
cp "$here/share/mime/packages/dev.t21.kitab.xml" "$data/mime/packages/"
cp -r "$here/share/icons" "$data/"
update-mime-database "$data/mime" 2>/dev/null || true
update-desktop-database "$data/applications" 2>/dev/null || true
gtk-update-icon-cache -q "$data/icons/hicolor" 2>/dev/null || true
echo "Kitab Reader is installed: $bin/kitab, and in your applications menu."
"""


def linux(v: str) -> None:
    name = f"Kitab_{v}_x86_64"
    top = fresh(DIST / name)
    shutil.copy2(REL / "kitab", top / "kitab")
    share = top / "share"
    (share / "applications").mkdir(parents=True)
    (share / "thumbnailers").mkdir()
    (share / "mime" / "packages").mkdir(parents=True)
    shutil.copy2(PKG / "linux" / "dev.t21.kitab.desktop", share / "applications")
    shutil.copy2(PKG / "linux" / "dev.t21.kitab.thumbnailer", share / "thumbnailers")
    shutil.copy2(PKG / "linux" / "dev.t21.kitab.xml", share / "mime" / "packages")
    shutil.copytree(PKG / "linux" / "icons", share / "icons")
    (top / "install.sh").write_text(INSTALL_SH, encoding="utf8")
    (top / "install.sh").chmod(0o755)
    shutil.copy2(ROOT / "LICENSE", top / "LICENSE")
    shutil.copy2(ROOT / "NOTICE", top / "NOTICE")
    tar = DIST / f"{name}.tar.gz"
    subprocess.check_call(["tar", "-czf", str(tar), "-C", str(DIST), name])
    print(tar)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    v = sys.argv[1].lstrip("v")
    DIST.mkdir(exist_ok=True)
    if sys.platform == "win32":
        windows(v)
    elif sys.platform == "darwin":
        macos(v)
    else:
        linux(v)
    return 0


if __name__ == "__main__":
    sys.exit(main())
