---
name: kitab
description: Use when a task involves reading, searching, converting to Markdown or text, or editing PDF, EPUB, MOBI, FB2, XPS, CBZ or DjVu files - extracting text, finding which page says something, looking at a page, merging, splitting, rotating or deleting pages, compressing, adding or removing passwords, watermarks, page numbers, highlights and notes, or signing and verifying PDF signatures. Uses the `kitab` command from Kitab Reader.
---

# kitab: documents from the command line

`kitab` is the command-line side of Kitab Reader. It reads every format the app opens (PDF,
EPUB, MOBI, FB2, XPS, CBZ, CBR, DjVu) and changes PDFs with the app's own tools.

Why use it instead of reading page images or writing a script:

- **Fewer tokens.** `kitab markdown` gives headings, lists and tables as text: a fraction of the
  tokens of page images, and searchable.
- **Exact page numbers.** `--page-markers`, `search` and `info` give 1-based page numbers you can
  cite and pass back to other commands.
- **Safe.** It never changes or overwrites an input. Results go to `-o`; an existing output is
  only replaced with `--force`.
- **Scriptable.** `--json` on every command prints one JSON object; exit codes are stable.

Check it is installed with `kitab --version`. Full help for any command: `kitab help COMMAND`.

## Top recipes

```sh
# 1. Read a document as Markdown, citing pages
kitab markdown report.pdf --page-markers --no-images

# 2. What is this file? (pages, page size, outline, encryption, signatures)
kitab info report.pdf --json

# 3. Find which page mentions something
kitab search contract.pdf "termination" --json

# 4. Look at a page (charts, scans, layout) - then view the PNG
kitab render report.pdf --page 7 -o /tmp/p7.png

# 5. Plain text of some pages
kitab text report.pdf --pages 5-9

# 6. Keep, delete, rotate or reorder pages
kitab pages keep report.pdf 1-3,7 -o summary.pdf
kitab pages delete scan.pdf 2 -o cleaned.pdf
kitab pages rotate scan.pdf 1- --by 90 -o turned.pdf

# 7. Merge and split
kitab merge cover.pdf body.pdf -o book.pdf
kitab split book.pdf --every 10 -o parts

# 8. Make a PDF smaller for email
kitab compress photos.pdf --quality smallest -o email.pdf

# 9. Highlight a phrase, add a note (coordinates in points from the top-left)
kitab annotate contract.pdf highlight --page 3 --text "termination fee" -o reviewed.pdf
kitab annotate reviewed.pdf note --page 3 --at 500,90 --text "Check this" -o reviewed2.pdf

# 10. Open a password-protected PDF (never put the password on the command line)
KITAB_PW='...' kitab text locked.pdf --password-env KITAB_PW
```

## Commands

Read (any format unless marked PDF):

| Command | Does |
|---|---|
| `info FILE` | format, title, author, pages, page size, outline, encryption, permissions, counts |
| `text FILE [--pages R] [-o OUT]` | text in reading order (stdout without -o) |
| `markdown FILE [--pages R] [--images DIR \| --no-images] [--page-markers] [-o OUT]` | Markdown |
| `search FILE QUERY [--case] [--whole-word] [--limit N]` | hits: page, line, rects |
| `render FILE [--page N] [--width PX \| --dpi D] -o OUT.png` | a page as PNG (default page 1, 1200 px) |
| `annotations list FILE` | PDF: existing annotations with page, index, kind, text, rect |
| `verify FILE [--trust CERT.pem]` | PDF: each signature's signer, status, coverage, time |

Change (PDF; all need `-o`):

| Command | Does |
|---|---|
| `pages keep FILE R` | keep only R, in the order typed |
| `pages delete FILE R` | delete R |
| `pages rotate FILE R --by 90\|180\|270` | turn clockwise |
| `pages move FILE R --before N` | move R before page N (`--before end` = to the end) |
| `pages insert FILE --from OTHER [--from-pages R] --before N` | put in pages of OTHER |
| `merge A B [C...]` | join in order |
| `split FILE (--every N \| --ranges "1-3,4-9")` | `-o DIR`; files NAME-1.pdf, NAME-2.pdf... |
| `compress FILE [--quality balanced\|smallest\|same]` | smaller copy; reports bytes |
| `password set FILE --new-password-env VAR` | lock (AES-256) |
| `password remove FILE --password-env VAR` | unlock |
| `images-to-pdf IMG...` | one page per picture |
| `images FILE -o DIR` | save embedded pictures as PNG |
| `watermark FILE TEXT` | diagonal text on every page |
| `number FILE [--start N]` | page numbers at the bottom |
| `annotate FILE highlight\|underline\|strike --page N --text PHRASE [--all]` | mark a phrase |
| `annotate FILE note --page N --at X,Y --text T` | sticky note |
| `annotate FILE textbox --page N --rect X,Y,W,H --text T [--size PT] [--bold] [--italic] [--underline] [--strike] [--align left\|centre\|right]` | text on the page |
| `annotations remove FILE (--all \| --page N [--index I])` | take annotations out |
| `sign FILE --key K.pfx --key-password-env VAR --page N --rect X0,Y0,X1,Y1 [--reason T] [--location T]` | digital signature |

