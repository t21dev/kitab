# SPDX-License-Identifier: AGPL-3.0-or-later
"""Lay out Kitab's downloads from a release build (release.yml runs this).

  Windows  dist/Kitab/ (for the Inno Setup installer, packaging/windows/kitab.iss)
           and dist/Kitab_<v>_x64_portable.zip (data kept next to the exe).
  macOS    dist/Kitab.app (Info.plist with the version, icons, ad-hoc signed)
           and dist/Kitab_<v>_<arch>.dmg.
  Linux    dist/Kitab_<v>_<arch>.tar.gz: the binary, desktop entry, MIME
           package, thumbnailer and icons, with install.sh for ~/.local;
           and the same files as Kitab_<v>_<arch>.deb (Debian, Ubuntu),
           .rpm (Fedora, openSUSE) and .AppImage (any distribution).
           The .deb's dependencies come from the libraries the binary loads
           (dpkg -S); rpmbuild finds the .rpm's itself. The .rpm needs
           rpmbuild, the AppImage appimagetool (APPIMAGETOOL=path); each is
           skipped with a note when its tool is missing.

Run after tools/build_release.py: python tools/package.py <version>
"""
import pathlib
import os
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


def win_arch() -> str:
    m = platform.machine().lower()
    return "arm64" if m in ("arm64", "aarch64") else "x64"


def windows(v: str) -> None:
    app = fresh(DIST / "Kitab")
    for f in ["kitab.exe", "kitab_shell.dll"]:
        shutil.copy2(REL / f, app / f)
    shutil.copy2(ROOT / "LICENSE", app / "LICENSE.txt")
    shutil.copy2(ROOT / "NOTICE", app / "NOTICE.txt")
    # Portable: the same files plus `portable`, which keeps data in data\.
    zip_path = DIST / f"Kitab_{v}_{win_arch()}_portable.zip"
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


def linux_arch() -> str:
    return "aarch64" if platform.machine().lower() in ("aarch64", "arm64") else "x86_64"


def linux(v: str) -> None:
    name = f"Kitab_{v}_{linux_arch()}"
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
    root = system_tree()
    deb(v, root)
    rpm(v, root)
    appimage(v, root)


def system_tree() -> pathlib.Path:
    """The files as a distribution installs them, under dist/root/usr."""
    root = fresh(DIST / "root")
    usr = root / "usr"
    (usr / "bin").mkdir(parents=True)
    shutil.copy2(REL / "kitab", usr / "bin" / "kitab")
    share = usr / "share"
    for sub, f in [("applications", "dev.t21.kitab.desktop"),
                   ("thumbnailers", "dev.t21.kitab.thumbnailer"),
                   ("mime/packages", "dev.t21.kitab.xml"),
                   ("metainfo", "dev.t21.kitab.metainfo.xml")]:
        (share / sub).mkdir(parents=True, exist_ok=True)
        shutil.copy2(PKG / "linux" / f, share / sub / f)
    shutil.copytree(PKG / "linux" / "icons", share / "icons")
    doc = share / "doc" / "kitab"
    doc.mkdir(parents=True)
    shutil.copy2(ROOT / "LICENSE", doc / "copyright")
    shutil.copy2(ROOT / "NOTICE", doc / "NOTICE")
    return root


def deb_depends(binary: pathlib.Path) -> str:
    """The Debian packages that hold the libraries `binary` loads."""
    out = subprocess.run(["objdump", "-p", str(binary)], capture_output=True, text=True).stdout
    needed = [line.split()[1] for line in out.splitlines() if line.strip().startswith("NEEDED")]
    known = subprocess.run(["ldconfig", "-p"], capture_output=True, text=True).stdout
    pkgs = set()
    # ldconfig -p: "	libfoo.so.1 (libc6,x86-64) => /lib/x86_64-linux-gnu/libfoo.so.1"
    where = {}
    for line in known.splitlines():
        left, _, path = line.strip().partition(" => ")
        if path and ("x86-64" in left or "AArch64" in left):
            where.setdefault(left.split()[0], path.strip())
    for lib in needed:
        for path in [where[lib]] if lib in where else []:
            real = str(pathlib.Path(path).resolve())
            for cand in (path, real):
                r = subprocess.run(["dpkg", "-S", cand], capture_output=True, text=True)
                if r.returncode == 0:
                    pkgs.add(r.stdout.split(":")[0].strip())
                    break
    return ", ".join(sorted(pkgs))


