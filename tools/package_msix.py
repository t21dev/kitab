# SPDX-License-Identifier: AGPL-3.0-or-later
"""Build Kitab's MSIX package for the Microsoft Store (release.yml runs this).

  dist/msix/                  the package layout; `Add-AppxPackage -Register
                              dist\\msix\\AppxManifest.xml` installs it as it is
                              (Developer Mode on) to try it without signing.
  dist/Kitab_<v>_<arch>.msix  the file to upload in Partner Center. It is not
                              signed: the Store signs it after certification.

The manifest is packaging/windows/msix/AppxManifest.xml. Its file types come
from FILE_TYPES in crates/kitab-os/src/assoc.rs (the same list Kitab writes to
the registry when it isn't packaged), with each type's icon from
packaging/windows/kitab.rc. The tile and Store icons are drawn from
assets/brand/kitab-1024.png. makepri and makeappx come from the Windows SDK.

Run after tools/build_release.py: python tools/package_msix.py <version>
"""
import pathlib
import platform
import re
import shutil
import subprocess
import sys
from xml.sax.saxutils import escape

from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
REL = ROOT / "target" / "release"
DIST = ROOT / "dist"
WIN = ROOT / "packaging" / "windows"
BRAND = ROOT / "assets" / "brand" / "kitab-1024.png"

# File types that Explorer can print without opening a window (assoc.rs).
NO_PRINT = {"Kitab.Text", "Kitab.Markdown"}


def sdk_tool(name: str) -> pathlib.Path:
    """The newest Windows SDK's x64 copy of `name`."""
    found = shutil.which(name)
    if found:
        return pathlib.Path(found)
    bins = pathlib.Path(r"C:\Program Files (x86)\Windows Kits\10\bin")
    versions = sorted(
        (p for p in bins.glob("10.*") if (p / "x64" / name).is_file()),
        key=lambda p: [int(x) for x in p.name.split(".")],
    )
    if not versions:
        sys.exit(f"{name} not found: install the Windows SDK")
    return versions[-1] / "x64" / name


def msix_version(v: str) -> str:
    """0.3.5 or 0.3.5-dev -> 0.3.5.0 (the Store owns the fourth number)."""
    nums = [int(n) for n in re.findall(r"\d+", v.split("-")[0])[:3]]
    nums += [0] * (3 - len(nums))
    return ".".join(map(str, nums)) + ".0"


def arch() -> str:
    return "arm64" if platform.machine().lower() in ("arm64", "aarch64") else "x64"


def file_types() -> list[dict]:
    """FILE_TYPES from assoc.rs: extensions, ProgID, description, icon id."""
    src = (ROOT / "crates" / "kitab-os" / "src" / "assoc.rs").read_text(encoding="utf8")
    table = src.split("pub const FILE_TYPES", 1)[1].split("];", 1)[0]
    types = []
    for block in re.findall(r"FileType \{(.*?)\}", table, re.S):
        types.append({
            "extensions": re.findall(r'"([^"]+)"', re.search(r"extensions: &\[(.*?)\]", block).group(1)),
            "prog_id": re.search(r'prog_id: "([^"]+)"', block).group(1),
            "description": re.search(r'description: "([^"]+)"', block).group(1),
            "icon_id": int(re.search(r"icon_id: (\d+)", block).group(1)),
        })
    if not types:
        sys.exit("no FILE_TYPES found in assoc.rs")
    return types


def type_icons() -> dict[int, pathlib.Path]:
    """Icon resource id -> .ico, from kitab.rc."""
    rc = (WIN / "kitab.rc").read_text(encoding="utf8")
    return {int(i): WIN / f for i, f in re.findall(r'^(\d+)\s+ICON\s+"([^"]+)"', rc, re.M)}


# ── images ──────────────────────────────────────────────────────────────

