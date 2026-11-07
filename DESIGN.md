# Kitab Reader design

How Kitab looks and behaves. New UI must follow it. If the code and this file disagree, fix one of them on purpose, in the same change.

Source of truth in code (once it exists): `crates/kitab-theme` (tokens, themes, density, accent maths) and `assets/themes/*.json`. The reference mockups are the "Kitab Reader mockups" canvas, page *Round 2*.

## Principles

- **Reading first, chrome quiet.** The page is the loudest thing on screen. Chrome is greyscale. Colour is for the accent, reading progress, highlights, signature status and destructive actions.
- **One surface.** The window, title bar, sidebar and reading area share the **page colour** of the theme (dark, white or paper). Panes are separated by 1px hairlines, not by different backgrounds. The sidebar may sit one step darker (`panel`). Fixed-layout pages (PDF) are the one exception: they are sheets on a slightly darker `desk`.
- **Depth, quietly.** Raised controls have a 1px top highlight and a two-layer shadow. Inputs, tracks and groups are recessed wells. Floating layers (menus, popovers, the scrubber, dialogs) carry a deeper float shadow. No glass blur, and no gradients except the primary button's 2-step shading.
- **Fewer switches on screen.** No segmented switch strips in the chrome. Theme and density live behind the ⋯ menu as icon rows and in Settings. Panel tabs are icon buttons.
- **Fast is a feature.** Nothing decorative may cost a frame. Every animation can be turned off, and Ultra mode turns off shadows too.
- **Plain copy.** Short sentences in sentence case. Helper text explains consequences ("Your key never leaves this computer"), never sells. No invented numbers anywhere, in the app or on the landing site.

## Colour

All colours are tokens. Components never contain a literal colour. Tokens are defined per theme in `assets/themes/<id>.json` and resolved in `kitab-theme`.

### Surface and text tokens

| Token | Dark | Light | Paper | Nord | Noctua | Vampire | Ink | Use |
|---|---|---|---|---|---|---|---|---|
| `bg` | `#141415` | `#FFFFFF` | `#F4EEE3` | `#2E3440` | `#0E1626` | `#17121A` | `#000000` | Window, chrome **and** reflowed reading surface |
| `panel` | `#111112` | `#FAFAFB` | `#EFE8DB` | `#2A303B` | `#0B1220` | `#130F16` | `#050506` | Sidebar, settings rail, wells |
| `desk` | `#0F0F10` | `#F2F2F4` | `#EBE3D5` | `#292E39` | `#0A101C` | `#120E14` | `#000000` | Behind fixed-layout page sheets |
| `raised` | `#1E1E21` | `#FFFFFF` | `#FAF6EE` | `#3B4252` | `#17223A` | `#231B28` | `#131316` | Raised controls, active tab, menus, cards |
| `hover` | `#18181A` | `#F3F3F5` | `#EDE5D7` | `#343A47` | `#121C30` | `#1D1621` | `#0C0C0E` | Hover fill, well fill |
| `line` | `#27272B` | `#E8E8EC` | `#DCD1BF` | `#434C5E` | `#1F2B45` | `#2F2534` | `#1E1E23` | Every hairline |
| `text` | `#ECECEE` | `#18181B` | `#2B241D` | `#ECEFF4` | `#E6ECF7` | `#F2EAF0` | `#F4F4F6` | Primary text |
| `read` | `#D9D9DD` | `#232326` | `#2B241D` | `#E5E9F0` | `#DCE3F0` | `#EADFE6` | `#E2E2E7` | Reading body text (slightly softer) |
| `muted` | `#A1A1A7` | `#55555D` | `#5F5346` | `#C3CAD6` | `#A9B5CE` | `#C2B2BF` | `#A7A7B0` | Secondary text, idle icons |
| `faint` | `#8C8C92` | `#6B6B73` | `#6E6253` | `#A3ACBC` | `#8A97B3` | `#A2909F` | `#8E8E98` | Meta, counts, helper text. Never the only copy of something important |
| `ring` | `#5C5C63` | `#B5B5BC` | `#B5A893` | `#4C566A` | `#33405E` | `#4A3B50` | `#4A4A53` | Unread ring, disabled glyphs, ticks |
| `accent` | `#E6B450` | `#B7791F` | `#A65424` | `#88C0D0` | `#F2A65A` | `#E0466A` | `#8EA8FF` | The accent (overridable, see below) |
| `highlight` | `rgba(230,180,80,.30)` | `#FBE2A6` | `#EFD08F` | `rgba(235,203,139,.30)` | `rgba(242,166,90,.28)` | `rgba(224,70,106,.28)` | `rgba(142,168,255,.28)` | Default highlight colour behind text |
| `ok` | `#4CC38A` | `#1F9D63` | `#4E8A57` | `#A3BE8C` | `#5FD3A3` | `#6CCB97` | `#4CC38A` | Finished, signature valid |
| `warn` | `#E6B450` | `#B7791F` | `#B87A1E` | `#EBCB8B` | `#F2C14E` | `#E8B04A` | `#E6B450` | Signer unknown, changed after signing |
| `bad` | `#F06A6A` | `#D33B3B` | `#B5402F` | `#BF616A` | `#F07A7A` | `#FF5C7A` | `#F06A6A` | Errors, broken signature, destructive |
| `scrim` | `rgba(0,0,0,.55)` | `rgba(20,20,30,.28)` | `rgba(60,40,20,.25)` | `rgba(0,0,0,.45)` | `rgba(0,0,0,.55)` | `rgba(0,0,0,.55)` | `rgba(0,0,0,.7)` | Behind dialogs |