def deb(v: str, root: pathlib.Path) -> None:
    if not shutil.which("dpkg-deb"):
        print("no dpkg-deb: .deb skipped")
        return
    arch = "arm64" if linux_arch() == "aarch64" else "amd64"
    stage = fresh(DIST / "deb")
    shutil.copytree(root / "usr", stage / "usr")
    (stage / "DEBIAN").mkdir()
    size = sum(f.stat().st_size for f in stage.rglob("*") if f.is_file()) // 1024
    depends = deb_depends(REL / "kitab") if shutil.which("dpkg") else ""
    (stage / "DEBIAN" / "control").write_text(
        "Package: kitab\n"
        f"Version: {v}\n"
        f"Architecture: {arch}\n"
        "Maintainer: t21 dev <hello@t21.dev>\n"
        f"Installed-Size: {size}\n"
        + (f"Depends: {depends}\n" if depends else "")
        # Read aloud speaks through speech-dispatcher, loaded when it starts.
        + "Recommends: speech-dispatcher\n"
        + "Section: text\nPriority: optional\n"
        "Homepage: https://github.com/t21dev/kitab\n"
        "Description: Fast, native reader for PDF, ebooks, comics and Markdown\n"
        " Kitab Reader opens PDF, EPUB, MOBI, FB2, CBZ, XPS, Markdown and text.\n"
        " Mark up, fill forms, sign and print, with a library that remembers\n"
        " where you stopped. No telemetry.\n",
        encoding="utf8",
    )
    out = DIST / f"Kitab_{v}_{arch}.deb"
    subprocess.check_call(["dpkg-deb", "--root-owner-group", "--build", "-Zxz", str(stage), str(out)])
    shutil.rmtree(stage)
    print(out)


RPM_SPEC = """Name: kitab
Version: {version}
Release: 1
Summary: Fast, native reader for PDF, ebooks, comics and Markdown
License: AGPL-3.0-or-later
URL: https://github.com/t21dev/kitab
BuildArch: {arch}
Recommends: speech-dispatcher

%description
Kitab Reader opens PDF, EPUB, MOBI, FB2, CBZ, XPS, Markdown and text.
Mark up, fill forms, sign and print, with a library that remembers where
you stopped. No telemetry.

%install
cp -a {root}/usr %{{buildroot}}/

%files
/usr/bin/kitab
/usr/share/applications/dev.t21.kitab.desktop
/usr/share/thumbnailers/dev.t21.kitab.thumbnailer
/usr/share/mime/packages/dev.t21.kitab.xml
/usr/share/metainfo/dev.t21.kitab.metainfo.xml
/usr/share/icons/hicolor/*/apps/dev.t21.kitab*
/usr/share/icons/hicolor/*/mimetypes/dev.t21.kitab*
%doc /usr/share/doc/kitab
"""


def rpm(v: str, root: pathlib.Path) -> None:
    if not shutil.which("rpmbuild"):
        print("no rpmbuild: .rpm skipped")
        return
    arch = linux_arch()
    top = fresh(DIST / "rpmbuild")
    spec = top / "kitab.spec"
    spec.write_text(RPM_SPEC.format(version=v.replace("-", "~"), arch=arch, root=root), encoding="utf8")
    subprocess.check_call([
        "rpmbuild", "-bb", "--define", f"_topdir {top}",
        "--define", "_build_id_links none", str(spec),
    ])
    built = next((top / "RPMS" / arch).glob("*.rpm"))
    out = DIST / f"Kitab_{v}_{arch}.rpm"
    shutil.copy2(built, out)
    shutil.rmtree(top)
    print(out)


APPRUN = """#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/kitab" "$@"
"""


def appimage(v: str, root: pathlib.Path) -> None:
    tool = os.environ.get("APPIMAGETOOL") or shutil.which("appimagetool")
    if not tool:
        print("no appimagetool: .AppImage skipped")
        return
    arch = linux_arch()
    app = fresh(DIST / "Kitab.AppDir")
    shutil.copytree(root / "usr", app / "usr")
    (app / "AppRun").write_text(APPRUN, encoding="utf8")
    (app / "AppRun").chmod(0o755)
    shutil.copy2(PKG / "linux" / "dev.t21.kitab.desktop", app / "dev.t21.kitab.desktop")
    shutil.copy2(PKG / "linux" / "icons" / "hicolor" / "256x256" / "apps" / "dev.t21.kitab.png",
                 app / "dev.t21.kitab.png")
    out = DIST / f"Kitab_{v}_{arch}.AppImage"
    env = dict(os.environ, ARCH=arch)
    subprocess.check_call([tool, "--appimage-extract-and-run", "--no-appstream", str(app), str(out)], env=env)
    shutil.rmtree(app)
    print(out)


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
