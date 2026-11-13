# Test fixtures

Every file Kitab's tests open. The machine-readable expectations are in
[`expected.json`](expected.json), written by `tools/verify_fixtures.py` with
MuPDF (the engine Kitab uses) and pyHanko. If the app and `expected.json`
disagree, the app is wrong, or the fixture changed on purpose and
`expected.json` is regenerated in the same commit.

```powershell
python -m venv tests/fixtures/tools/.venv
tests/fixtures/tools/.venv/Scripts/pip install -r tests/fixtures/tools/requirements.txt
tests/fixtures/tools/.venv/Scripts/python tests/fixtures/tools/generate_fixtures.py --large
tests/fixtures/tools/.venv/Scripts/python tests/fixtures/tools/verify_fixtures.py
tests/fixtures/tools/.venv/Scripts/python tests/fixtures/tools/fetch_real_world.py
```

`pdf/06-large-20mb.pdf` and `real/` are gitignored. CI rebuilds and fetches them.

## PDF (`pdf/`)

| File | Covers | Expect |
|---|---|---|
| `01-text-outline.pdf` | 40 pages, 2-level outline (8 chapters, 16 sections), internal + external link on page 1, metadata, one needle per page | Outline shows 8 chapters with closed state from chapter 3. Search: `lantern` substring, case-insensitive = **10**; whole word, case-insensitive = **9**; `Lantern` case-sensitive = **1**; `kitab-needle-17` = **1**, on page 17. Clicking "go to Chapter 6" lands on page 26. The external link asks before opening the browser. |
| `02-mixed-page-sizes.pdf` | A4, A4 landscape, A5, Letter, 80 × 400 mm receipt, A4 with `/Rotate 90` | Every page keeps its own size at one zoom. Fit width fits each page to the width. The rotated page shows landscape. "TOP LEFT" is top-left on every page. Print rotates paper per page. |
| `03-forms.pdf` | AcroForm: text, prefilled text (`Dhaka`), checkbox (checked), radio group, dropdown, list box | Fields render and show their values. v1 views forms; editing fields is later. |
| `04a-encrypted-password-kitab.pdf` | AES-256, user password `kitab` | Password prompt (the "Password protected" placeholder). A wrong password shakes the field and says so. `kitab` opens it. The password is never stored unless the user asks. |
| `04b-owner-restricted.pdf` | Opens without a password; print and copy denied | Opens. Print and Copy are disabled, with a tooltip explaining why. |
| `05a-damaged-truncated.pdf` | First 60 % of 01 | MuPDF repairs it. Show a quiet "Repaired" notice, render what survives, and don't crash. |
| `05b-damaged-bad-xref.pdf` | Wrong `startxref` | Opens through repair, all 40 pages. |
| `05c-not-a-pdf.pdf` | Text file with a `.pdf` name | "Can't open this file" placeholder, with Show in folder. |
| `06-large-20mb.pdf` | 300 pages, about 22 MB, one unique image per page, `kitab-large-needle-NNN` | Performance budget file (see PLAN.md § Performance). First page visible within the budget, smooth scrolling, search finds `kitab-large-needle-299`. |
| `07-unicode-rtl.pdf` | Bangla, Arabic, Hebrew, Chinese, Japanese, Korean, Latin diacritics (HarfBuzz-shaped) | Correct glyphs. Selecting and copying Arabic gives logical order. Search finds `কিতাব`, `كتاب` and `快速`. |
| `08-lease-unsigned.pdf` | Base lease, uncompressed | Source for the signing fixtures. |
| `08a-signed-valid.pdf` | One signature (Landlord) and an empty Tenant field | Status bar: signed, valid, unchanged. Trusted only when `certs/kitab-test-root-ca.pem` is in the trust list; otherwise "signer unknown". |
| `08b-signed-two-signers.pdf` | Landlord, then Tenant, incremental | Both valid. The Landlord signature reports a form-fill-only change after it. |
| `08c-signed-then-modified.pdf` | Annotation added after signing (incremental) | Signature intact, but "the document changed after it was signed". |
| `08d-signed-tampered.pdf` | One byte inside the signed range changed | Signature broken (red). |
| `certs/` | Test root CA (PEM) and two signer `.p12` files, password `kitab` | For the signing tests and for trying Sign with a `.pfx`. **Test keys only.** |
| `09-annotations.pdf` | Highlight, underline, strikeout, sticky note, free text, ink, rectangle on page 1 | Every annotation shows in the page and in the Annotations panel. Export to Markdown lists them. |
| `10-scanned-and-images.pdf` | Page 1 is an image with no text layer; page 2 has PNG with alpha, JPEG and 1-bit images | Page 1: search finds nothing and says the page has no text. Extract images on page 2 writes 3 files. |

