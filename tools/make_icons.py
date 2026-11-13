# SPDX-License-Identifier: AGPL-3.0-or-later
"""Kitab's app icon and document icons, for every platform, from one script.

The app icon is concept F ("Ribbon"): a dark bookmark ribbon on an amber tile.
Document icons are a paper page with a folded corner, the amber ribbon, and a
coloured band naming the format (PDF, EPUB, MD, ...). Shapes and text are SVG
drawn by MuPDF (PyMuPDF); the tile's amber gradient is laid in with Pillow,
since MuPDF's SVG reader has no gradients. Text uses MuPDF's built-in Nimbus
Sans Bold (URW, shipped with MuPDF).

Writes (committed, so packaging needs no Python):
  assets/brand/kitab.svg                         master app icon (flat amber)
  packaging/windows/kitab.ico, doc-<ext>.ico    16-256 px; embedded in kitab.exe
  packaging/macos/Kitab.icns, doc-<ext>.icns
  packaging/linux/icons/hicolor/<n>x<n>/apps/dev.t21.kitab.png
  packaging/linux/icons/hicolor/<n>x<n>/mimetypes/dev.t21.kitab-<ext>.png
  packaging/linux/icons/hicolor/scalable/...     the SVGs

Run with the fixtures venv (it has PyMuPDF and Pillow):
  tests\\fixtures\\tools\\.venv\\Scripts\\python tools\\make_icons.py
"""
import io
import pathlib

import pymupdf as fitz
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
PKG = ROOT / "packaging"

AMBER_TOP = (0xF4, 0xC7, 0x66)
AMBER_BOTTOM = (0xD9, 0x92, 0x2A)
AMBER = "#E6AD48"
INK = "#17171A"

# Formats Kitab reads, with the band colour of their document icon.
DOCS = {
    "pdf": ("PDF", "#C8402A"),
    "epub": ("EPUB", "#2E8B57"),
    "md": ("MD", "#3D6FD1"),
    "mobi": ("MOBI", "#8B5A2B"),
    "fb2": ("FB2", "#C2702A"),
    "cbz": ("CBZ", "#7B4FD6"),
    "cbr": ("CBR", "#7B4FD6"),
    "cb7": ("CB7", "#7B4FD6"),
    "djvu": ("DJVU", "#4F6D8F"),
    "xps": ("XPS", "#1F8A99"),
    "txt": ("TXT", "#5F6670"),
}

SIZES = [16, 24, 32, 48, 64, 128, 256, 512, 1024]


def app_svg(flat_fill: bool) -> str:
    tile = f'<rect x="64" y="64" width="896" height="896" rx="216" fill="{AMBER if flat_fill else "#fff"}"/>'
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
{tile}
<rect x="66" y="66" width="892" height="892" rx="214" fill="none" stroke="#FFFFFF" stroke-opacity="0.25" stroke-width="4"/>
<path d="M392 64 H632 V812 L512 700 L392 812 Z" fill="{INK}"/>
<path d="M392 64 H632 V96 H392 Z" fill="#000000" fill-opacity="0.25"/>
</svg>'''


def tile_mask_svg() -> str:
    return '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
<rect x="64" y="64" width="896" height="896" rx="216" fill="#000"/></svg>'''