Nord uses the published Nord palette (MIT; credit in About and NOTICE). Noctua, Vampire and Ink are Kitab's own. Ink is true black for OLED screens, with a blue-black ink accent. A new theme must define every token above and pass the contrast check: `text`/`read` ≥ 7:1 on `bg`, `muted` ≥ 4.5:1, `faint` ≥ 4.5:1 on `bg` and `panel`, `accent` ≥ 3:1 as a mark and ≥ 4.5:1 when used as text.

### Accent

- Presets: theme default, Amber, Rose, Blue, Emerald, Grape, Wine, Mono. **Custom: any hex**, typed or picked from a hue strip.
- Kitab keeps the hue and chroma of a custom colour and adjusts OKLCH lightness per theme until it reaches 3:1 against `bg` for marks and 4.5:1 where it is text. The hex field shows the measured ratio, and says when it lightens the colour.
- The accent marks: active and selected states, progress bars, the focus ring, the scrubber thumb, reading-state rings, toggled icon buttons and switches. Never large fills, and never body text.

### Depth tokens

| Token | Dark-family themes (Dark, Nord, Noctua, Vampire, Ink) | Light | Paper |
|---|---|---|---|
| `elev` (raised control) | `inset 0 1px 0 rgba(255,255,255,.07), 0 0 0 1px rgba(0,0,0,.5), 0 2px 6px rgba(0,0,0,.35)` | `0 0 0 1px rgba(20,20,40,.08), 0 1px 2px rgba(20,20,40,.08), 0 2px 6px rgba(20,20,40,.05)` | `inset 0 1px 0 rgba(255,255,255,.7), 0 0 0 1px rgba(80,55,25,.12), 0 1px 3px rgba(80,55,25,.12)` |
| `well` (recessed) | `inset 0 1px 2px rgba(0,0,0,.45), inset 0 0 0 1px rgba(255,255,255,.05)` | `inset 0 1px 2px rgba(20,20,40,.06), inset 0 0 0 1px rgba(20,20,40,.08)` | `inset 0 1px 2px rgba(80,55,25,.10), inset 0 0 0 1px rgba(80,55,25,.10)` |
| `float` (menus, popovers, scrubber) | `inset 0 1px 0 rgba(255,255,255,.08), 0 0 0 1px rgba(0,0,0,.6), 0 8px 20px rgba(0,0,0,.4), 0 24px 48px rgba(0,0,0,.35)` | `0 0 0 1px rgba(20,20,40,.08), 0 4px 12px rgba(20,20,40,.08), 0 20px 40px rgba(20,20,40,.10)` | `inset 0 1px 0 rgba(255,255,255,.6), 0 0 0 1px rgba(80,55,25,.12), 0 6px 16px rgba(80,55,25,.10), 0 22px 44px rgba(80,55,25,.12)` |
| `modal` | float with a 40px/80px outer layer | same, 32px/64px | same, 36px/72px |
| `sheet` (PDF page) | `inset 0 0 0 1px rgba(255,255,255,.06), 0 0 0 1px rgba(0,0,0,.6), 0 4px 10px rgba(0,0,0,.35), 0 16px 32px rgba(0,0,0,.3)` | `0 0 0 1px rgba(20,20,40,.08), 0 2px 6px rgba(20,20,40,.08), 0 12px 28px rgba(20,20,40,.10)` | `0 0 0 1px rgba(80,55,25,.12), 0 2px 6px rgba(80,55,25,.10), 0 12px 28px rgba(80,55,25,.12)` |

