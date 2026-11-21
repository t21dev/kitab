// SPDX-License-Identifier: AGPL-3.0-or-later

//! The help text. Written so an agent can use any command correctly from
//! the help alone: a usage line, what it does, every flag, examples, and
//! what `--json` returns. Lines stay under 100 characters; no colours.

/// `kitab --help`.
pub const OVERVIEW: &str = "\
kitab: read, search, convert and change documents (PDF, EPUB, MOBI, FB2, XPS, CBZ, DjVu)
from a terminal, a script or an AI agent. Inputs are never changed; results go to -o.

Usage: kitab <command> [arguments] [flags]
       kitab FILE...            open files in Kitab Reader
       kitab help <command>     full help for one command, with examples

Read
  info FILE                     Format, title, pages, page sizes, outline, security, counts
  text FILE                     The text, in reading order (stdout or -o OUT.txt)
  markdown FILE                 Markdown with headings, lists and pictures (stdout or -o OUT.md)
  search FILE QUERY             Find words; each hit's page and line of text
  render FILE -o OUT.png        A page as a PNG picture, to look at it
  annotations list FILE         Annotations already in a PDF: page, kind, text, rect
  verify FILE                   Check a PDF's digital signatures

Pages (PDF)
  pages keep FILE RANGES        Keep only these pages
  pages delete FILE RANGES      Delete these pages
  pages rotate FILE RANGES      Turn pages by 90, 180 or 270 degrees (--by)
  pages move FILE RANGES        Move pages to before page N (--before)
  pages insert FILE             Put in pages from another PDF (--from, --before)

Files (PDF)
  merge A.pdf B.pdf...          Join PDFs into one
  split FILE                    One PDF per N pages or per range (-o DIR)
  compress FILE                 Make a PDF smaller (pictures scaled down)
  password set|remove FILE      Lock a PDF with a password, or take it off
  images-to-pdf IMG...          One page per picture (PNG, JPEG, WebP, GIF, BMP, TIFF)
  images FILE -o DIR            Save the pictures inside a PDF as PNG files
  watermark FILE TEXT           Light diagonal text across every page
  number FILE                   Page numbers at the bottom of every page

Marks (PDF)
  annotate FILE highlight|underline|strike|note|textbox
                                Add an annotation (find a phrase, or place it by coordinates)
  annotations remove FILE       Take annotations out (--all, --page N, --index I)

Signatures (PDF)
  sign FILE --key KEY.pfx       Sign with a certificate (.pfx/.p12)
  verify FILE                   Signer, valid or changed, whole document covered, time

Agent
  agent-skill                   Print the agent skill (SKILL.md); --install claude|codex|agents|DIR
  help [COMMAND]                This overview, or one command's help
  --version                     Print \"kitab <version>\"