def doc_svg(label: str, color: str, with_text: bool) -> str:
    # Page 680 x 860 centred; folded corner top right; ribbon top left; band at the foot.
    x0, y0, x1, y1, fold = 172, 82, 852, 942, 190
    page = (f'<path d="M{x0 + 40} {y0} H{x1 - fold} L{x1} {y0 + fold} V{y1 - 40} '
            f'Q{x1} {y1} {x1 - 40} {y1} H{x0 + 40} Q{x0} {y1} {x0} {y1 - 40} V{y0 + 40} '
            f'Q{x0} {y0} {x0 + 40} {y0} Z" fill="#FBFAF7" stroke="#C9C4B8" stroke-width="10"/>')
    corner = (f'<path d="M{x1 - fold} {y0} V{y0 + fold - 30} Q{x1 - fold} {y0 + fold} {x1 - fold + 30} {y0 + fold} '
              f'H{x1} Z" fill="#E4DED2" stroke="#C9C4B8" stroke-width="10" stroke-linejoin="round"/>')
    ribbon = f'<path d="M260 {y0} H352 V350 L306 312 L260 350 Z" fill="{AMBER}"/>'
    lines = "".join(
        f'<rect x="250" y="{y}" width="{w}" height="26" rx="13" fill="#D8D3C8"/>'
        for y, w in [(420, 520), (482, 520), (544, 380)]
    )
    band_y = 640
    band = f'<rect x="{x0}" y="{band_y}" width="{x1 - x0}" height="230" fill="{color}"/>'
    text = ""
    if with_text:
        size = 190 if len(label) <= 3 else 150
        text = (f'<text x="512" y="{band_y + 115 + size * 0.36:.0f}" font-family="Helvetica" font-weight="bold" '
                f'font-size="{size}" text-anchor="middle" fill="#FFFFFF">{label}</text>')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
{page}{band}{corner}{ribbon}{lines if with_text else ""}{text}</svg>'''


def render(svg: str, px: int) -> Image.Image:
    doc = fitz.open(stream=svg.encode(), filetype="svg")
    pix = doc[0].get_pixmap(matrix=fitz.Matrix(px / 1024, px / 1024), alpha=True)
    return Image.open(io.BytesIO(pix.tobytes("png"))).convert("RGBA")


def app_icon(px: int) -> Image.Image:
    # Supersample small sizes for clean edges.
    big = max(px, 256) * 2
    mask = render(tile_mask_svg(), big).split()[3]
    grad = Image.new("RGBA", (big, big))
    for y in range(big):
        t = y / (big - 1)
        c = tuple(round(a + (b - a) * t) for a, b in zip(AMBER_TOP, AMBER_BOTTOM))
        grad.paste((*c, 255), (0, y, big, y + 1))
    tile = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    tile.paste(grad, (0, 0), mask)
    fg = render(app_svg(flat_fill=False).replace('fill="#fff"', 'fill="none"'), big)
    out = Image.alpha_composite(tile, fg)
    return out.resize((px, px), Image.LANCZOS)


def doc_icon(ext: str, px: int) -> Image.Image:
    label, color = DOCS[ext]
    big = max(px, 256) * 2
    # Below 32 px the label can't be read: keep the band, drop the text.
    img = render(doc_svg(label, color, with_text=px >= 32), big)
    return img.resize((px, px), Image.LANCZOS)


def write_set(name: str, make, ico: pathlib.Path, icns: pathlib.Path, linux_dir: str, linux_name: str):
    images = {s: make(s) for s in SIZES}
    ico.parent.mkdir(parents=True, exist_ok=True)
    images[256].save(ico, sizes=[(s, s) for s in (16, 24, 32, 48, 64, 128, 256)],
                     append_images=[images[s] for s in (16, 24, 32, 48, 64, 128)])
    icns.parent.mkdir(parents=True, exist_ok=True)
    images[1024].save(icns, append_images=[images[s] for s in (16, 32, 64, 128, 256, 512)])
    for s in (16, 24, 32, 48, 64, 128, 256, 512):
        d = PKG / "linux" / "icons" / "hicolor" / f"{s}x{s}" / linux_dir
        d.mkdir(parents=True, exist_ok=True)
        images[s].save(d / f"{linux_name}.png")
    print(f"{name}: {ico.relative_to(ROOT)}, {icns.relative_to(ROOT)}")


def main() -> None:
    brand = ROOT / "assets" / "brand"
    brand.mkdir(parents=True, exist_ok=True)
    (brand / "kitab.svg").write_text(app_svg(flat_fill=True), encoding="utf8")
    app_icon(1024).save(brand / "kitab-1024.png")
    scal = PKG / "linux" / "icons" / "hicolor" / "scalable"
    (scal / "apps").mkdir(parents=True, exist_ok=True)
    (scal / "mimetypes").mkdir(parents=True, exist_ok=True)
    (scal / "apps" / "dev.t21.kitab.svg").write_text(app_svg(flat_fill=True), encoding="utf8")
    write_set("app", app_icon, PKG / "windows" / "kitab.ico", PKG / "macos" / "Kitab.icns", "apps", "dev.t21.kitab")
    for ext, (label, color) in DOCS.items():
        (scal / "mimetypes" / f"dev.t21.kitab-{ext}.svg").write_text(doc_svg(label, color, True), encoding="utf8")
        write_set(ext, lambda s, e=ext: doc_icon(e, s),
                  PKG / "windows" / f"doc-{ext}.ico", PKG / "macos" / f"doc-{ext}.icns",
                  "mimetypes", f"dev.t21.kitab-{ext}")


if __name__ == "__main__":
    main()