Meta: `help [COMMAND]`, `--version`, `agent-skill [--install claude|codex|agents|DIR] [--force]`.

Page ranges everywhere: `1-3,7,10-` (pages from 1; `10-` = 10 to the last; `5-3` counts down).

## Output contract

Global flags, anywhere on the line: `--json`, `-o/--out PATH`, `--force`, `-q/--quiet`,
`--password-env VAR`, `--password-stdin`, `--password PW` (visible in process lists; avoid).

With `--json`, stdout is exactly one JSON object:

```json
{"ok":true,"command":"merge","output":"book.pdf","inputs":2,"pages":42}
{"ok":false,"command":"merge","error":"b.pdf: no such file","code":"not_found","exit":3}
```

Useful fields: `info` → `pages`, `page_size`, `page_sizes_differ`, `outline[{title,page}]`,
`needs_password`, `annotations`, `signatures`; `search` → `hits[{page,text,rects}]`,
`pages_without_text`; `text`/`markdown` without `-o` → `text`/`markdown`; `verify` →
`status` and `signatures[{signer,status,intact,covers_whole_document,changed_after,time}]`.

Without `--json`, success prints one short sentence ("Merged 2 PDFs (42 pages) into book.pdf");
errors go to stderr as `kitab: ...`.

Exit codes:

| Code | Meaning | What to do |
|---|---|---|
| 0 | ok | |
| 1 | operation failed (damaged file, page out of range, phrase not found) | read the error |
| 2 | usage error (unknown command or flag, missing argument) | check `kitab help CMD` |
| 3 | input not found or unreadable | check the path |
| 4 | needs a password, or it is wrong | pass `--password-env VAR` |
| 5 | output exists | choose another `-o` or add `--force` |

## Coordinates

Annotations, `search` rects, `annotations list` rects and `sign --rect` use **points** (1/72 inch)
from the page's **top-left** corner, x to the right, **y downwards**, as the page is shown (its
rotation applied). `info` gives page sizes (US Letter is 612 x 792, A4 595 x 842).

- `annotate ... textbox --rect X,Y,W,H` and rects in JSON: corner plus width and height.
- `sign --rect X0,Y0,X1,Y1`: two corners.
- To place a note beside a phrase: `search --json`, take a hit's `rects[0]`, use its x + w and y.

## Passwords

Never put a password in the command line. Use `--password-env VAR` (set the variable in the same
command: `KITAB_PW=... kitab ...`) or `--password-stdin` (first line of stdin). For
`password set`, the new one comes from `--new-password-env` or `--new-password-stdin`; with both
stdin flags, line 1 is the current password and line 2 the new one. `sign` reads the key's
password from `--key-password-env` or `--key-password-stdin`. `info` on a locked file still
succeeds and reports `"needs_password": true`.

## Pitfalls

- **Scanned PDFs have no text.** `text`/`markdown` come back empty and `search` reports
  `pages_without_text`. Use `kitab render FILE --page N -o p.png` and look at the picture.
- **Books reflow.** EPUB, MOBI and FB2 page numbers are for a 400 x 600 pt layout; they differ
  from the app's window. Cite headings, not page numbers, for books.
- **Write commands are PDF only.** Convert other formats by reading them (`markdown`, `text`).
- **Outputs are never overwritten silently.** Exit 5 means the file exists: pick a new name or
  add `--force`. An input can't be the output even with `--force`.
- **Markdown pictures.** On stdout pictures are left out unless `--images DIR`; with `-o x.md`
  they go to `x_images/`. Pass `--no-images` when you only need the text.
- **Annotations keep signatures.** `annotate` and `annotations remove` append an incremental
  update, so earlier signatures stay intact but `verify` will report `changed-after-signing`.
- **Highlight needs the exact phrase on that page.** Case and accents are ignored; line breaks
  are not. If it is not found (exit 1), `search` first to get the right page and wording.
- **One JSON object.** Don't mix `--json` with reading stdout as text; the content is inside the
  object (`text`, `markdown`).
