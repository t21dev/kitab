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

| Token | Dark | Light | Paper | Nord | Noctua | Vampire | Use |
|---|---|---|---|---|---|---|---|
| `bg` | `#141415` | `#FFFFFF` | `#F4EEE3` | `#2E3440` | `#0E1626` | `#17121A` | Window, chrome **and** reflowed reading surface |
| `panel` | `#111112` | `#FAFAFB` | `#EFE8DB` | `#2A303B` | `#0B1220` | `#130F16` | Sidebar, settings rail, wells |
| `desk` | `#0F0F10` | `#F2F2F4` | `#EBE3D5` | `#292E39` | `#0A101C` | `#120E14` | Behind fixed-layout page sheets |
| `raised` | `#1E1E21` | `#FFFFFF` | `#FAF6EE` | `#3B4252` | `#17223A` | `#231B28` | Raised controls, active tab, menus, cards |
| `hover` | `#18181A` | `#F3F3F5` | `#EDE5D7` | `#343A47` | `#121C30` | `#1D1621` | Hover fill, well fill |
| `line` | `#27272B` | `#E8E8EC` | `#DCD1BF` | `#434C5E` | `#1F2B45` | `#2F2534` | Every hairline |
| `text` | `#ECECEE` | `#18181B` | `#2B241D` | `#ECEFF4` | `#E6ECF7` | `#F2EAF0` | Primary text |
| `read` | `#D9D9DD` | `#232326` | `#2B241D` | `#E5E9F0` | `#DCE3F0` | `#EADFE6` | Reading body text (slightly softer) |
| `muted` | `#A1A1A7` | `#55555D` | `#5F5346` | `#C3CAD6` | `#A9B5CE` | `#C2B2BF` | Secondary text, idle icons |
| `faint` | `#8C8C92` | `#6B6B73` | `#6E6253` | `#A3ACBC` | `#8A97B3` | `#A2909F` | Meta, counts, helper text. Never the only copy of something important |
| `ring` | `#5C5C63` | `#B5B5BC` | `#B5A893` | `#4C566A` | `#33405E` | `#4A3B50` | Unread ring, disabled glyphs, ticks |
| `accent` | `#E6B450` | `#B7791F` | `#A65424` | `#88C0D0` | `#F2A65A` | `#E0466A` | The accent (overridable, see below) |
| `highlight` | `rgba(230,180,80,.30)` | `#FBE2A6` | `#EFD08F` | `rgba(235,203,139,.30)` | `rgba(242,166,90,.28)` | `rgba(224,70,106,.28)` | Default highlight colour behind text |
| `ok` | `#4CC38A` | `#1F9D63` | `#4E8A57` | `#A3BE8C` | `#5FD3A3` | `#6CCB97` | Finished, signature valid |
| `warn` | `#E6B450` | `#B7791F` | `#B87A1E` | `#EBCB8B` | `#F2C14E` | `#E8B04A` | Signer unknown, changed after signing |
| `bad` | `#F06A6A` | `#D33B3B` | `#B5402F` | `#BF616A` | `#F07A7A` | `#FF5C7A` | Errors, broken signature, destructive |
| `scrim` | `rgba(0,0,0,.55)` | `rgba(20,20,30,.28)` | `rgba(60,40,20,.25)` | `rgba(0,0,0,.45)` | `rgba(0,0,0,.55)` | `rgba(0,0,0,.55)` | Behind dialogs |

Nord uses the published Nord palette (MIT; credit in About and NOTICE). Noctua and Vampire are Kitab's own. A new theme must define every token above and pass the contrast check: `text`/`read` ≥ 7:1 on `bg`, `muted` ≥ 4.5:1, `faint` ≥ 4.5:1 on `bg` and `panel`, `accent` ≥ 3:1 as a mark and ≥ 4.5:1 when used as text.

### Accent

- Presets: theme default, Amber, Rose, Blue, Green, Mono. **Custom: any hex.**
- Kitab keeps the hue and chroma of a custom colour and adjusts OKLCH lightness per theme until it reaches 3:1 against `bg` for marks and 4.5:1 where it is text. The hex field shows the measured ratio, and says when it lightens the colour.
- The accent marks: active and selected states, progress bars, the focus ring, the scrubber thumb, reading-state rings, toggled icon buttons and switches. Never large fills, and never body text.

### Depth tokens

| Token | Dark-family themes (Dark, Nord, Noctua, Vampire) | Light | Paper |
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
- **Tabs (title bar):** a Home button as a fixed first slot (house icon, `accent` when Home is showing), a divider, then document tabs. The active tab is `raised` + `elev`, with a format icon tinted by state (bookmark = reading, shield = signed). Close on hover or active. Pinned tabs are icon-only. Drag to reorder, middle-click to close.
- **Header row:** breadcrumb (Library › Title · Author), find field (`well`), zoom group (`raised`), ⋯ button.
- **⋯ menu:** Theme row (one swatch per theme: its background with an accent dot, the current one ringed in `accent`), Density row (a `well` group), divider, Contents (F12), Go to page (Ctrl+G), Focus mode (F11), Export highlights, Settings… (Ctrl+,). `float` panel, 260 px.
- **Menus (⋯ and right click):** `raised` + `float`, 12 px radius, 4 px inset. Rows: 16 px icon or check, label, shortcut in `faint`. Disabled rows are `faint` and inert. Separators are 1px `line`. Esc or a click outside closes. Right-click menus exist for pages, tabs, Home cards and contents entries.
- **Sidebar:** `panel` background. Header is the panel title plus icon tabs (Contents, Highlights with an accent dot when there are any, Bookmarks, Pages). The **outline** is grouped rows with status rings, counts and progress pills ("6/17"), with children on a 1px tree line. The active row is `raised` + `elev`.
- **Reading area:** same surface as the chrome. Column max 640–680 px for reflow. Selection shows the **selection popover** (`pop` ink surface: 3 highlight colours, Note, Copy).
- **Page navigator** (Settings › Reading, each with a live preview): **Scrubber** (default; page field, a track you click or drag, total), **Stepper** (‹ page field "of N" ›), **Counter** (page field and "/ N", bottom right), **Hidden** (nothing until Ctrl+G). In every style the page number is a field: click it or press Ctrl+G, type, Enter goes, Esc or clicking away cancels. While editing it is a `well` with an `accent` border and caret.
- **Scrubber:** floating, centred bottom, `raised` + `float`. Page number, a track (`well`) with chapter ticks and an accent fill, a white thumb, total, and "x min left". It replaces a status bar in reflow and Focus mode. Fixed-layout documents add zoom and page size to a quiet status line.
- **Find bar:** slides down under the header. Field (`well`) with "3 of 41", previous/next icon buttons, option icon toggles (Aa case, whole word, diacritics), close. Hits use `highlight`; the current hit is `accent` at 45 %.
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