Ultra mode and "animations off" do not change depth. **Ultra mode replaces every shadow with its 1px ring only** (the `0 0 0 1px` layer), which costs nothing to draw.

## Typography

- UI: **Inter** (OFL), subset to Latin, Latin-Ext, Greek and Cyrillic and shipped. Other scripts fall back to the system UI font (Segoe UI / SF / Noto via fontconfig) for UI text.
- Reading: **Literata** (default), **Atkinson Hyperlegible Next**, **Source Serif 4**, Inter, and "System". Loaded on first use, not at start-up.
- Code and numbers that must align: **JetBrains Mono** (OFL), subset.
- Numbers in counts, page numbers, zoom and sizes use tabular figures.

| Role | Size / weight | Line height |
|---|---|---|
| UI body | 13 px / 400 (Dense 12.5, Standard 14) | 1.35 |
| UI label, meta | 12 px / 400; 11.5 px for counts | 1.3 |
| UI emphasis | 13 px / 500; section titles 13 px / 600 | |
| Dialog title | 16 px / 600 | |
| Screen title (Home) | 22 px / 600, −0.01em | |
| Reading body | 18 px default (17 Dense, 19 Standard), user-adjustable 14–28 | 1.75 |
| Reading H1 | 32–34 px / 600, −0.01em | 1.2 |
| Kicker above reading H1 | 12.5 px UI face, `faint` | |

Headings are never italic. Emphasis in chrome uses weight or colour.

## Density

| | Dense | Compact (default) | Standard |
|---|---|---|---|
| Control height | 26 | 30 | 36 |
| Icon button | 26 × 26 | 28 × 28 | 32 × 32 |
| List row | 26–28 | 30–32 | 36–38 |
| Title bar | 38 | 44 | 50 |
| Header row | 42 | 50 | 58 |
| UI text | 12.5 | 13 | 14 |
| Icon size | 14 | 15 | 16 |
| Radius (controls) | 7 | 8 | 9 |

Radii: 6–9 px controls, 10–12 px cards and wells, 14–16 px floating panels and dialogs, 2 px page sheets, 999 px pills and switches.

## Icons

- One set, chosen in Settings: **Hairline** (default; 1.4 px stroke, round caps, open shapes), **Geometric** (2 px, square caps, mitred), **Duotone** (1.6 px, round, 20 % accent fill inside closed shapes). Same 24 px grid and the same shapes in all three. Only stroke, caps and fill change.
- Icons are SVG in `assets/icons/`, drawn on the 24 grid, and tinted with `currentColor` from tokens.
- **Status glyphs** (16 px): unread (ring in `ring`), reading (ring + half disc in `accent`), finished (filled disc in `ok` with check), pinned, indexing (dashed ring), signed (shield + check, `ok`), broken (shield + x, `bad`), unknown signer (shield + !, `warn`), encrypted (lock), syncing (two arrows).
- **Placeholders** (56–64 px line drawings, no tile behind them) for: no books, no matches, can't open, password protected, opening, signature needs a look. Each is a title, one sentence and one action.
- No emoji in UI text. No second icon set.

## Components

Every component lists its states. Interactive components implement **default, hover, pressed, focus-visible, disabled**, and where relevant **on/selected, loading, error**.