def fit(src: Image.Image, w: int, h: int, scale: float) -> Image.Image:
    """`src` centred on a transparent w×h canvas, its side `scale` of the
    shorter edge."""
    side = max(1, round(min(w, h) * scale))
    canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    icon = src.resize((side, side), Image.LANCZOS)
    canvas.alpha_composite(icon, ((w - side) // 2, (h - side) // 2))
    return canvas


def ico_frame(ico: pathlib.Path, size: int) -> Image.Image:
    """The .ico's frame closest to `size` (at least as big), scaled to it."""
    im = Image.open(ico)
    sizes = sorted(im.info.get("sizes", {(256, 256)}))
    pick = next((s for s in sizes if s[0] >= size), sizes[-1])
    im.size = pick
    return im.convert("RGBA").resize((size, size), Image.LANCZOS)


def assets(out: pathlib.Path, types: list[dict]) -> None:
    """The app's tile, list and Store icons at the scales and target sizes
    Windows asks for (resources.pri picks one), and each file type's icon."""
    out.mkdir(parents=True)
    brand = Image.open(BRAND).convert("RGBA")
    for scale in (100, 125, 150, 200, 400):
        f = scale / 100
        fit(brand, round(44 * f), round(44 * f), 1.0).save(out / f"Square44x44Logo.scale-{scale}.png")
        fit(brand, round(150 * f), round(150 * f), 0.66).save(out / f"Square150x150Logo.scale-{scale}.png")
        fit(brand, round(310 * f), round(150 * f), 0.66).save(out / f"Wide310x150Logo.scale-{scale}.png")
        fit(brand, round(50 * f), round(50 * f), 1.0).save(out / f"StoreLogo.scale-{scale}.png")
    # Taskbar, Start's list, Explorer: target sizes. The icon carries its own
    # plate, so the unplated forms are the same picture.
    for size in (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256):
        img = fit(brand, size, size, 1.0)
        img.save(out / f"Square44x44Logo.targetsize-{size}.png")
        img.save(out / f"Square44x44Logo.targetsize-{size}_altform-unplated.png")
        img.save(out / f"Square44x44Logo.targetsize-{size}_altform-lightunplated.png")
    icons = type_icons()
    for t in types:
        name = doc_logo(t)
        for size in (16, 24, 32, 48, 256):
            ico_frame(icons[t["icon_id"]], size).save(out / f"{name}.targetsize-{size}.png")


def doc_logo(t: dict) -> str:
    return "Doc" + t["prog_id"].split(".", 1)[1].capitalize()


# ── manifest ────────────────────────────────────────────────────────────

def associations(types: list[dict]) -> str:
    clsid = "7080945F-C214-4A8A-B62B-FC5E2E3D047E"
    parts = []
    for t in types:
        name = "kitab" + re.sub(r"[^a-z0-9]", "", t["prog_id"].split(".", 1)[1].lower())
        exts = "\n".join(
            f"              <uap:FileType>.{e}</uap:FileType>" for e in t["extensions"]
        )
        verbs = ""
        if t["prog_id"] not in NO_PRINT:
            verbs = (
                "            <uap2:SupportedVerbs>\n"
                '              <uap3:Verb Id="print" Parameters="--print &quot;%1&quot;">Print</uap3:Verb>\n'
                "            </uap2:SupportedVerbs>\n"
            )
        parts.append(
            '        <uap:Extension Category="windows.fileTypeAssociation">\n'
            f'          <uap3:FileTypeAssociation Name="{name}" Parameters="&quot;%1&quot;">\n'
            f"            <uap:DisplayName>{escape(t['description'])}</uap:DisplayName>\n"
            f"            <uap:Logo>Assets\\{doc_logo(t)}.png</uap:Logo>\n"
            "            <uap:SupportedFileTypes>\n"
            f"{exts}\n"
            "            </uap:SupportedFileTypes>\n"
            f"{verbs}"
            f'            <desktop2:ThumbnailHandler Clsid="{clsid}" />\n'
            "          </uap3:FileTypeAssociation>\n"
            "        </uap:Extension>"
        )
    return "\n".join(parts)


def manifest(version: str, types: list[dict]) -> str:
    text = (WIN / "msix" / "AppxManifest.xml").read_text(encoding="utf8")
    text = text.replace("@VERSION@", msix_version(version)).replace("@ARCH@", arch())
    return text.replace("@FILE_TYPES@", associations(types))


# ── package ─────────────────────────────────────────────────────────────

def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    version = sys.argv[1]
    types = file_types()
    layout = DIST / "msix"
    if layout.exists():
        shutil.rmtree(layout)
    layout.mkdir(parents=True)
    for f in ["kitab.exe", "kitab_shell.dll"]:
        shutil.copy2(REL / f, layout / f)
    shutil.copy2(ROOT / "LICENSE", layout / "LICENSE.txt")
    shutil.copy2(ROOT / "NOTICE", layout / "NOTICE.txt")
    assets(layout / "Assets", types)
    (layout / "AppxManifest.xml").write_text(manifest(version, types), encoding="utf8")

    # resources.pri: lets Windows pick each icon's scale and target size.
    work = DIST / "msix-work"
    if work.exists():
        shutil.rmtree(work)
    work.mkdir()
    makepri = sdk_tool("makepri.exe")
    config = work / "priconfig.xml"
    subprocess.check_call([
        str(makepri), "createconfig", "/cf", str(config), "/dq", "en-US", "/pv", "10.0.0", "/o",
    ])
    # One resources.pri: the default config splits scales into resource
    # packages, which only a bundle carries.
    text = config.read_text(encoding="utf8")
    config.write_text(re.sub(r"<packaging>.*?</packaging>", "", text, flags=re.S), encoding="utf8")
    subprocess.check_call([
        str(makepri), "new", "/pr", str(layout), "/cf", str(config),
        "/mn", str(layout / "AppxManifest.xml"), "/of", str(layout / "resources.pri"), "/o",
    ])
    shutil.rmtree(work)

    out = DIST / f"Kitab_{version}_{arch()}.msix"
    subprocess.check_call([
        str(sdk_tool("makeappx.exe")), "pack", "/d", str(layout), "/p", str(out), "/o",
    ])
    print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