## EPUB (`epub/`)

Page counts in `expected.json` are at a 400 × 600 px layout with 11 pt text. They change with the window size, which is the point.

| File | Covers | Expect |
|---|---|---|
| `01-epub3-basic.epub` | EPUB 3 nav, 5 chapters, cover image, `kitab-epub-needle-N` | Cover in the library, 5 contents entries, reflows on resize, keeps position across resizes. |
| `02-epub2-ncx.epub` | EPUB 2, NCX only, cover via `<meta name="cover">` | Same as 01, from NCX. |
| `03-footnotes-images.epub` | `epub:type` noteref and footnote, PNG figure, SVG image, table | Footnote opens as a popover, not a page jump. Images scale to the column. |
| `04-rtl-arabic.epub` | `page-progression-direction="rtl"`, Arabic | Pages advance right to left. Arrow keys follow reading direction. |
| `05-bangla-cjk.epub` | Bangla conjuncts, Chinese, a vertical Japanese chapter | Correct shaping. Vertical text where MuPDF supports it, readable fallback where not. |
| `06-large-300-chapters.epub` | 300 chapters, about 1,200 pages at the test size | Opens fast, contents list virtualised, search finds `kitab-epub-large-needle-300`. |
| `07a-broken-no-container.epub` | No `META-INF/container.xml` | "Can't open this file". |
| `07b-broken-missing-chapter.epub` | Spine item points at a missing file | Opens. The missing chapter shows an inline error, and the rest read normally. |
| `07c-not-a-zip.epub` | Not a zip | "Can't open this file". |
| `08-fixed-layout.epub` | `rendition:layout pre-paginated`, 600 × 800 viewport | Treated like a PDF: fixed pages, zoom, no reflow. |
| `09-no-toc.epub` | No nav, no NCX | Contents panel shows the spine as "Section 1…4", with a note that the book has no table of contents. |

## Markdown (`md/`)

| File | Covers | Expect |
|---|---|---|
| `01-gfm-kitchen-sink.md` | Every GFM element: headings, emphasis, lists, task lists, nested quotes, tables (aligned, overwide), footnotes, images (present and missing), `<details>`, `<kbd>`, long words | Matches the md-viewer-pwa checklist. The overwide table scrolls in its own box. A missing image shows alt text. |
| `02-mermaid.md` | Flowchart, sequence, class, state, ER, Gantt, pie, mindmap, timeline, git graph, one invalid diagram | All render natively in all themes. The invalid one shows its source and the parser error inline. |
| `03-code-highlighting.md` | 13 languages, no-lang fence, unknown lang, long line, indented code, tilde fence, HTML in a code block | Highlighted, with a copy button. HTML inside code is never rendered. |
| `04-frontmatter.md` | Valid YAML frontmatter | Properties table. Title and author feed the tab and library. |
| `05-frontmatter-invalid.md` | Broken YAML | Shown as a raw code block. The document still renders. |
| `06-math.md` | Inline and display TeX, a `math` fence, dollar amounts that are not math, invalid TeX | Math renders. `$12 … $15` stays text. Invalid TeX shows its source with an error mark. |
| `07-unicode-rtl.md` | Bangla, Arabic RTL block, mixed direction, Hebrew, CJK, emoji ZWJ, combining marks, Unicode bullets | Correct shaping and direction. Unicode bullets become a list, except inside fences (md-viewer-pwa `normalizeMarkdown`). |
| `08-links-and-anchors.md` | Heading anchors, links to other .md files and headings, a PDF, a missing file, external link, relative images | In-page jumps, with Alt+Left back. Cross-file links open a tab. Missing targets show an error. Live reload keeps scroll position. |
| `09-hostile.md` | `<script>`, `onerror`, iframe, `javascript:` and `data:` links, SVG script, `<style>`, remote image, fixed overlay, deep nesting, unclosed fence | Nothing runs, nothing overlays the app, remote images stay blocked until the user allows them, no stack overflow. |
| `10-large.md` | About 1.2 MB, 400 sections, code, tables, Mermaid every 100 sections, `kitab-md-needle-NNN` | Opens without blocking the UI. Outline has 401 entries. Search finds `kitab-md-needle-400`. |
| `assets/sample.png` | Image used by 01 and 08 | |

