# Kitab Reader: plan

> **Read beautifully. Work efficiently.**
> A native, GPU-drawn reader for PDF, EPUB, Markdown and comics. As fast to open as SumatraPDF, with a calm modern interface.

This is the working plan. It records decisions, the architecture, the performance budgets, the test strategy and the phased roadmap. [AGENTS.md](../AGENTS.md) holds the rules for working on the code, and [DESIGN.md](../DESIGN.md) holds the visual system. If the code and this file disagree, change one of them on purpose, in the same commit.

Status: **Phase 0** (workspace, themes and CI in place; Spike A results in [ADR 0001](adr/0001-spike-a-gpui-mupdf.md): MuPDF renders in milliseconds, GPUI's Windows start-up is ~250 ms and needs work before the gate). Mockups: Kitab Reader mockups canvas (private Claude artifact; Round 2 is current).

---

## 1. Decisions

| # | Decision | Why | Revisit if |
|---|---|---|---|
| D1 | **GPUI** (Zed's GPU UI framework, Rust) for the whole UI. No webview, no Electron. | Rendered page bitmaps go straight to the GPU. One language. Sumatra-class start-up is plausible only without a browser engine. | The Phase 0 spike misses the start-up or scroll budgets (§6), or Windows support proves unstable. Spike A missed S1 by about 2×; kept with a start-up plan, see [ADR 0002](adr/0002-keep-gpui-fix-startup.md). |
| D2 | **MuPDF** for PDF, XPS, EPUB, MOBI, FB2, CBZ, SVG and images, through the `mupdf` crate. | Same engine as Sumatra. One engine lays out reflowable books **and** fixed pages, and gives structured text with character boxes for search, selection and highlights in every format. | Never cheaply: the licence follows from it (D3). |
| D3 | **AGPL-3.0-or-later** for all of Kitab. | MuPDF is AGPL. Compatible with GPL-3 code (Zed crates) and permissive crates. | A commercial MuPDF licence is bought. |
| D4 | **Markdown is rendered natively** in GPUI (parse, then GPUI elements), not through MuPDF's HTML engine. | Crisp UI text, live reload, copy buttons on code, collapsible blocks, later editing. MuPDF's CSS support is too limited for GitHub-like rendering. | |
| D5 | **Mermaid is native Rust** (merman, mermaid-rs-renderer or ariel-rs, chosen in Phase 0) → SVG → `resvg` → image, cached by content hash. | No JS runtime and no hidden webview. Deterministic and fast. | Diagram coverage is too thin. Fallback: show the source with "diagram type not supported yet". |
| D6 | **SQLite** (rusqlite, bundled, WAL) for library, reading state, sessions, annotations and FTS5 search. Settings and keymap are **JSON files** the user can edit. | Local-first. Human-editable config, like Zed. | |
| D7 | **v1 includes full sign + verify**, local only. Keys never leave the machine. | Product requirement. | |
| D8 | **Installer under 15 MB** per platform (Windows x64 is the reference). | Sumatra is around 10 MB. | |
| D9 | **Semantic Versioning 2.0.0.** 0.x while phases land, `1.0.0` when the v1 scope (§12) ships. | §11 | |
| D10 | **Shell = "Docked"** (tabs in the title bar, outline docked left) with **Focus mode = "Floating"** (chrome fades, tab pill, floating outline). Default density **Compact**, default icons **Hairline**. | Round 2 mockups. *Defaults until the owner confirms.* | |
| D11 | **Organisation t21** (`github.com/t21dev/kitab`). App identifier `dev.t21.kitab`, e2e build `dev.t21.kitab.e2e`. Landing site on Cloudflare Workers. | Matches Turbo Reader. | |

---

## 2. Kitab vs SumatraPDF

SumatraPDF is the bar for speed and lightness. Kitab matches that and adds a library, reading memory, Markdown, signing and a modern UI.

| | SumatraPDF | Kitab v1 |
|---|---|---|
| Platforms | Windows only | Windows first; macOS and Linux from the same code |
| Language / UI | C++, Win32 + GDI | Rust, GPUI (DirectX 11 / Metal / Vulkan) |
| Engine | MuPDF (+ DjVuLibre, CHM) | MuPDF (DjVu later, CHM not planned) |
| Licence | GPLv3 | AGPL-3.0 |
| Formats | PDF, EPUB, MOBI, FB2, XPS, DjVu, CBZ/CBR, CHM, images | PDF, EPUB, MOBI/AZW3 (non-DRM), FB2, XPS, CBZ (CBR later), images, TXT, **Markdown with Mermaid and math** |
| Tabs | Yes | Yes, plus pin, reopen closed, drag, previews, session restore |
| Library | Start page with recent thumbnails | Home (continue reading, recent as cards or list), library, collections, tags, full-text search |
| Reading memory | Per-file position and zoom | Position, zoom, rotation, view mode and layout per file; whole-workspace restore; reading stats |
| Search | Find in document | Find bar (case, whole word, diacritics), result list, library-wide FTS |
| Annotations | Basic (highlight, note) | Highlights in colours, notes, underline, strike, ink; export to Markdown, CSV and text |
| Digital signatures | No | Verify (chain, revocation, timestamp, changed-after-signing) and sign (.pfx/.p12, Windows store, macOS keychain) |
| PDF tools | No | Merge, split, extract, delete, reorder, rotate, compress, extract text and images, watermark, page numbers, encrypt/decrypt |
| Print | Native Windows | Native on all three, per-page orientation |
| Themes | Light/dark-ish via colours | Dark, Light, Paper, Nord, Noctua, Vampire, plus custom accent |
| Keyboard | Excellent, Sumatra keys | Sumatra-compatible defaults, plus a command palette and a rebindable keymap |
| Portable | Yes | Yes (portable zip, data beside the exe) |
| Start-up | The bar | **≤ Sumatra on the same machine** (§6) |

What not to copy: Sumatra's settings-as-text-file-only UX, its dated chrome, and any feature that costs start-up time.

---

## 3. Architecture

### 3.1 Process and threads

```
kitab.exe (one process per user session; later launches forward argv and exit)
├─ main thread: GPUI event loop, views, input, layout, paint
├─ render pool (N = physical cores − 1, min 1): MuPDF contexts, one per thread
│    jobs: open, layout (reflow), render tile, extract stext, search page, thumbnail
├─ io thread: SQLite (single writer), file watching, thumbnail disk cache
├─ markdown worker: parse, highlight, mermaid → svg → raster (cached)
└─ ipc thread: single-instance pipe / socket listener
```

- Nothing blocks the main thread on disk, the network, MuPDF or SQLite. Every job is cancellable and carries a generation number, so stale results are dropped (scrolling past a page cancels its render).
- MuPDF runs only on pool threads, each with its own `fz_context` (cloned with locks). Panics or MuPDF errors in a job become an error result, never a crash. A document that crashes a worker twice is quarantined with the "Can't open this file" state.
- **Progressive rendering:** first a low-resolution preview of the visible pages (fast path), then sharp tiles at the device scale, then pages ahead and behind. Ultra mode skips the preview pass when the sharp pass is quick anyway, and renders on screen + 1.
- **Tile cache:** keyed by (doc, page, zoom bucket, rotation, theme-page-mode, tile x/y). LRU by bytes, default 512 MB, configurable. GPU textures are a smaller second-level cache. Thumbnails go in a disk cache under the data dir, keyed by file id and mtime.

### 3.2 Workspace layout

```
kitab/
├─ Cargo.toml                 workspace, version = single source of truth (§11)
├─ crates/
│  ├─ kitab/                  bin: entry, CLI args, single instance, start-up path
│  ├─ kitab-ui/               GPUI views: shell, tabs, reader, home, settings, palette, find bar, panels
│  ├─ kitab-theme/            tokens, the six themes, accent and contrast maths, density
│  ├─ kitab-core/             Document trait, locators, text model, search, history, format detection
│  ├─ kitab-mupdf/            safe wrapper over `mupdf`, worker pool, tiles, stext, reflow
│  ├─ kitab-md/               parse (pulldown-cmark, GFM) into a sanitised block tree, frontmatter, slugs
│  ├─ kitab-store/            SQLite schema, migrations, session, library, annotations, FTS5
│  ├─ kitab-sign/             verify + sign (MuPDF pkcs7, Windows CNG, macOS Keychain; PKCS#11 later)
│  ├─ kitab-print/            native print: Windows, macOS, GTK/CUPS
│  ├─ kitab-os/               file associations, default app, jump lists, paths, portable mode, IPC
│  ├─ kitab-tools/            merge, split, rotate, compress, extract, watermark, encrypt
│  ├─ kitab-bench/            performance harness, Sumatra comparison (§6)
│  └─ kitab-e2e/              end-to-end driver and flows (§7)
├─ assets/                    fonts (subset), icons (SVG), themes (JSON), app icons
├─ tests/fixtures/            corpus + expected.json (see MANIFEST.md)
├─ tests/golden/              screenshot goldens per theme
├─ packaging/                 wix/, macos/, linux/ (desktop, metainfo, flatpak), scoop/, winget/, homebrew/
└─ docs/                      PLAN.md, perf/, adr/
```

The landing site is **not** in this tree. It lives on the orphan branch `landing` (§10).

### 3.3 The document model

```rust
pub trait Document: Send + Sync {
    fn format(&self) -> Format;
    fn layout_kind(&self) -> LayoutKind;              // Fixed (PDF, CBZ, fixed EPUB) | Reflow (EPUB, MOBI, FB2, TXT) | Flow (Markdown)
    fn metadata(&self) -> Metadata;                   // title, author, language, direction, page count if known
    fn outline(&self) -> Outline;
    fn page_count(&self, layout: &LayoutParams) -> u32;   // reflow formats depend on width/height/em
    fn page_size(&self, page: u32) -> SizePt;             // fixed formats: the page's own size, never a global one
    fn text(&self, page: u32) -> TextPage;                // glyphs + boxes + logical order, for find/select/highlight
    fn links(&self, page: u32) -> Vec<Link>;
    fn resolve(&self, loc: &Locator, layout: &LayoutParams) -> Position;
}
```

- **Locator** is format-independent and survives re-layout. Fixed: `page + point`. Reflow: `spine item + character offset` (CFI-like). Markdown: `heading slug + block index + character offset`. Highlights store a locator range **and** a text quote with prefix and suffix, so they re-anchor after edits and re-layouts.
- **True page sizes.** Fixed-layout pages render at their own size and orientation at one zoom. "Fit width" fits each page's width independently. Mixed portrait, landscape and receipt pages stack in continuous mode, each a sheet with its own shadow (DESIGN.md § Pages). Fixture: `pdf/02-mixed-page-sizes.pdf`.
- **Page colour modes** for fixed pages: Original, Dark (inverted, with images kept un-inverted by MuPDF image masks), Theme tint (Paper). Per-document override.

### 3.4 Markdown pipeline

`file → normalize (Unicode bullets, CRLF) → frontmatter split (YAML) → pulldown-cmark (GFM: tables, task lists, strikethrough, footnotes, math; autolinks added by Kitab) → sanitize → block tree → GPUI elements`

- **Code:** syntax highlighting with `syntect`, or `tree-sitter-highlight` for the top languages; choose in Phase 5 by size and speed. Copy button, sideways scroll, line wrap toggle.
- **Mermaid:** fenced ` ```mermaid ` → native renderer → SVG → `resvg` → image at device scale, themed to match (dark/light/paper variants). Cache key = hash(source, theme, scale). An invalid diagram shows the source and the error inline (`md/02-mermaid.md` last block).
- **Math:** `$…$`, `$$…$$`, ` ```math `. Candidates: a Rust TeX-to-SVG renderer (ReX-style), or Typst's math layout. Spike in Phase 5; fallback is styled TeX source. Dollar amounts stay text (`md/06-math.md`).
- **HTML:** an allow-list only (`details`, `summary`, `kbd`, `sub`, `sup`, `br`, `img`, `a`, inline formatting). Everything else is dropped. No scripts, no styles, no iframes, no event handlers, no `javascript:` or `data:` links, remote images blocked until allowed per file (`md/09-hostile.md`).
- **Links:** in-page anchors follow GitHub slug rules. Relative `.md` links open in a tab, and other supported files open in Kitab. External links confirm the first time per domain.
- **Live reload:** a file watcher reloads on save and keeps the scroll position (anchor-based), like md-viewer-pwa's watcher.
- Parity checklist: md-viewer-pwa `docs/plans/testing-checklist.md`, rendering sections.

### 3.5 Find (Ctrl+F) and search

- **Find bar** (Ctrl+F, also `/`): incremental and debounced, searching visible pages first and then fanning out in the pool. It shows "3 of 41" and highlights every hit on visible pages. Enter/F3 goes to the next hit, Shift+Enter/Shift+F3 to the previous, Esc closes. Options: match case, whole word, diacritic-insensitive (default on), regular expression (later). A results list in the side panel shows each hit with its context line.
- Text comes from MuPDF structured text for every MuPDF format, and from the block tree for Markdown. Ligatures and hyphenation are normalised for matching.
- **Library search** (Ctrl+K on Home, or "Search all books" in the palette): FTS5 over extracted text, indexed in the background at low priority, paused in Ultra mode and on battery saver.
- Pages with no text layer say so ("This page is an image; no text to search"), as in `pdf/10-scanned-and-images.pdf`. OCR is out of v1.

### 3.6 Data model (SQLite)

```
documents        id, path, file_key (volume+file id | dev+inode), quick_hash (size + first/last 64 KiB),
                 format, title, author, language, pages, cover_ref, size_bytes, added_at,
                 last_opened_at, finished_at, favourite, missing
reading_state    document_id, locator, progress, zoom_mode, zoom, rotation, view_mode,
                 page_mode, layout (w,h,em), sidebar_state, updated_at
sessions         id, created_at, closed_cleanly
session_tabs     session_id, ord, document_id, pinned, active, scroll_state
closed_tabs      id, document_id, closed_at, scroll_state           (reopen with Ctrl+Shift+T)
bookmarks        id, document_id, locator, title, created_at
annotations      id, document_id, kind, colour, anchor (locator range + quote/prefix/suffix + quads),
                 note, created_at, updated_at, written_to_file
collections      id, name, colour, smart_query (nullable)
collection_items collection_id, document_id, ord
tags, document_tags
reading_sessions id, document_id, started_at, ended_at, pages_read, words_read
trusted_certs    id, subject, sha256, der, added_at
doc_text (FTS5)  document_id, chunk_ix, text                         (contentless, external)
schema_version   via rusqlite_migration
```

- Files are recognised after a move or rename by `file_key`, then `quick_hash`. Missing files stay in the library greyed out, with Locate….
- Settings: `settings.json`. Keymap: `keymap.json` (contexts + actions, like Zed). Both sit in the data dir and are watched, so edits apply live. Invalid JSON keeps the last good value and shows a one-line error.
- Data dir: `%APPDATA%\t21\Kitab`, `~/Library/Application Support/dev.t21.kitab`, `$XDG_DATA_HOME/kitab`. Portable mode: a `portable` file beside the exe puts data in `data/` next to it (as in Turbo Reader).
- Crash safety: WAL, a state write at most every 2 s plus on tab switch and quit, `closed_cleanly` to offer "Restore previous session" after a crash.

### 3.7 Signatures

- **Verify** on open (worker, off the UI thread): list signature fields, then for each one byte-range coverage, digest check, signer and chain, expiry, revocation (OCSP/CRL only when the user allows network for verification, off by default with "Check revocation online" in the panel), embedded timestamps, and the modification level after signing (none, form fill, annotations, other). The status bar has states for valid, valid but changed after signing, signer unknown, and broken. Fixtures: `pdf/08a`–`08d`, expected statuses in `expected.json`.
- **Sign:** place by click or drag; appearance drawn, typed or from an image. Keys come from a `.pfx/.p12` file, the Windows certificate store (CNG via `windows` crate as a MuPDF `pdf_pkcs7_signer`), or the macOS keychain. PKCS#11 tokens come later. Always an **incremental** save to a new file by default ("Save signed copy…"), so earlier signatures stay valid (`08b` proves it). Passwords are never stored. The system credential store is used only when the user asks to remember them.
- Viewing, verifying and signing are three distinct actions in the UI. Nothing signs without an explicit confirm step that shows the signer, the field and the output path.

### 3.8 Native print

GPUI has no printing, so `kitab-print` has one backend per OS. On each, MuPDF draws every page at that page's own size onto the printer's device:

| OS | Dialog | Drawing |
|---|---|---|
| Windows | `PrintDlgEx` (the standard dialog) and a Kitab print sheet in front of it | GDI printer DC via `StartDoc/StartPage`; MuPDF renders bands at printer DPI. Fallback "print as image" for broken fonts. |
| macOS | `NSPrintOperation` + print panel | Custom `NSView` drawing each page with MuPDF into a CGContext |
| Linux | `GtkPrintOperation` (portal-aware under Flatpak) | Cairo context; PDF pass-through to CUPS where possible |

The Kitab print sheet covers range (all, current, "11–13"), copies, scaling (fit, actual, shrink oversized), **orientation per page** (auto-rotate paper), annotations on/off and print as image. "System dialog…" goes straight to the OS dialog. Owner-restricted PDFs (`04b`) disable print with an explanation. Shell "print" verb: `kitab --print <file>`.

### 3.9 OS integration and default app

| | Windows | macOS | Linux |
|---|---|---|---|
| File types | ProgIDs `Kitab.PDF`, `Kitab.EPUB`, `Kitab.Markdown`…, `Capabilities\FileAssociations`, `RegisteredApplications` (per-user, or per-machine with the MSI) | `CFBundleDocumentTypes` + UTIs (`com.adobe.pdf`, `org.idpf.epub-container`, `net.daringfireball.markdown`; exported UTI where none exists), `LSHandlerRank = Alternate` | `.desktop` `MimeType=`, shared-mime-info for `text/markdown` |
| Make default | Can't be set silently on Windows 10+. "Make Kitab the default…" opens `ms-settings:defaultapps?registeredAppUser=Kitab` | `NSWorkspace.setDefaultApplication(at:toOpen:)` (macOS 12+), with confirmation | `xdg-mime default kitab.desktop <type>` |
| Open from shell | argv, Open With, Send to, drag onto the exe or dock | Apple Events `openURLs` | argv, D-Bus activation |
| One window | Named mutex + named pipe forwards argv to the running instance, which opens tabs and focuses | Native (one app instance) | Unix socket in `$XDG_RUNTIME_DIR` |
| Thumbnails | `IThumbnailProvider` + `IInitializeWithStream` in `kitab_shell.dll` (crate `kitab-shell`, ~300 KB), registered under Kitab's own ProgIDs (`Kitab.PDF\ShellEx\{e357fccd-…}`) so it never replaces another reader's handler. It holds no document code: it pipes the stream to `kitab.exe --thumbnail --stdin EXT --bgra` (10 s limit), so MuPDF ships once and a hostile file can't crash Explorer. Preview pane (`IPreviewHandler`) later | Quick Look Thumbnail extension (`QLThumbnailProvider` .appex) for EPUB, MD, CBZ, MOBI, FB2 (macOS draws PDF itself); calls a Rust static lib. Quick Look preview extension later | `kitab --thumbnail -s SIZE IN OUT` (exits before any UI) + `/usr/share/thumbnailers/kitab.thumbnailer` (GNOME Files, Nemo, Caja, Thunar via tumbler); Dolphin needs a KIO plugin (check KF6) |
| Extras | Jump list (recent, pinned), taskbar progress for long tools | Dock menu recents | AppStream metainfo |
| Portable | `portable` marker file; no registry writes | n/a | AppImage |

The CLI is `kitab [files…] [--page N] [--find TEXT] [--new-window] [--print] [--bench …]`.

### 3.10 Keyboard

Everything is an **action**, bound in `keymap.json` by context (`Workspace`, `Reader`, `FindBar`, `Home`, `Palette`, `Settings`). Defaults are Sumatra-compatible where Sumatra has a binding (to be checked against Sumatra's published shortcut list in Phase 0; that check is an exit item). Settings › Shortcuts lists, searches and rebinds by key capture, with conflict warnings. Ctrl+/ opens Settings › Shortcuts, the cheat sheet (`?` is left free for typing). Every action is also in the command palette (Ctrl+K / Ctrl+Shift+P) with its binding shown.

| Area | Keys |
|---|---|
| Files and tabs | Ctrl+O open · Ctrl+W / Ctrl+F4 close tab · Ctrl+Shift+T reopen closed · Ctrl+Tab / Ctrl+Shift+Tab and Ctrl+PgDn / Ctrl+PgUp next/prev · Alt+1…8 tab n, Alt+9 last (Ctrl+0/1/2 stay Sumatra's zoom keys) · Ctrl+Shift+PgUp/PgDn move tab · Ctrl+Shift+H Home · Ctrl+N new window · Ctrl+P print · Ctrl+S save annotated copy |
| Navigate | ←/→, PgUp/PgDn, Space/Shift+Space, Home/End · n / p next/prev page · j / k scroll · Ctrl+G go to page · Alt+←/→ back/forward (history) · F12 toggle contents |
| Find | Ctrl+F or / find · F3 / Enter next · Shift+F3 / Shift+Enter previous · Alt+C match case · Alt+W whole word · Esc close |
| Zoom and view | Ctrl+= / Ctrl+− zoom · Ctrl+scroll and pinch · Ctrl+0 fit page · Ctrl+1 actual size · Ctrl+2 fit width · Ctrl+6 single page · Ctrl+7 two-page · Ctrl+8 book view · c continuous · Ctrl+Shift+− / Ctrl+Shift+= rotate left/right |
| Read | F11 minimal (focus) mode · F5 presentation · Ctrl+Shift+L theme cycle · Ctrl+B sidebar · Ctrl+Alt+= / Ctrl+Alt+− text size (reflow) |
| Select | drag to select, double-click word, triple-click line, Shift+click extend · Ctrl+C copy · Esc clear |
| Markdown | Ctrl+E edit beside the preview · Ctrl+S save · Ctrl+Z / Ctrl+Y undo, redo · Esc done |
| Annotate | h highlight selection (1–5 colours) · Ctrl+Alt+N note · Ctrl+D bookmark |
| App | Ctrl+K command palette · Ctrl+, settings · Esc closes menus and dialogs · ? shortcuts · Ctrl+Shift+F12 performance overlay |
| Mouse | Middle-click a tab to close it · right-click a page, tab, Home card or contents entry for its menu · back/forward mouse buttons follow history · Ctrl+wheel zooms · click or drag the scrubber track · click the page number to type a page |

macOS maps Ctrl to Cmd, and Alt+←/→ to Cmd+[ and ].

---

## 4. UI

Shell, components and tokens live in [DESIGN.md](../DESIGN.md). Screens, by mockup board:

- **Reader (Docked):** title bar with Home button + tabs; header with breadcrumb, find, zoom and ⋯ menu (theme and density icon rows, focus mode, export, Settings…); left panel with icon tabs (contents, highlights, bookmarks, pages) and the grouped outline with status rings; page area on the same surface as the chrome; floating scrubber; selection popover.
- **Focus mode (Floating):** chrome fades after 1.5 s; tab pill on mouse-to-top; floating outline and ⋯ panels.
- **Pages:** true-size sheets, mixed sizes, page colour modes.
- **Home:** library rail (Home, Library, Favourites, Highlights, Signed, Finished, Collections); Continue reading; Recent as **cards or list**; search titles and text; drop files anywhere; watched folders.
- **Settings modal:**
  - General: startup mode, opening files, one window, per-file memory, default apps.
  - Appearance: six themes plus Follow system, accent presets plus **custom hex** with a contrast check, density, icon style.
  - Reading: fonts, size, width, line height, page colour modes.
  - Performance: Ultra mode, hardware acceleration, smooth scrolling, animations, thumbnails, threads, prefetch, cache memory, performance overlay.
  - Shortcuts, Signing (trusted certs, revocation), Privacy, About.
- **Print sheet**, **Signature panel and status bar**, **Command palette**, **Find bar**, **Placeholders** (no books, no matches, can't open, password, opening, signature needs a look).

**Startup modes** (Settings › General): Restore last session (default) · Open Home · Open the last file only · Empty window (fastest). Files passed on the command line always open, on top of the chosen mode.

---

## 5. Themes and accent

Six built-in themes in v1: **Dark, Light, Paper, Nord, Noctua, Vampire**. Nord uses the published Nord palette (MIT, credited in About and NOTICE). Noctua (deep night blue, owl-eye amber) and Vampire (aubergine black, crimson) are Kitab's own palettes. Themes are JSON in `assets/themes/`, so more can be added later, and user themes can be loaded from the data dir.

The custom accent takes any hex. Kitab keeps the hue and chroma, and adjusts the OKLCH lightness per theme until text-on-surface reaches 4.5:1 and UI marks reach 3:1. The input shows the measured ratio and says when it lightens the colour. Tokens and rules are in DESIGN.md § Colour.

---

## 6. Performance

The promise: **Kitab opens as fast as SumatraPDF or faster, on the same machine, and stays smooth with many tabs and while resizing.** Regressions block merge.

### 6.1 What we measure

`kitab-bench` measures **black-box**, the same way for Kitab and SumatraPDF. It launches the process, polls the window with a screen capture (Windows Graphics Capture / `PrintWindow`), and stops the clock when the page area has stable, non-blank content in two consecutive captures. Kitab also emits internal trace marks (`process_start → window_shown → first_frame → first_page_sharp`) for detail, which is never used for the Sumatra comparison. Every scenario runs 15 times, after 2 warm-up runs, and reports the median, p90 and max.

| ID | Scenario | Metric |
|---|---|---|
| S1 | Warm start, no file (Empty window mode) | process start → first frame |
| S2 | Warm start with `01-text-outline.pdf` | process start → first page sharp |
| S3 | Start with `06-large-20mb.pdf` | → first page sharp; RSS at 5 s |
| S4 | Start with `real/tracemonkey-pldi-09.pdf`, `epub/06-large-300-chapters.epub`, `md/10-large.md` | → first page / first screen |
| S5 | Cold-ish start: file copied to a fresh path, Kitab caches cleared | → first page sharp |
| S6 | Restore session of **20, 30, 50, 100 tabs** (mixed fixtures) | → active tab sharp; total RSS; time until all tabs have metadata |
| S7 | Open 20/30/50/100 files at once (`kitab a.pdf b.pdf …`) | → last tab created; active tab sharp; RSS |
| S8 | Tab switch among 50 tabs (cached and uncached) | input → new page sharp |
| S9 | Scroll `06-large` top to bottom with PgDn held and with a smooth-scroll script | frame time p50/p95/p99, dropped frames, blank-tile time |
| S10 | Zoom 50 % → 400 % in steps; pinch / Ctrl+wheel bursts | frame time; time to sharp |
| S11 | Find `kitab-large-needle-299` in `06-large` | first hit shown; all hits counted |
| S12 | Window sizes **1280×720, 1920×1080, 2560×1440, 3840×2160** at 100/150/200 % scaling | S2 + S9 at each size |
| S13 | **Live resize**: scripted `SetWindowPos` drag from 800×600 to 2560×1440 and back over 2 s, reflow EPUB and PDF | frame time p95/p99; content never blank > 1 frame |
| S14 | Memory: idle with 1 / 20 / 100 tabs after 60 s | private bytes, GPU memory |
| S15 | Ultra mode and hardware acceleration off: S2, S6 (50), S9 | same metrics |

### 6.2 Budgets

Relative budgets are the contract. Absolute ones are guardrails until the Phase 0 baseline is recorded in `docs/perf/baseline.md` (machine, OS build, GPU, Sumatra version).

| Scenario | Budget |
|---|---|
| S1, S2, S3, S5 | **median ≤ SumatraPDF median** on the same machine; absolute guardrail S1 ≤ 150 ms, S2 ≤ 250 ms |
| S6/S7 | Active tab as fast as S2 regardless of tab count (other tabs are lazy); RSS at 100 tabs ≤ SumatraPDF with the same 100 files |
| S8 | cached ≤ 1 frame; uncached ≤ S2 − process start |
| S9, S10, S13 | p95 frame ≤ 16.7 ms on a 60 Hz display (≤ 8.3 ms on 120 Hz); no blank visible tile longer than 100 ms |
| S11 | first hit ≤ 300 ms; UI never stalls |
| S14 | idle, 1 tab ≤ SumatraPDF + 25 %; nothing proportional to library size at idle |

### 6.3 How we hit them

- **Start-up path is sacred** (AGENTS.md rule):
  1. Single-instance check before GPUI init.
  2. Window shown with chrome immediately.
  3. Open only the active tab's document. Other tabs get metadata from SQLite, not from opening files.
  4. MuPDF context created lazily on the first worker job.
  5. Fonts: the UI face is pre-subset; reading fonts load on first use.
  6. SQLite opens with one indexed query for the session.
  7. No network, no update check, no indexing in the first 5 s.
  8. Thumbnails come from the disk cache.
- **Reading fast path:** render the visible page at preview resolution on the first worker job, then sharp tiles. Prefetch ±1 page (±2 normal, 1 in Ultra).
- **Resizing:** reuse existing tiles scaled during a live resize, re-render sharp once the size settles (150 ms). Reflow formats re-layout off-thread, keeping the current locator, and swap in atomically.
- **Many tabs:** inactive tabs hold no MuPDF document after 60 s (configurable). Tiles are evicted by LRU and memory pressure (Windows memory notifications, macOS `didReceiveMemoryWarning`-style dispatch source).
- **Ultra performance mode:** no animations, no shadows or blur, no thumbnail pre-render, no background indexing, preview pass skipped, prefetch 1, smaller cache. One switch, plus individual toggles beneath it.
- **Hardware acceleration toggle:** GPU on by default. Off uses a software path: WARP (Direct3D 11 software rasteriser) on Windows, llvmpipe/lavapipe on Linux; on macOS the toggle is hidden (Metal always). *Depends on GPUI exposing a software or WARP adapter; Phase 0 checks this, and patches GPUI if needed.*
- **Performance overlay** (Ctrl+Shift+F12): frame time graph, render queue depth, cache hit rate, tiles in flight, RSS.

### 6.4 In CI

The `perf` job runs S1, S2, S6(50), S9 and S11 on a fixed Windows runner. It fails on a > 10 % median regression against the last `main` result, stored as an artifact. A full S1–S15 matrix with the Sumatra comparison runs nightly and before every release. Results go into `docs/perf/` and the release notes. Hosted CI runners are noisy, so the release gate is a run on the reference machine.

---

## 7. Testing

### 7.1 Fixtures

`tests/fixtures/` holds the corpus: 17 PDF, 11 EPUB and 10 Markdown synthetic files, plus 6 real-world files, all described in [MANIFEST.md](../tests/fixtures/MANIFEST.md). `expected.json` is the oracle, generated with MuPDF and pyHanko by `tools/verify_fixtures.py`. Tests assert against it rather than restating numbers.

### 7.2 Layers

1. **Unit** (`cargo test`): locators, search normalisation, contrast maths, keymap parsing, Markdown sanitiser (every `09-hostile.md` case), migrations, signature status mapping.
2. **Engine** (`kitab-mupdf`, `kitab-md`): open every fixture, compare page counts, sizes, outline, search counts and signature states with `expected.json`. Fuzz the open path (`cargo fuzz`, MuPDF and pulldown-cmark) in the nightly job.
3. **View** (`#[gpui::test]`, `TestAppContext` / `VisualTestContext`): simulate keystrokes and actions (`ctrl-f`, `f3`, `ctrl-g`), assert view state (current page, hits, focus, panel visibility).
4. **End-to-end** (`kitab-e2e`): the real release-profile binary, built with the `e2e` feature under identifier `dev.t21.kitab.e2e`, with its own data dir, wiped per test file. The binary exposes a local automation channel (named pipe / Unix socket, JSON-RPC: dispatch action, send keys, query state, capture window). The feature does not exist in release builds, and CI checks that. Assert UI state **and** SQLite state, as Turbo Reader does. Native dialogs (open, save, print, Default apps) are tested at the command layer behind them.
5. **Visual**: golden screenshots of key screens in all six themes and three densities at 1280×800 and 1920×1080 @150 %, compared with a perceptual threshold. Windows first.
6. **Performance**: §6.

### 7.3 Flows (e2e)

- **A. Open:** A1 each fixture opens or shows its intended error state · A2 password prompt (`04a`) · A3 restricted (`04b`) · A4 damaged repaired (`05a`, `05b`) · A5 not-a-file (`05c`, `07c`) · A6 drag and drop, Open With, CLI, second-instance forwarding · A7 recent files and missing-file handling
- **B. Navigation:** B1 keys from §3.10 · B2 Ctrl+G · B3 outline click and closed state · B4 internal link → page 26, Alt+← back · B5 thumbnails · B6 true sizes and rotation (`02`) · B7 RTL page progression (`epub/04`)
- **C. Find:** C1 `lantern` counts from MANIFEST (10 / 9 / 1) · C2 F3 / Shift+F3 order · C3 Unicode finds (`07`) · C4 no text layer (`10`) · C5 Markdown and EPUB finds · C6 large file (`06-large`)
- **D. Tabs and session:** D1 open, close, reopen, pin, reorder · D2 restore after restart, exact positions · D3 crash, then restore prompt · D4 startup modes (all four)
- **E. Home and library:** E1 cards ⇄ list remembered · E2 continue reading order · E3 sort and filter · E4 collections · E5 FTS search · E6 watched folder picks up a new file
- **F. Settings:** F1 each theme persists and applies live · F2 custom hex, low-contrast warning, lightening applied · F3 density · F4 icon style · F5 Ultra mode toggles its children · F6 hardware acceleration needs a restart and survives it · F7 keymap rebind and conflict
- **G. Markdown:** G1 kitchen sink parity · G2 every Mermaid type, invalid block · G3 code highlight and copy · G4 frontmatter valid/invalid · G5 math · G6 hostile file runs nothing · G7 links, anchors, live reload
- **H. Annotations:** H1 create each kind · H2 survive re-layout (EPUB resize) · H3 existing PDF annotations listed (`09`) · H4 export Markdown/CSV
- **I. Signatures:** I1 four statuses (`08a`–`08d`) · I2 trust the test root, then trusted · I3 sign the Tenant field with `tenant.p12`, Landlord still valid · I4 wrong password · I5 Windows store signing (manual on CI, automated with a test cert in the user store)
- **J. Print:** J1 range parsing · J2 per-page orientation plan for `02` · J3 restricted file disabled · J4 print to PDF driver output page count
- **K. PDF tools:** merge, split, extract, rotate, compress, extract text and images (`10`: 3 images)
- **L. OS integration:** L1 associations registered · L2 Default apps page opens · L3 jump list entries · L4 portable mode writes nothing outside its folder

---

## 8. Security and privacy

- Every document is untrusted input. The parsers run in worker threads with limits (max page count rendered at once, max image size, decompression ratio caps). Each crash is isolated to its job. Fuzzing runs nightly.
- No document content runs as code: no PDF JavaScript, no EPUB scripts, no Markdown HTML beyond the allow-list.
- **No network by default.** Remote images in Markdown and EPUB, revocation checks, the update check and sync are each off until the user turns them on. A first-run choice offers the update check.
- No telemetry. Crash reports stay local (a `crash/` folder), and are shared only by the user's hand.
- Private keys and certificate passwords never leave the machine and are never written by Kitab. The OS credential store is used only on request.
- Settings and the database live in the user data dir. Annotations can optionally be encrypted at rest (OS-protected key: DPAPI, Keychain, libsecret).
- `PRIVACY.md` and the landing site's privacy page say exactly this.

---

## 9. Build, CI and release

Modelled on Turbo Reader's workflows.

- **`build.yml`** (push, PR): matrix windows-latest, macos-latest, ubuntu-22.04. Steps: fmt (Linux), clippy `-D warnings`, `cargo test`, `cargo deny` (licences compatible with AGPL, advisories, bans), engine tests against fixtures, e2e on Windows, installer size check (fail if over 15 MB), `perf` short job.
- **`release.yml`** (tag `vX.Y.Z`):
  1. Create one draft release with notes from CHANGELOG (as Turbo Reader does).
  2. Build Windows x64 + arm64 (MSI via WiX, NSIS `.exe`, portable zip), macOS arm64 + x64 `.dmg`, Linux x64 `.AppImage`, `.deb`, `.rpm` and `.flatpak`.
  3. Compute `SHA256SUMS.txt` and append it to the notes.
  4. Publish manually after a smoke test.
- **Signing:** unsigned at first, with the same SmartScreen / Gatekeeper note as Turbo Reader. Later: Windows code signing (SignPath OSS or Azure Trusted Signing), Apple Developer ID + notarisation.
- **Distribution (after 1.0):**
  - **Homebrew:** cask `kitab` in `t21dev/homebrew-tap`, bumped by the release workflow.
  - **Scoop:** `t21dev/scoop-bucket` manifest with `autoupdate` and `checkver`.
  - **winget:** manifests to `microsoft/winget-pkgs` via `wingetcreate` from the release job.
  - **Flathub:** `dev.t21.kitab`.
  - **AUR:** later.

---

## 10. Landing site (branch `landing`)

- Lives on the **orphan git branch `landing`** in this repository, with no shared history with `main`. Cloudflare Workers Builds is pointed at that branch, so pushing to `landing` deploys. App code never goes on `landing`, and site code never goes on `main`.
- React + TypeScript + Vite + Tailwind CSS v4, deployed to **Cloudflare Workers** (static assets via `@cloudflare/vite-plugin` and `wrangler.jsonc`, as md-viewer-pwa does). Organisation t21. Domain to confirm (proposal: `kitab.t21.dev`).
- Content that comes from `main` (CHANGELOG, PRIVACY, perf results, screenshots) is copied in by a workflow on release (`docs: sync landing content`), never edited by hand on `landing`.
- Pages:
  - `/`: hero, a real screenshot per theme, feature sections (formats, tabs and memory, find, Markdown and Mermaid, signing, PDF tools, print, keyboard), a performance section with the measured Sumatra comparison from `docs/perf/`, and download buttons that read the latest GitHub release.
  - `/download`, `/privacy` (from PRIVACY.md), `/changelog` (from CHANGELOG.md at build time), `/shortcuts`.
- Rules: no trackers and no third-party scripts. Every number shown is a measured one with its source, and screenshots are real app captures from the e2e harness (`kitab-e2e --capture-landing`), so they never drift from the app. It follows DESIGN.md tokens.
- Built in Phase 0 as a skeleton and filled in Phase 10.

---

## 11. Versioning and changes

- **Semantic Versioning 2.0.0.** The version lives once, in the workspace `Cargo.toml` (`[workspace.package] version`). The app, installers, About and the landing site read it from there.
- **The first release is `0.1.0`** (owner's decision): nothing is released before it, and it ships everything on `main` when it is cut, which by then is more than Phase 1 (formats, Markdown, find and the shell are already in). After it, each release is a minor (`0.2.0`, `0.3.0`, …) and the phase labels in §12 are the order of work, not fixed version numbers. Fixes are patches. Breaking changes to settings, keymap or database are allowed but must migrate automatically.
- From 1.0: MAJOR for removed features or a data migration that can't roll back, MINOR for features, PATCH for fixes. Pre-releases are `1.1.0-beta.1`.
- **Conventional Commits** (`feat:`, `fix:`, `perf:`, `docs:`, `test:`, `chore:`, `refactor:`, `build:`, `ci:`, with `!` for breaking). Every user-visible change adds a line under `## [Unreleased]` in [CHANGELOG.md](../CHANGELOG.md) (Keep a Changelog) in the same commit. A release commit `chore(release): X.Y.Z` moves Unreleased into a version section and tags `vX.Y.Z`.
- Database schema and settings format each carry their own version and migrations, independent of the app version.

---

## 12. Roadmap

Each phase ends with its exit criteria green in CI and a minor release. A phase does not start until the previous one's exit criteria pass, except Phase 0 spikes, which run in parallel.

### Phase 0: Foundations and spikes (not released)

- Repo, workspace, CI (`build.yml`), `cargo deny`, licence headers, fixtures in CI, landing skeleton on the `landing` branch deployed to Cloudflare Workers.
- **Spike A (go/no-go):** GPUI window on Windows, MuPDF renders `01` page 1 into a GPUI image (`RenderImage` / `paint_image`), scroll 40 pages. Measure S1/S2 against SumatraPDF and record `docs/perf/baseline.md`. **Gate:** projected S1/S2 within budget, or a written reason and plan.
- Spike B: tile upload cost and atlas behaviour for 4K pages; software rendering (WARP) feasibility in GPUI.
- Spike C: Mermaid crates on `md/02-mermaid.md` (coverage, size, speed), then pick one (ADR).
- Spike D: Windows CNG signer through MuPDF's pkcs7 hook; verify `08a`–`08d`.
- Spike E: native print path on Windows with `02-mixed-page-sizes.pdf`.
- Exit: ADRs for A–E in `docs/adr/`; Sumatra shortcut list checked against §3.10.

### Phase 1: PDF reader core → `0.1.0`

- Open (dialog, CLI, drag), progressive render, tiles, continuous and single-page, true page sizes, rotation, zoom modes (Ctrl+0/1/2, wheel, pinch), keyboard navigation, status bar, scrubber, per-file position memory, recent files, single instance, basic file associations, error placeholders, page colour modes.
- Exit: flows A1–A6, B1–B2, B6; S1, S2, S3, S9, S12, S13 within budget.

### Phase 2: Shell, Home and Settings → `0.2.0`

- Tabs (pin, reorder, reopen closed, previews), session restore and the four startup modes, Home (continue reading, recent cards/list, library rail), Settings modal (General, Appearance with six themes, custom hex accent, density, icon style; Performance with Ultra mode and hardware acceleration), ⋯ menu, command palette, keymap.json and the Shortcuts page, cheat sheet.
- Exit: D1–D4, E1–E3, F1–F7; S6, S7, S8, S14, S15 within budget; goldens for all themes.

### Phase 3: Find and navigation → `0.3.0`

- Find bar with options and results list, outline panel (grouped status rings), thumbnails, bookmarks, Ctrl+G, links, history back/forward, presentation mode.
- Exit: B3–B5, C1–C4, C6; S11 within budget.

### Phase 4: Books and other formats → `0.4.0`

- EPUB (reflow, typography settings, footnote popovers, RTL, fixed layout), MOBI (non-DRM), FB2, CBZ, XPS, TXT. Layout preserved across resizes. **Not image files** (the owner's decision: Kitab is a document reader).
- Exit: every EPUB fixture behaves as MANIFEST says; B7, C5, H2 (once annotations land); S4 within budget.
- Done: every format opens, renders, searches and shows its contents (engine tests against `expected.json` › `epub` and `other`); books lay out at 400 × 600 pt, 11 pt; non-Latin text falls back to system fonts by script (`kitab-mupdf/src/fonts.rs`: font-kit took 23 s per small EPUB); per-format tab icons, file types and thumbnails.
- Done since: fixed-layout EPUB at its viewport, one page per spine document (`kitab-mupdf/src/epub.rs` reads the OPF; MuPDF's page margins are absorbed by growing the page); right-to-left books swap the arrow keys (`PageLeft`/`PageRight`). Books are laid out to the reading area (one page per screen at fit width) and re-flow 220 ms after a resize; zoom in a book is its text size (`book_text_size`, 8–32 pt, default 12), kept across layouts by chapter and fraction (`reader/book.rs`).
- Still to do: more typography settings (face, line spacing, margins); footnote popovers; Kindle KF8 (.azw3, refused for now: MuPDF reads only its cover); MuPDF's Charis SIL book face (Kitab uses Times to keep the installer small).

### Phase 5: Markdown → `0.5.0`

- GFM rendering, code highlighting, Mermaid, math, frontmatter, sanitiser, links and anchors, live reload, md-viewer-pwa parity checklist.
- Exit: G1–G7; `md/10-large.md` S4 within budget.

### Phase 6: Annotations → `0.6.0`

- Highlights (5 colours), notes, underline, strike, ink, existing PDF annotations, Annotations panel, export (Markdown, CSV, text), optional save into PDF (incremental).
- Done: drawing and markup (pen, highlighter, line, arrow, rectangle, ellipse, eraser, colour picker, widths, undo/redo) on PDFs, comics, XPS and fixed-layout books; marks kept in the library as they are drawn (schema 2, `marks`), and saved into PDFs as standard annotations, incrementally (`kitab-mupdf/src/marks.rs`). A Settings switch hides the tools.
- Done since: highlight, underline and strike-through from a text selection (H, right-click menu); Note and Text tools with a small writer (`text_field.rs`: a reusable field with input methods, clipboard, undo); notes are pins with their text as a tooltip.
- Done since: the Marks pane (sidebar) lists Kitab's marks and the PDF's own annotations (`Engine::annotations`, tested against `annot_kinds_page1`/`annot_notes_page1` of fixture 09) with quotes for text markup, jump and delete, and exports Markdown or CSV (H3, H4).
- Still to do: erasing or editing annotations already in a PDF; text highlights in re-flowing books.
- Exit: H1–H4.

### Phase 7: Native print → `0.7.0`

- Print sheet + native dialogs on Windows, macOS and Linux, per-page orientation, ranges, restrictions, shell print verb.
- Exit: J1–J4 on all three OSes.

### Phase 8: Signatures → `0.8.0`

- Verify (full status model), Signature panel and status bar, trusted certificates, optional online revocation; sign with `.pfx/.p12`, the Windows store and the macOS keychain; drawn, typed and image appearances; incremental save.
- Exit: I1–I5.

### Phase 9: PDF tools, library search, stats → `0.9.0`

> Pulled forward at the owner's request (2026-11-07): save a page as PDF/image/text, extract pages, merge, and split view shipped with the shell work. The rest of this phase is still to do.


- Merge, split, extract, delete, reorder, rotate, compress, extract text and images, watermark, page numbers, encrypt/decrypt, print range. FTS5 library search, collections, smart collections, tags, watched folders, reading stats and streaks.
- Exit: K, E4–E6.

### Phase 10: Polish, accessibility, distribution → `1.0.0`

- Accessibility pass (AccessKit roles for every control, focus order, screen reader labels), onboarding (tabs, library, highlights, signing, shortcuts), full S1–S15 against Sumatra on the reference machine, published in the release notes, code signing, landing site complete, Homebrew/Scoop/winget manifests, Flathub submission.
- Exit: all flows green, all budgets met, size ≤ 15 MB, `1.0.0` tagged.

After 1.0: optional end-to-end encrypted sync, DjVu, CBR, OCR, Explorer thumbnails and Quick Look, PKCS#11 tokens, plugins for themes.

---

## 13. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| GPUI breaking changes (pre-1.0, tied to Zed) | High | Pin a git revision; upgrade on purpose in its own PR with the full e2e and perf run. |
| GPUI on Windows lacks something (IME, software adapter, a11y detail, DPI changes) | Medium | Phase 0 spike lists gaps; patch upstream or vendor a fork with a short diff. |
| Large page textures thrash the GPUI atlas | Medium | Spike B; custom tiled element painting textures directly. |
| Mermaid coverage in native crates | Medium | Spike C; contribute upstream; graceful source fallback. |
| Windows store signing through MuPDF | Medium | Spike D early, as it is the hardest single v1 item. |
| Matching Sumatra start-up | Medium | Budgets from day one, the start-up path rule, perf CI. |
| Installer size with fonts and MuPDF | Medium | Subset fonts, strip unused MuPDF fonts (CJK as optional download), measure every build. |
| AGPL obligations | Low | Source offer in About; licence notices; `cargo deny`. |