Global flags (anywhere on the line)
  --json                One JSON object on stdout, nothing else: {\"ok\":true,\"command\":...}
                        or {\"ok\":false,\"error\":\"...\",\"code\":\"not_found\",\"exit\":3}
  -o, --out PATH        Where to write the result
  --force               Overwrite an existing output (never an input)
  -q, --quiet           Print nothing on success (content you asked for still prints)
  --password-env VAR    Password for an encrypted input, read from environment variable VAR
  --password-stdin      Password for an encrypted input, read from the first line of stdin
  --password PW         Same, given directly (visible to other programs in process lists)
  -h, --help            Help for the command

Pages are numbered from 1. Ranges: \"1-3,7,10-\" (10- means 10 to the last page; 5-3 counts down).

Exit codes
  0  ok
  1  the operation failed (damaged file, page out of range, phrase not found...)
  2  usage error (unknown command or flag, missing argument)
  3  input not found or unreadable
  4  needs a password, or the password is wrong
  5  output exists (add --force to overwrite)

For AI agents
  Use `kitab markdown FILE` or `kitab text FILE` to read a document: far fewer tokens than page
  images, with exact page numbers via --page-markers. Use `kitab search` to find where something
  is, and `kitab render FILE --page N -o p.png` to look at a page (scans have no text). Add --json
  to every command for a machine-readable result and check the exit code. Run
  `kitab agent-skill --install claude` (or codex, agents, or a folder) to install the full skill.
";

/// Help for one command (or a command group), by name.
pub fn for_command(name: &str) -> Option<&'static str> {
    Some(match name {
        "info" => INFO,
        "text" => TEXT,
        "markdown" => MARKDOWN,
        "search" => SEARCH,
        "render" => RENDER,
        "annotations" => ANNOTATIONS,
        "verify" => VERIFY,
        "pages" => PAGES,
        "merge" => MERGE,
        "split" => SPLIT,
        "compress" => COMPRESS,
        "password" => PASSWORD,
        "images-to-pdf" => IMAGES_TO_PDF,
        "images" => IMAGES,
        "watermark" => WATERMARK,
        "number" => NUMBER,
        "annotate" => ANNOTATE,
        "sign" => SIGN,
        "agent-skill" => AGENT_SKILL,
        "help" => HELP,
        _ => return None,
    })
}

const INFO: &str = "\
Usage: kitab info FILE [--password-env VAR] [--json]

Describe a document without changing it: format, title, author, page count, page size (the first
page's, and whether others differ), encryption and permissions, whether it needs a password, page
labels, the outline's top-level headings with their pages, and for PDFs the number of annotations,
signatures and form fields. Works on every format Kitab opens.

An encrypted file opened without its password still succeeds (exit 0) and reports
\"needs_password\": true; pass the password to see the rest. A wrong password is exit 4.

Examples
  kitab info report.pdf
  kitab info book.epub --json
  KITAB_PW=secret kitab info locked.pdf --password-env KITAB_PW --json

--json returns
  {\"ok\":true,\"command\":\"info\",\"file\":\"report.pdf\",\"format\":\"pdf\",\"title\":\"...\",
   \"author\":\"...\",\"pages\":40,\"page_size\":{\"width\":612,\"height\":792,\"unit\":\"pt\"},
   \"page_sizes_differ\":false,\"page_sizes\":[...only when they differ...],
   \"encrypted\":false,\"encryption\":null,\"needs_password\":false,
   \"permissions\":{\"print\":true,\"copy\":true,\"annotate\":true,\"fill_forms\":true,
   \"modify\":true,\"assemble\":true},\"page_labels\":false,\"reflowable\":false,\"repaired\":false,
   \"outline\":[{\"title\":\"Chapter 1\",\"page\":1}],\"annotations\":0,\"signatures\":0,
   \"form_fields\":0}
  Sizes are in points (1/72 inch). Books (EPUB, MOBI, FB2) are laid out at 400 x 600 pt.
";

const TEXT: &str = "\
Usage: kitab text FILE [--pages RANGES] [-o OUT.txt] [--json]

The document's text in reading order, pages separated by a blank line. Without -o it prints to
stdout. Works on every format Kitab opens. Scanned pages have no text: use `kitab render`.

Flags
  --pages RANGES   Only these pages, e.g. 1-3,7 (default: all)
  -o, --out FILE   Write to FILE instead of stdout (refuses to overwrite without --force)

Examples
  kitab text report.pdf
  kitab text report.pdf --pages 5-9 -o chapter.txt
  kitab text book.epub --json

--json returns
  {\"ok\":true,\"command\":\"text\",\"pages\":40,\"text\":\"...\"}   (without -o)
  {\"ok\":true,\"command\":\"text\",\"pages\":40,\"output\":\"chapter.txt\",\"bytes\":1234}   (with -o)
";

const MARKDOWN: &str = "\
Usage: kitab markdown FILE [--pages RANGES] [--images DIR | --no-images] [--page-markers]
                           [-o OUT.md] [--json]

The document as Markdown: headings from font sizes, bold and italic, lists, and pictures. The best
way for an agent to read a document: far fewer tokens than page images. Works on PDF, EPUB, MOBI,
FB2, XPS and CBZ.

Pictures: printing to stdout leaves them out unless --images DIR is given. With -o x.md they are
saved to x_images/ beside it and linked relatively, unless --no-images or --images DIR.

Flags
  --pages RANGES    Only these pages, e.g. 1-3,7 (default: all)
  --images DIR      Save pictures in DIR and link them
  --no-images       Leave pictures out
  --page-markers    Put <!-- page N --> before each page, to cite exact pages
  -o, --out FILE    Write to FILE instead of stdout

Examples
  kitab markdown paper.pdf --page-markers
  kitab markdown paper.pdf -o paper.md            (pictures in paper_images/)
  kitab markdown book.epub --pages 1-20 --no-images -o part1.md
  kitab markdown slides.pdf --json --no-images

--json returns
  {\"ok\":true,\"command\":\"markdown\",\"pages\":12,\"images\":3,\"markdown\":\"...\"}   (without -o)
  {\"ok\":true,\"command\":\"markdown\",\"pages\":12,\"images\":3,\"output\":\"paper.md\",
   \"images_dir\":\"paper_images\"}   (with -o)
";

const SEARCH: &str = "\
Usage: kitab search FILE QUERY [--case] [--whole-word] [--limit N] [--json]

Find QUERY in the document. Prints each hit's page and the line of text it is on. Accents are
ignored (\"cafe\" finds \"cafe\" with an accent); case is ignored unless --case.

Flags
  --case          Match upper and lower case exactly
  --whole-word    Only whole words
  --limit N       Stop after N hits (default 200; 0 = no limit)

Examples
  kitab search contract.pdf \"termination\"
  kitab search report.pdf revenue --whole-word --limit 10 --json
  kitab search book.epub \"Chapter 3\" --case

--json returns
  {\"ok\":true,\"command\":\"search\",\"query\":\"revenue\",\"count\":2,\"truncated\":false,
   \"hits\":[{\"page\":17,\"text\":\"the line containing the hit\",\"rects\":[[x,y,w,h]]}]}
  rects are the hit's boxes in page points from the page's top-left (usable with annotate).
  With no hits, \"pages_without_text\" tells how many pages are pictures only (scans).
";

const RENDER: &str = "\
Usage: kitab render FILE [--page N] [--width PX | --dpi D] -o OUT.png [--json]

Draw one page as a PNG, to look at it (charts, scans, layout). Default: page 1, 1200 px wide.

Flags
  --page N       The page, from 1 (default 1)
  --width PX     Picture width in pixels (default 1200; height follows the page's shape)
  --dpi D        Or a resolution: 72 = one pixel per point, 150 = sharp, 300 = print
  -o, --out FILE The PNG to write (required)

Examples
  kitab render scan.pdf -o page1.png
  kitab render report.pdf --page 7 --width 1600 -o p7.png
  kitab render book.epub --page 3 --dpi 150 -o p3.png --json

--json returns
  {\"ok\":true,\"command\":\"render\",\"page\":7,\"output\":\"p7.png\",\"width\":1600,\"height\":2070}
";

const ANNOTATIONS: &str = "\
Usage: kitab annotations list FILE [--json]
       kitab annotations remove FILE (--all | --page N [--index I]) -o OUT.pdf [--json]

list: the annotations already in a PDF (highlights, notes, ink, text boxes, shapes...), with their
page, index on that page, kind, text and rectangle. Links and form fields are not listed.

remove: write a copy without some of them: --all, every one on --page N, or the one at --index I
on that page (I is the index `annotations list` shows).

Rectangles are [x, y, w, h] in points (1/72 inch) from the page's top-left corner, y downwards.

Examples
  kitab annotations list reviewed.pdf
  kitab annotations list reviewed.pdf --json
  kitab annotations remove reviewed.pdf --all -o clean.pdf
  kitab annotations remove reviewed.pdf --page 3 --index 2 -o fixed.pdf

--json returns
  list:   {\"ok\":true,\"command\":\"annotations list\",\"count\":2,\"annotations\":[{\"page\":1,
           \"index\":1,\"kind\":\"Highlight\",\"text\":null,\"rect\":[72,90,200,14],
           \"color\":\"#FFE600\"}]}
  remove: {\"ok\":true,\"command\":\"annotations remove\",\"removed\":2,\"output\":\"clean.pdf\"}
";

const VERIFY: &str = "\
Usage: kitab verify FILE [--trust CERT.pem]... [--json]

Check every digital signature in a PDF: who signed, whether the signature is intact and valid,
whether the document changed after signing, whether it covers the whole document, and when.
Certificates trusted in Kitab's Settings are used; add more with --trust (PEM or DER).

Status per signature
  valid                  intact, signer trusted, nothing but form filling since
  signer-unknown         intact, but the signer's certificate isn't trusted (or had expired)
  changed-after-signing  intact, but pages, content or annotations changed later
  invalid                the signed bytes or the signature don't check out
A PDF without signatures succeeds with count 0. The exit code is 0 whenever the check ran;
read \"status\" (the worst signature's) to decide.

Examples
  kitab verify contract.pdf
  kitab verify contract.pdf --trust company-root.pem --json

--json returns
  {\"ok\":true,\"command\":\"verify\",\"count\":1,\"status\":\"valid\",\"signatures\":[{
   \"field\":\"Landlord\",\"page\":1,\"signer\":\"Common Name: ...\",\"signer_name\":\"...\",
   \"status\":\"valid\",\"intact\":true,\"signature_valid\":true,\"trusted\":true,
   \"expired\":false,\"covers_whole_document\":true,\"changed_after\":\"none\",
   \"time\":\"2026-01-02 10:00:00 UTC\",\"reason\":null,\"location\":null,\"problem\":null}]}
  changed_after: none | form-filling | annotations | other.
";

const PAGES: &str = "\
Usage: kitab pages keep   FILE RANGES -o OUT.pdf
       kitab pages delete FILE RANGES -o OUT.pdf
       kitab pages rotate FILE RANGES --by 90|180|270 -o OUT.pdf
       kitab pages move   FILE RANGES --before N -o OUT.pdf
       kitab pages insert FILE --from OTHER.pdf [--from-pages RANGES] --before N -o OUT.pdf

Change a PDF's pages; the result goes to OUT, the input is never changed.
  keep     only RANGES, in the order typed (\"3,1-2\" puts page 3 first)
  delete   everything but RANGES
  rotate   turn RANGES clockwise by --by degrees (90, 180 or 270)
  move     move RANGES (kept in their order) to before page N; N = last page + 1, or \"end\",
           puts them at the end
  insert   put pages of OTHER (all, or --from-pages) before page N of FILE (N = \"end\" appends)

RANGES: \"1-3,7,10-\" (from 1; 10- = to the last page). --password-env opens FILE; OTHER uses the
same password when it needs one.

Examples
  kitab pages keep report.pdf 1-3,7 -o summary.pdf
  kitab pages delete scan.pdf 2,5-6 -o cleaned.pdf
  kitab pages rotate scan.pdf 1- --by 90 -o turned.pdf
  kitab pages move deck.pdf 10-12 --before 2 -o reordered.pdf
  kitab pages insert report.pdf --from appendix.pdf --before end -o full.pdf

--json returns
  {\"ok\":true,\"command\":\"pages keep\",\"output\":\"summary.pdf\",\"pages\":4,\"changed\":4}
  pages = pages in the new file; changed = pages kept, deleted, turned, moved or inserted.
";

const MERGE: &str = "\
Usage: kitab merge A.pdf B.pdf [C.pdf...] -o OUT.pdf [--json]

Join PDFs into one, in the order given. --password-env is tried on every input that needs one.

Examples
  kitab merge cover.pdf body.pdf appendix.pdf -o book.pdf
  kitab merge a.pdf b.pdf -o both.pdf --force --json

--json returns
  {\"ok\":true,\"command\":\"merge\",\"output\":\"book.pdf\",\"inputs\":3,\"pages\":42}
";

const SPLIT: &str = "\
Usage: kitab split FILE (--every N | --ranges \"1-3,4-9\") -o DIR [--json]

Split a PDF into several files in DIR (created if missing), named NAME-1.pdf, NAME-2.pdf...
after FILE's name. --every N: one file per N pages. --ranges: one file per comma-separated part
(\"1-3,4-9,10-\" makes three files).

Examples
  kitab split book.pdf --every 10 -o parts
  kitab split scan.pdf --ranges \"1-2,3-7,8-\" -o pieces --json

--json returns
  {\"ok\":true,\"command\":\"split\",\"dir\":\"parts\",\"files\":[{\"output\":\"parts/book-1.pdf\",
   \"pages\":10}]}
";

const COMPRESS: &str = "\
Usage: kitab compress FILE [--quality balanced|smallest|same] -o OUT.pdf [--json]

Write a smaller copy of a PDF. Text, fonts and drawings stay sharp; pictures larger than the pages
need are scaled down and saved as JPEG.
  balanced  pictures at most 150 dpi, JPEG quality 75 (default)
  smallest  pictures at most 96 dpi, JPEG quality 60 (for email)
  same      pictures untouched; only a clean rewrite
If the copy isn't smaller it is still written, and the message (and \"smaller\":false) says so.

Examples
  kitab compress scan.pdf -o scan-small.pdf
  kitab compress photos.pdf --quality smallest -o email.pdf --json

--json returns
  {\"ok\":true,\"command\":\"compress\",\"output\":\"email.pdf\",\"bytes_before\":9123456,
   \"bytes_after\":1234567,\"smaller\":true,\"pictures_shrunk\":12}
";

const PASSWORD: &str = "\
Usage: kitab password set FILE (--new-password-env VAR | --new-password-stdin) -o OUT.pdf
       kitab password remove FILE (--password-env VAR | --password-stdin) -o OUT.pdf

set: write a copy locked with a new password (AES-256; the password opens it and allows
everything). If FILE already has one, give it with --password-env too.
remove: write a copy with no password.
The new password is never taken as a plain argument. With both --password-stdin and
--new-password-stdin, stdin's first line is the current password and the second the new one.
A PDF whose owner restricts it can't have its password changed (exit 1).

Examples
  KITAB_NEW=s3cret kitab password set report.pdf --new-password-env KITAB_NEW -o locked.pdf
  KITAB_PW=s3cret kitab password remove locked.pdf --password-env KITAB_PW -o open.pdf
  echo s3cret | kitab password remove locked.pdf --password-stdin -o open.pdf

--json returns
  {\"ok\":true,\"command\":\"password set\",\"output\":\"locked.pdf\",\"encrypted\":true}
";

const IMAGES_TO_PDF: &str = "\
Usage: kitab images-to-pdf IMG [IMG...] -o OUT.pdf [--json]

Make a PDF with one page per picture, in the order given. Each page has the picture's shape
(at most A4's long side); phone photos are turned upright. PNG, JPEG, WebP, GIF, BMP, TIFF.

Examples
  kitab images-to-pdf scan1.jpg scan2.jpg scan3.jpg -o scans.pdf
  kitab images-to-pdf *.png -o album.pdf --json

--json returns
  {\"ok\":true,\"command\":\"images-to-pdf\",\"output\":\"scans.pdf\",\"pages\":3}
";

const IMAGES: &str = "\
Usage: kitab images FILE -o DIR [--json]

Save every picture inside a PDF as PNG files in DIR (created if missing), named
\"page 3 - 1.png\". DIR must be empty or new, unless --force. No pictures is not an error.

Examples
  kitab images report.pdf -o report-pictures
  kitab images scan.pdf -o out --force --json

--json returns
  {\"ok\":true,\"command\":\"images\",\"dir\":\"out\",\"images\":3,\"files\":[\"out/page 2 - 1.png\"]}
";

const WATERMARK: &str = "\
Usage: kitab watermark FILE TEXT -o OUT.pdf [--json]

Write TEXT lightly across the middle of every page at 45 degrees (Helvetica; characters outside
Latin-1 print as ?).

Examples
  kitab watermark draft.pdf \"DRAFT\" -o draft-marked.pdf
  kitab watermark offer.pdf \"CONFIDENTIAL\" -o offer-marked.pdf --json

--json returns
  {\"ok\":true,\"command\":\"watermark\",\"output\":\"draft-marked.pdf\",\"pages\":12}
";

const NUMBER: &str = "\
Usage: kitab number FILE [--start N] -o OUT.pdf [--json]

Put page numbers at the bottom centre of every page, starting at N (default 1).

Examples
  kitab number thesis.pdf -o numbered.pdf
  kitab number body.pdf --start 5 -o body-numbered.pdf --json

--json returns
  {\"ok\":true,\"command\":\"number\",\"output\":\"numbered.pdf\",\"pages\":80,\"first\":1}
";

const ANNOTATE: &str = "\
Usage: kitab annotate FILE highlight --page N --text PHRASE [--all] -o OUT.pdf
       kitab annotate FILE underline|strike --page N --text PHRASE [--all] -o OUT.pdf
       kitab annotate FILE note --page N --at X,Y --text TEXT -o OUT.pdf
       kitab annotate FILE textbox --page N --rect X,Y,W,H --text TEXT [--size PT] [--bold]
                      [--italic] [--underline] [--strike] [--align left|centre|right] -o OUT.pdf
       (the kind may also come before FILE)

Add a standard PDF annotation that every PDF reader shows. The copy is written incrementally,
so existing digital signatures and history stay as they were.
  highlight, underline, strike  find PHRASE on page N (case and accents ignored) and mark its
                                first occurrence there, or every one with --all; exit 1 if absent
  note                          a sticky note with TEXT pinned at X,Y
  textbox                       TEXT written on the page in the box X,Y,W,H (--size, default 12)

Coordinates are points (1/72 inch) from the page's top-left corner, x to the right, y downwards,
as the page is shown (its rotation applied). `kitab info` gives page sizes; `kitab search --json`
gives the rects of words, to place notes next to them.

Flags
  --page N          The page, from 1 (required)
  --text T          The phrase to find, or the note's or text box's text (required)
  --color #RRGGBB   Colour (default: yellow highlight, red underline/strike, yellow note,
                    black text)
  --opacity F       0.05 to 1 (default 1)

Examples
  kitab annotate contract.pdf highlight --page 3 --text \"termination fee\" -o reviewed.pdf
  kitab annotate report.pdf note --page 1 --at 500,72 --text \"Check this figure\" -o r.pdf
  kitab annotate form.pdf textbox --page 2 --rect 72,600,250,30 --text \"Approved\" --size 14 \\
      --bold --color #C00000 -o approved.pdf

--json returns
  {\"ok\":true,\"command\":\"annotate\",\"kind\":\"highlight\",\"page\":3,\"output\":\"reviewed.pdf\",
   \"added\":1,\"rects\":[[x,y,w,h]]}
";

const SIGN: &str = "\
Usage: kitab sign FILE --key KEY.pfx (--key-password-env VAR | --key-password-stdin)
                  --page N --rect X0,Y0,X1,Y1 [--reason T] [--location T] -o OUT.pdf [--json]

Digitally sign a PDF with the certificate and private key in a .pfx/.p12 file. A visible box is
drawn at --rect on page N (\"Digitally signed by ...\", the time, the reason). The signature is
appended as an incremental update, so earlier signatures stay valid. A PDF with a password can't
be signed yet: remove the password first.

--rect gives two corners, X0,Y0 (top-left) and X1,Y1 (bottom-right), in points from the page's
top-left corner, y downwards. The key's password is never taken as a plain argument.

Examples
  KEY_PW=... kitab sign lease.pdf --key me.pfx --key-password-env KEY_PW --page 1 \\
      --rect 72,700,272,760 --reason \"I agree\" -o lease-signed.pdf
  kitab verify lease-signed.pdf

--json returns
  {\"ok\":true,\"command\":\"sign\",\"output\":\"lease-signed.pdf\",\"field\":\"Signature1\",
   \"signer\":\"...\",\"page\":1}
";

const AGENT_SKILL: &str = "\
Usage: kitab agent-skill [--install claude|codex|agents|DIR] [--force] [--json]

Print Kitab's agent skill (SKILL.md, the Agent Skills format), or install it:
  claude   ~/.claude/skills/kitab/SKILL.md   (Claude Code)
  codex    ~/.codex/skills/kitab/SKILL.md    (Codex)
  agents   ~/.agents/skills/kitab/SKILL.md   (other agents that read ~/.agents/skills)
  DIR      DIR/kitab/SKILL.md                (e.g. a project's .claude/skills)
An existing, different SKILL.md is only replaced with --force.

Examples
  kitab agent-skill
  kitab agent-skill --install claude
  kitab agent-skill --install .claude/skills --force --json

--json returns
  {\"ok\":true,\"command\":\"agent-skill\",\"installed\":\"C:/Users/me/.claude/skills/kitab/SKILL.md\",
   \"changed\":true}   (without --install: {\"ok\":true,\"command\":\"agent-skill\",\"skill\":\"...\"})
";

const HELP: &str = "\
Usage: kitab help [COMMAND]

The overview of every command, or one command's full help. `kitab COMMAND --help` is the same.

Examples
  kitab help
  kitab help pages
  kitab merge --help
";