## Other formats (`other/`)

Built by `generate_fixtures.py --only-other`. Reflowable files are counted at the EPUB layout (400 × 600 pt, 11 pt, Kitab's Times face) in `expected.json` › `other`.

| File | Covers | Expect |
|---|---|---|
| `01-plain.txt` | UTF-8 text, 60 paragraphs, `kitab-txt-needle-1…3`, Bangla, Arabic and Chinese lines | Opens as a book with the text icon; search finds the three needles. |
| `02-book.fb2` | FictionBook 2: title, author, 3 sections with `kitab-fb2-needle-N`, a Bangla section | Title in the tab, 4 contents entries, needles found, Bangla shaped. |
| `03-comic.cbz` | Five 800 × 1200 PNG pages | Five pages, image icon, fit to the window. |
| `04-vector.xps` | Two fixed pages of vector paths: Letter portrait, then landscape (no fonts) | Two pages with their own sizes. |
| `05-not-a-zip.cbz` | Not a zip | "Can't open this file". |
| `06-broken.fb2` | Truncated XML | Opens with what MuPDF can read (one blank page); never a crash. |
| `07-comic.cb7` | 7z (LZMA2, solid): four PNG pages and one WebP page, plus `ComicInfo.xml` | Five pages (the XML isn't one), the WebP page drawn by Kitab. Needs `7z` to rebuild. |
| `08-stored-rar5.cbr` | RAR 5, files stored: three PNG pages and `ComicInfo.xml` | Three pages. |
| `09-stored-rar4.cbr` | RAR 4, files stored: three PNG pages and `ComicInfo.xml` | Three pages. |
| `10-zip-named.cbr` | A ZIP comic named `.cbr` (common in the wild) | Two pages: MuPDF reads it by its content. |
| `11-scan-cyrillic.djvu` | Public-domain 12-page scan (Simonovich, 1905) with a text layer; not generated (see NOTICE) | 12 pages at 129 dpi (426 × 621 pt); "углекислота" found 21 times. |
| `12-not-an-archive.cbr` | A RAR signature and nothing after it | "Can't open this file". |
| `13-not-a-djvu.djvu` | A DjVu header with no pages | "Can't open this file". |
| `14-password.cbr` | libarchive's encrypted RAR 4 (BSD-2-Clause) | Refused: password-protected archives aren't supported. |

Compressed RAR (LZ, PPMd, solid, filters, Unicode names) is covered by libarchive's archives in `rar/` (`rar/README.md`).

## Real world (`real/`, fetched)

| File | Why |
|---|---|
| `meditations-gutenberg-2680.epub`, `alice-gutenberg-11.epub` | Real EPUB 3 from Project Gutenberg (public domain) |
| `tracemonkey-pldi-09.pdf` | Two-column academic paper from the pdf.js test corpus |
| `irs-f1040-2024.pdf` | Real government AcroForm (public domain) |
| `commonmark-spec.md` | Large real Markdown (CC BY-SA 4.0) |
| `mermaid-readme.md` | Real-world Mermaid blocks (MIT) |
| `alice-gutenberg-11.mobi`, `alice-gutenberg-11.azw3` | Project Gutenberg's Kindle editions (public domain). The MOBI reads as a book; the KF8 (.azw3) is refused until KF8 is supported, since MuPDF reads only its cover. |