- **Icon button:** square at the density size, `muted` icon. Hover: `hover` fill, `text` icon. Pressed: `well` + `hover`. On: `raised` + `elev`, `accent` icon (filled glyph where the shape allows). Focus: 2px `bg` gap + 2px `accent` ring, shown instantly. Disabled: `ring` colour, no hover. Always has a tooltip with its shortcut, and an accessible label.
- **Buttons:** *primary* (ink fill with a 2-step shading and `elev`, one per view at most), *secondary* (`raised` + `elev`), *ghost* (text only). Destructive confirmations use `bad`.
- **Icon group / segmented:** a `well` track holding icon buttons, with the selected one `raised` + `elev`. Used in Settings and the ⋯ menu, not as free-standing strips in chrome.
- **Tabs (title bar):** a Home button as a fixed first slot (house icon, `accent` when Home is showing), a divider, then document tabs. The active tab is `raised` + `elev`, with a format icon tinted by state (bookmark = reading, shield = signed). The icon names the format: page for PDF, Markdown mark, book for EPUB, MOBI and FB2, image for comics, text lines for TXT, pages for XPS. Close on hover or active. Pinned tabs are icon-only. Drag to reorder, middle-click to close. A **+** icon button after the last tab opens the system Open dialog (Ctrl+O). Tabs keep their title width up to 220 px; when they don't fit they share the room equally down to 120 px, then the strip scrolls sideways (wheel or trackpad), the active tab is scrolled into view, inactive tabs show their close button over the title on hover, and an **All tabs** button (chevron) lists every tab. A 96 px drag area always stays free.
- **Quick dock** (replaces the old header row): a floating `raised` + `float` panel, 14 px radius, holding `well` groups: sidebar; back/forward; zoom out, zoom %, zoom in, fit width, fit page; find, rotate, ⋯. Settings › Appearance › Quick dock puts it at the **Top**, **Bottom** (default; it then also holds the page navigator, as one bar), **Left** or **Right** (upright, the zoom % small between − and +). It enters with a 6–8 px slide. **Fade the dock while reading** fades it after 1.8 s without pointer movement or keys, never while it is hovered, Find is open or the page field is being edited; it returns on the next movement or key.
- **Tabs placement** (Settings › Appearance › Tabs): **Along the top** (the title bar with tabs) or **In the sidebar**: no title bar; the sidebar opens with a header (sidebar, Home, drag area, open) and an **Open** list (file icon, full title, the page you're on as 13/40, close on hover, middle-click closes, right-click is the tab menu, scrolls past 300 px), then Contents. The window buttons sit on a solid `bg` corner tab at the top right over the page area, which keeps 36 px free above the first page; with the sidebar closed, the sidebar and Home buttons sit on a matching tab at the top left.
- **Layout presets** (Settings › Appearance › Layout, three choice cards): **Full** (animations on, depth, the full dock, the Scrubber, sidebar open) and **Minimal** (animations off, **Flat surfaces** (`ultra` depth: hairline rings, no shadows), the **compact dock** with only zoom, find and ⋯ and with the Counter navigator inside it, sidebar closed). Theme, accent, density, tabs and the dock position are left alone. The current settings show as Full, Minimal or **Custom** when they match neither; changing any covered setting makes it Custom. Flat surfaces and Compact dock are also their own switches.
- **Minimal mode** (F11, ⋯ menu): hides what Settings › Reading › Minimal mode lists (tabs, sidebar, quick dock, page navigator; all on by default) and the reserved top space. Pointing at the top 6 px edge shows the window buttons with **Exit minimal mode**; F11 always restores everything. Session only, not saved.
- **⋯ menu:** Theme row (one swatch per theme: its background with an accent dot, the current one ringed in `accent`), Density row (a `well` group), divider, Contents (F12), Go to page (Ctrl+G), Focus mode (F11), Export highlights, Settings… (Ctrl+,). `float` panel, 260 px.
- **Menus (⋯ and right click):** `raised` + `float`, 12 px radius, 4 px inset. Rows: 16 px icon or check, label, shortcut in `faint`. Disabled rows are `faint` and inert. Separators are 1px `line`. Esc or a click outside closes. Right-click menus exist for pages, tabs, Home cards and contents entries. A menu has at most one toolbar row, at the top. The page menu's row holds `well` groups for back/forward, zoom out/in with the zoom %, and rotate; it shows live state, has tooltips with shortcuts, and keeps the menu open so it can be pressed again. Everything else (fit modes with a check, Copy and Find for a selection, page copies, tools) is an ordinary row. Related file actions are one entry each that opens a dialog (Save page as…), not one row per format. A menu taller than the window minus 8 px margins scrolls.
- **Sidebar:** `panel` background. Its header names the pane and has icon tabs: Contents (outline icon) and Marks (highlighter). The Marks pane starts with "Export  Markdown  CSV" in `faint`, then rows: a 10 px colour dot, the kind (`text`) with its page number (`faint`) on the right, up to three lines of the note or the highlighted words (`muted`, 12 px), and for Kitab's own marks "Not saved into the file yet" or "Kept in Kitab" (`faint`, 11 px) and a delete × that shows on hover. The Bookmarks pane: rows with an `accent` bookmark glyph, the name ("Chapter 2 · page 7"), the current page's row `raised` + `elev`, and a × on hover. A bookmarked page has a 14 × 22 px `accent` ribbon hanging from its top edge, 24 px from the right. Its header has a hide button (sidebar icon, "Hide sidebar Ctrl+B"): on the Contents title with tabs on top, in the sidebar's own head with tabs in the sidebar. Header is the panel title plus icon tabs (Contents, Highlights with an accent dot when there are any, Bookmarks, Pages). The **outline** is grouped rows with status rings, counts and progress pills ("6/17"), with children on a 1px tree line. The active row is `raised` + `elev`.
- **Markdown editing** (Edit button top right of a Markdown document, Ctrl+E): the tab splits into the source (left half) and the live preview (right). The editor pane has a 40 px `panel` toolbar (pencil in `accent`, "Editing" or "Editing · unsaved", a `ok` "Saved" note for 3 s, "Ln, Col", Save (secondary) and Done (primary)), then lines in the mono face at 14 px on 22 px rows with a 52 px gutter of line numbers (`ring`, the caret line's `muted`). The caret is a 2 px `accent` bar, the caret line has a faint `hover` wash, the selection is `accent` at 28 %, an IME composition is underlined in `accent`. Enter keeps indentation and continues `-`, `*`, `+` and task-list markers; Tab inserts four spaces; Home goes to the indentation first. The preview re-parses 180 ms after the last edit. Done or closing the tab with unsaved changes asks Save / Don't save / Keep editing (or Cancel). Long lines don't wrap yet.
- **Markdown reading:** blocks in a centred column (720 px at 100 %; zoom scales the type) on `bg`. Body 16 px, line height 1.6, `read` colour; headings 2 / 1.55 / 1.28 / 1.12 / 1 / 0.92 em, H1 and H2 with a hairline below, H6 `muted`. Links `accent`, underlined at half strength. Inline code and `kbd` in the mono face on `hover`. Code blocks: `panel` box, 10 px radius, a header with the language and Copy, sideways scroll, syntax colours from theme tokens (keyword `accent`, string `ok`, comment `faint` italic, number `warn`, type `warn` 85 %, tag and parameter `bad` 85 %, function `read` bold, punctuation `muted`); unknown languages stay `text`. Settings › Reading › **Code colours** swaps these for a named palette (GitHub, One, Solarized in light and dark variants chosen by the theme's appearance; Dracula, Monokai, Nord dark only), which also sets the code box background; the palettes live in `kitab-theme` (`code.rs`). The setting shows 150 px choice cards with a five-line Rust preview. Quotes: italic `muted` text with a 4 px rule in `accent` at 50 % (as md-viewer-pwa). Tables: hairline rows, header on `panel`, every other row tinted; wider than the column means fixed 160 px columns that scroll in their own box. Task items: a 15 px box, `accent` with a check when done. `<details>` is a bordered row with a chevron that opens it. Frontmatter: a `panel` card of key/value rows. Images up to the column width; remote images and missing files show a bordered note; video and audio links show a card with Open (system player). Right-to-left paragraphs align right. Mermaid and math show their source in a labelled box until they are drawn. Reading face until Literata ships: Cambria (Windows), Georgia elsewhere; mono: Consolas.
- **Reading area:** same surface as the chrome. Column max 640–680 px for reflow. Selection shows the **selection popover** (`pop` ink surface: 3 highlight colours, Note, Copy).
- **Page navigator** (Settings › Reading, each with a live preview): **Scrubber** (default; page field, a track you click or drag, total), **Stepper** (‹ page field "of N" ›), **Counter** (page field and "/ N", bottom right), **Hidden** (nothing until Ctrl+G). In every style the page number is a field: click it or press Ctrl+G, type, Enter goes, Esc or clicking away cancels. While editing it is a `well` with an `accent` border and caret.
- **Scrubber:** floating, centred bottom, `raised` + `float`. Page number, a track (`well`) with chapter ticks and an accent fill, a white thumb, total, and "x min left". It replaces a status bar in reflow and Focus mode. Fixed-layout documents add zoom and page size to a quiet status line.
- **Split view:** two documents side by side, divided by a 1px `line`. The active pane drives the header, contents and shortcuts; clicking the other pane makes it active. The side document's tab gets the `hover` fill.
- **Toast:** bottom centre above the page navigator, `raised` + `float` pill: an `ok` check or `bad` alert icon, one line of text, and an optional accent text button (Open, Show in folder). It rises 10 px in, and leaves after about 4 s.
- **Small dialogs** (Save page, Save pages as a new PDF): 440 px, `raised` + `modal`, 16 px radius, title, one line of help, a `well` field with an `accent` focus border (`bad` when the input is wrong, with the reason below), Cancel (secondary) and the action (primary). **Save page** has a page field, three format choice cards (PDF, Image, Text; PDF is off for non-PDF documents; ←/→ switch) and, for images, a resolution group (96 / 150 / 300 dpi) with the resulting pixel size.
- **Books:** a page is the reading area at 100 %, so fit width shows one page per screen. The dock's zoom shows the text size ("12 pt") and − / + change it by a point; the page re-flows.
- **Writer (notes and text):** a `raised` + `float` card, 280 px, anchored under the point clicked: a `muted` 11 px title ("Note · Ctrl+Enter to keep, Esc to drop"), a `hover` well with the field (UI font 13 px, `accent` caret, `highlight` selection), Cancel and a primary Done. Note pins are 20 px rounded squares in the note's colour with a white note glyph and `elev`; their tooltip is the note.
- **Dock handle:** the dock's last button is a chevron toward its edge ("Hide the dock  Ctrl+."). Hidden, the dock is a 64 × 20 px handle (20 × 64 at the sides), `raised` with a hairline and the `float` shadow, 6 px from its edge, with a chevron pointing back into the window; a click or Ctrl+. shows the dock again. `dock_hidden` in settings.json.
- **File info chip:** bottom left of the reading area (above the dock when the dock is at the bottom, with an 8 px gap), `raised` with a hairline and `elev`, 11.5 px `muted` text on one line, parts joined by "  ·  ": format, size (KB/MB, 1024-based), pages, and the current page's paper name (A4, Letter, …) or its size in points; a book shows its text size, Markdown its lines and words; "Encrypted" when it is. The tooltip is the full path. Hidden in minimal mode and by the setting.
- **Markup bar:** a `raised` + `float` bar centred under the header, like Edge's PDF viewer: tool buttons (pen, highlighter, line, arrow, rectangle, ellipse, eraser; the active one `raised` + `elev` in `accent`), hairline separators, colour swatches (six ink colours, or five highlighter colours, plus a + that opens the picker: 12 hues × 6 OKLCH shades and a grey row, 20 px cells with a hairline, and under them a preview swatch and a hex field: six digits apply as typed, Enter takes three or six and closes, Esc closes), three width dots, undo and redo. The page cursor is a crosshair while a tool is active. Marks keep their ink colours in every theme (they belong to the document). Unsaved marks in a PDF show **Save** in the dock beside the pen, with an `accent` count badge; the bar shows a note ("Saved 3 marks into the PDF", "Kept in Kitab") under it.
- **Find bar:** slides down under the header. Field (`well`) with "3 of 41", previous/next icon buttons, option icon toggles (Aa case, whole word, diacritics), close. Hits use `highlight`; the current hit is `accent` at 45 %. The bar is a `raised` + `float` panel, 440 px, top-right over the pages; it drops 8 px in. Searching starts 140 ms after the last key, from the current page onwards, and the count shows "Searching…" until the first hit; a miss turns the field border and "No results" `bad`.
- **Text selection:** drag on a page (across pages too), double-click a word, triple-click a line, Shift+click extends. Selection is `accent` at 30 % behind the glyphs, one box per line. The pointer is an I-beam over pages and a hand over links; a link shows its address (or "Go to page N") in a tooltip and a faint `accent` wash on hover. Only http, https and mailto links open, in the system browser.
- **Page sheets (fixed layout):** each page at its own size and orientation on `desk`, with the `sheet` shadow and a 2px radius (Settings › Reading › Page corners: Square 2 px, Soft 8 px, Round 16 px at 100 %, scaled with zoom), 22 px between pages, and a faint size label under each page in Pages view. The current page gets a 2px `accent` ring in thumbnails only.
- **Home:** library rail (`panel`) with Home, Library, Favourites, Highlights, Signed, Finished and Collections. Content: title, search (`well`, Ctrl+K), Cards/List icon group, Sort. "Continue reading" cards (cover + title + author + where + progress bar), then Recent as **cards** (2:3 covers with a format badge and state line) or **list** (mini cover, title, author, format, progress, opened), remembered per user.
- **Covers:** the real cover when the file has one, else a generated cover. That's a muted hue derived from the title hash, the title initial in the reading face, and a spine shadow (`inset 3–4px 0 0 rgba(0,0,0,.18)`).
- **Settings modal:** 900–920 px, `raised` + `modal`. Left rail (`panel`) with icon + label sections: General, Appearance, Reading, Performance, Shortcuts, Signing, Privacy, About. Right: 56 px header with title and close, scrolling body. Rows sit in `well` groups with hairlines between. Choice cards (theme previews, startup modes) are bordered, with the selected one `raised` + 2px `accent` ring. The last section used reopens.
- **Switch:** 32 × 18, `well` when off, `accent` when on, white knob with a small shadow. A switch whose parent is overridden (by Ultra mode) dims to 45 % and is not interactive.
- **Print sheet, Signature panel, Command palette, Toasts:** built from the same parts. Toasts are bottom-right, `float`, and auto-dismiss in 4 s except errors.
- **Placeholders:** centred in the pane, drawing + title + sentence + one secondary button.

## Motion

- Durations: 120 ms (hover colour), 150 ms (menus, toggles), 200 ms (panels, tabs), 250 ms max (dialogs, page turn in paged mode). Exits are about 20 % faster than entrances.
- Easing: `out` = cubic-bezier(0.16, 1, 0.3, 1) for entrances, `in` = (0.7, 0, 0.84, 0) for exits, `in-out` = (0.65, 0, 0.35, 1) for moves. A spring (critically damped, no overshoot) for smooth zoom and scroll.
- Animate opacity and transforms only. Never animate layout sizes. GPUI has no element transforms, so slides are a small position offset on an element whose size doesn't change; the sidebar takes its full width at once and its content slides 12 px.
- What moves: menus fade and drop 4 px; Settings and its scrim fade, the panel rises 10 px; the page navigator rises 8 px; new tabs drop in 6 px; contents rows slide in from the left and Home cards rise, each staggered 24 ms (capped at 12 items). Keyboard actions repeated all day (page turns, zoom, Ctrl+G) don't animate.
- Focus rings and selection never animate in.
- **Animations off** and **Ultra mode** snap every transition. `prefers-reduced-motion` caps motion at a 150 ms opacity crossfade. State must land correctly with motion off.
- Focus-mode chrome fades after 1.5 s of reading and returns on pointer movement near an edge or any key.

## Accessibility

- Every control is an AccessKit node with a role, name, state and the keyboard shortcut in its description.
- Full keyboard paths for every action (PLAN.md §3.10). Visible focus everywhere. Escape closes every overlay. Focus returns to where it was.
- Contrast rules in § Colour are checked by a unit test over every theme JSON.
- Text scales with the interface size setting (80–200 %) without clipping. Layouts reflow; nothing is fixed to pixels that hold text.
- Colour is never the only signal: signature states add a shape (check, x, !); highlights have a name in the Annotations panel.
- RTL documents mirror the reading controls (page turn direction, scrubber direction), not the app chrome.

## Do / Don't

Do:
- Use tokens for every colour, shadow and radius.
- Put controls behind ⋯ or in Settings unless people use them every minute.
- Check new UI in all six themes, two accents (one custom), three densities, and Ultra mode.
- Use real content in mockups and goldens (public-domain books, the fixtures).

Don't:
- Draw a segmented switch strip in the chrome, or a tab bar inside a panel when icon tabs will do.
- Use gradients (except the primary button's two-step shading), glass blur, glows, or coloured shadows.
- Fill large areas with the accent, or colour body text.
- Move rows on hover. Hover is colour only. Cards may lift 2–3 px.
- Add emoji, a second sans face, or a second icon set.
- Invent numbers in UI copy, empty states or the landing site.

## Checklist for new UI

1. Tokens only. Checked in Dark, Light, Paper, Nord, Noctua, Vampire, with a custom accent.
2. Density sizes from the table. Checked in Dense, Compact and Standard.
3. All states implemented (default, hover, pressed, focus-visible, disabled, plus on, loading and error where relevant).
4. Works with animations off and in Ultra mode. No shadows needed for meaning.
5. Keyboard path, tooltip with shortcut, AccessKit role and name, Escape closes.
6. Empty, error and loading states written as one plain sentence, with one action.
7. A golden screenshot added or updated, in the same commit.
