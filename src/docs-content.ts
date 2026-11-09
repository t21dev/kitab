// SPDX-License-Identifier: AGPL-3.0-or-later
// The /docs page: every feature of Kitab Reader, organised into sections and topics.
// Every claim here comes from the app's CHANGELOG.md, README.md, PRIVACY.md or its source
// on main (crates/kitab-ui). Keyboard tables and the command list are generated from the
// source into src/app-keys.ts by `npm run sync`. The page, its Markdown mirror (/docs.md)
// and llms-full.txt are all built from this file.
//
// Inline markup in text: [[Ctrl+F]] is a key, `code`, **bold**, [label](href).
// Copy rules: plain sentences, no em dashes, British spelling, no invented numbers.

import type { IconName } from "./components/Icon";
import { KEY_GROUPS, type KeyGroup } from "./app-keys";
import { FAQ } from "./content";

const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");

export type Img = { src: string; w: number; h: number; alt: string; caption?: string };

export type Block =
  | { p: string }
  | { list: string[]; ordered?: boolean }
  | { img: Img }
  | { table: { head: string[]; rows: string[][] } }
  | { code: string }
  | { note: string }
  /** One group of the shortcut list from src/app-keys.ts. */
  | { keytable: KeyGroup }
  /** Every command palette command from src/app-keys.ts. */
  | { palette: true }
  /** Kitab's window rebuilt in HTML (src/mock), opened on Settings › Appearance. */
  | { mock: "appearance" };

export type Topic = { id: string; title: string; blocks: Block[] };
export type DocSection = { id: string; title: string; icon: IconName; summary: string; topics: Topic[] };

const S = (name: string, alt: string, caption?: string, w = 1253, h = 791): { img: Img } => ({
  img: { src: `/docs/${name}.webp`, w, h, alt, caption },
});

export const DOCS: DocSection[] = [
  // ─────────────────────────────────────────────────────────────── Getting started
  {
    id: "getting-started",
    title: "Getting started",
    icon: "download",
    summary: "Install Kitab on Windows, macOS or Linux and open your first file.",
    topics: [
      {
        id: "install",
        title: "Install",
        blocks: [
          { p: "Every release is on [GitHub Releases](https://github.com/t21dev/kitab/releases/latest). Pick the file for your system:" },
          {
            table: {
              head: ["System", "Download"],
              rows: [
                ["Windows 10 and 11 (Intel, AMD)", "`Kitab_*_x64_setup.exe`, or `Kitab_*_x64_portable.zip` to run without installing"],
                ["Windows on Arm", "The x64 downloads, which Windows runs through its emulation"],
                ["macOS, Apple silicon", "`Kitab_*_aarch64.dmg`"],
                ["macOS, Intel", "`Kitab_*_x86_64.dmg`"],
                ["Linux", "`.flatpak`, `.AppImage`, `.deb`, `.rpm` or `.tar.gz` (x86_64; all but the Flatpak also for aarch64)"],
              ],
            },
          },
          { p: "The Windows installer installs for your account only, so it never asks for an administrator. `kitab.exe` carries its own C runtime: a clean Windows needs nothing else installed first." },
          { p: "Or install with a package manager:" },
          { code: "# macOS, Homebrew\nbrew install --cask t21dev/tap/kitab\n\n# Windows, Scoop\nscoop bucket add t21dev https://github.com/t21dev/scoop-bucket\nscoop install t21dev/kitab" },
          { note: "WinGet and Flathub listings are planned for version 1.0. Until then, use the installer, the portable zip or Scoop on Windows, and the Flatpak bundle from the release on Linux." },
        ],
      },
      {
        id: "linux-packages",
        title: "Flatpak, AppImage, deb and rpm",
        blocks: [
          { p: "**Flatpak.** Install the bundle with `flatpak install --user Kitab_*_x86_64.flatpak`. Flatpak fetches the Freedesktop runtime from [Flathub](https://flathub.org/setup). Inside Flatpak, Kitab can read and save files in your home folder and on external drives, and has no network access, so Flatpak brings the updates instead of Kitab. To make it the default app for PDFs, use your system's Default Applications settings." },
          { p: "**AppImage.** Run `chmod +x Kitab_*.AppImage`, then start it." },
          { p: "**deb, rpm and tar.gz.** These also install a thumbnailer, so file managers show page 1 of your documents. The `.tar.gz` comes with an `install.sh`." },
        ],
      },
      {
        id: "first-run",
        title: "The first run",
        blocks: [
          { p: "The downloads aren't code-signed yet, so your system may warn you the first time:" },
          {
            list: [
              "**Windows:** SmartScreen may say \"Windows protected your PC\". Choose More info, then Run anyway.",
              "**macOS:** right-click Kitab in Applications and choose Open, the first time only.",
            ],
          },
          { p: "Every release lists the SHA-256 checksum of each file, so you can check a download before you run it." },
          S("home-empty", "Kitab's Home on first start: No books yet, a card offering the Welcome to Kitab guide, and Open file and Settings buttons", "A new Home offers the four-page Welcome to Kitab guide."),
          { p: "A new Home offers **Welcome to Kitab**, a four-page PDF about finding your way, marking up, themes and focus mode. You can open it again any time from Settings › Advanced › Welcome guide." },
        ],
      },
      {
        id: "command-line",
        title: "Command line",
        blocks: [
          {
            table: {
              head: ["Command", "What it does"],
              rows: [
                ["`kitab file.pdf other.epub`", "Opens the files as tabs. If Kitab is already running, they open as tabs in that window, which comes forward."],
                ["`kitab --print <file> [--printer NAME] [--to FILE]`", "Prints without a window. File Explorer's right-click Print uses it once Kitab is set up for the file type. On macOS and Linux it prints through CUPS, and `--to` keeps the PDF it would have sent."],
                ["`kitab --version`", "Prints the version."],
                ["`kitab --unregister`", "Takes Kitab's file types and thumbnails off your account. The Windows uninstaller runs it."],
                ["`kitab --thumbnail`", "Draws page 1 of a PDF, or a small preview of a Markdown or text file, for file managers. Linux file managers use it through `dev.t21.kitab.thumbnailer`."],
              ],
            },
          },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Opening files
  {
    id: "opening-files",
    title: "Opening files and formats",
    icon: "file",
    summary: "The formats Kitab reads, the ways to open them, and what it remembers.",
    topics: [
      {
        id: "formats",
        title: "Supported formats",
        blocks: [
          {
            table: {
              head: ["Format", "Notes"],
              rows: [
                ["PDF", "Password-protected files too. Links, contents, text selection, forms and annotations already in the file"],
                ["EPUB", "Flowing and fixed-layout, right-to-left books, every script your system has fonts for"],
                ["MOBI, FB2, TXT", "Flow like EPUB, in your typeface and text size"],
                ["CBZ", "Comics, page by page"],
                ["XPS", "Pages at their own size"],
                ["Markdown", "Tables, task lists, footnotes, code with syntax colours, math, Mermaid diagrams, frontmatter"],
              ],
            },
          },
          { p: "Kitab is a document reader, so it leaves image files to your image viewer. Books with DRM, Kindle KF8 (`.azw3`) books, DjVu and CBR aren't supported yet." },
        ],
      },
      {
        id: "open",
        title: "Ways to open a file",
        blocks: [
          {
            list: [
              "Press [[Ctrl+O]], or click the folder button in the title bar (Settings › Appearance › Toolbar shows or hides it).",
              "Drop a file on Home.",
              "Click a book on Home, or a search result from your library.",
              "Double-click it in your file manager once Kitab is the default app for that type (see [File types](#settings-file-types)).",
              "Start `kitab` with the files on the command line.",
            ],
          },
          { p: "There is one Kitab per user. Opening a file while Kitab is running, by double-clicking a PDF say, adds a tab to the open window and brings it forward instead of starting a second Kitab." },
          { p: "If a file can't be read, its tab says why, for example that the file isn't there any more because it was moved or deleted." },
        ],
      },
      {
        id: "memory",
        title: "What Kitab remembers",
        blocks: [
          {
            list: [
              "The page, zoom and layout of every file you open, so it reopens where you stopped.",
              "Your open tabs: close Kitab and open it again, and every tab comes back on the same page.",
              "Where the window was, how big, whether it was maximised, and which monitor it was on. A place on a screen that's no longer connected is ignored.",
              "Whether the sidebar was open, and whether the dock was hidden.",
            ],
          },
          { p: "Settings › Advanced › Clear reading history forgets Recent files, reading positions and closed tabs. Your marks, bookmarks and open tabs stay." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Reading
  {
    id: "reading",
    title: "Reading",
    icon: "pages",
    summary: "Zoom, fit, rotate, move through pages, and the dock, status bar, focus and presentation modes.",
    topics: [
      {
        id: "page-sizes",
        title: "Pages at their own size",
        blocks: [
          S("mixed-page-sizes", "A PDF at 25 % zoom with an A4 portrait page, an A4 landscape page and a smaller page below, each drawn at its own shape", "One zoom, three page shapes: each page keeps its own size."),
          { p: "Each page keeps its own size and shape. A landscape chart sits between portrait pages instead of being squeezed to one width, and A4, A5, Letter and receipt-sized pages each keep their shape at one zoom." },
        ],
      },
      {
        id: "zoom",
        title: "Zoom and fit",
        blocks: [
          {
            table: {
              head: ["Action", "Keys"],
              rows: [
                ["Zoom in", "[[Ctrl+=]] or [[Ctrl]] and the mouse wheel"],
                ["Zoom out", "[[Ctrl+-]] or [[Ctrl]] and the mouse wheel"],
                ["Fit page", "[[Ctrl+0]]"],
                ["Actual size", "[[Ctrl+1]]"],
                ["Fit width", "[[Ctrl+2]]"],
              ],
            },
          },
          { p: "The dock's − and + buttons and its zoom percentage do the same, and the page's right-click menu has Fit width, Fit page and Actual size. Fit width and fit page follow the window when you resize it." },
          { p: "In a book, zoom changes the text size instead (8 to 32 pt, remembered per book), and [[Ctrl+0]] resets it. In Markdown, zoom scales the text too." },
        ],
      },
      {
        id: "rotate",
        title: "Rotate",
        blocks: [
          { p: "Rotate clockwise with [[Ctrl+Shift+=]] and counter-clockwise with [[Ctrl+Shift+-]], or use the two arrows at the top of the page's right-click menu. Add a Rotate button to the dock or title bar in Settings › Appearance › Toolbar." },
          { p: "To turn pages inside the file itself, use [Page tools](#page-tools)." },
        ],
      },
      {
        id: "navigation",
        title: "Moving through a document",
        blocks: [
          {
            table: {
              head: ["Action", "Keys and mouse"],
              rows: [
                ["Next / previous page", "[[Right]] / [[Left]], or [[N]] / [[P]]"],
                ["First / last page", "[[Home]] / [[End]]"],
                ["Scroll", "[[Down]] / [[Up]], or [[J]] / [[K]]"],
                ["A screen down / up", "[[PgDn]] or [[Space]] / [[PgUp]] or [[Shift+Space]]"],
                ["Go to a page", "[[Ctrl+G]], or click the page number and type"],
                ["Back / forward to where you were", "[[Alt+Left]] / [[Alt+Right]], or the mouse's side buttons"],
              ],
            },
          },
          { p: "In right-to-left books (Arabic, Hebrew, manga) the left arrow goes to the next page." },
          { p: "**Page navigator.** At the bottom, with the dock, sits the page navigator. Choose its style in Settings › Reading: Scrubber (drag or click the track), Stepper (previous and next buttons), Counter (quiet, in the corner) or Hidden ([[Ctrl+G]] when you need it). In Markdown it shows how far through you are." },
          { p: "**Scroll bar.** A scroll bar runs along the right of the pages: drag it, or click beside the thumb to jump there. Settings › Reading › Scroll bar turns it off." },
          { p: "**Middle-click scrolling.** Click the mouse wheel and move the pointer above or below the mark to scroll, faster the further away you go. Any click ends it. Holding the wheel while you drag scrolls only while it's held. It works on Home too." },
        ],
      },
      {
        id: "links-footnotes",
        title: "Links and footnotes",
        blocks: [
          { p: "Page links in a PDF jump to their page, and [[Alt+Left]] comes back. Web and mail links open in your browser; point at one first to see the address. Other kinds of links are ignored." },
          { p: "Clicking a footnote mark (1, [2], *, †) in a book or PDF shows the note in a small card beside it instead of jumping away. Go to note jumps there ([[Alt+Left]] comes back), and [[Esc]] or a click elsewhere closes the card." },
        ],
      },
      {
        id: "select-copy",
        title: "Selecting and copying text",
        blocks: [
          { p: "Drag across the page to select text, double-click a word, or triple-click a line. [[Shift]]+click extends the selection, [[Ctrl+C]] copies it and [[Esc]] clears it. Copying follows the document's copy permission." },
          { p: "Right-click a selection for its own menu: Copy, Find, Highlight, Underline and Strike through." },
        ],
      },
      {
        id: "page-menu",
        title: "The page's right-click menu",
        blocks: [
          S("page-menu", "The page's right-click menu: a row of back, forward, zoom and rotate buttons, then Fit width, Fit page, Actual size, Go to page, Bookmark this page, Copy text of page 1, Copy page 1 as image, Save page as, Page tools, Save pages as a new PDF, Merge with other PDFs, Open to the side, Copy file path and Show in folder"),
          { p: "Right-click a page for back and forward, zoom and rotate in one row of buttons, then the fit modes, Go to page…, Bookmark this page, Copy text of the page, Copy the page as an image (a 150 dpi PNG), Save page as…, Page tools…, Save pages as a new PDF…, Merge with other PDFs…, Open to the side, Copy file path and Show in folder. In Markdown it keeps to what applies there." },
        ],
      },
      {
        id: "dock",
        title: "The quick dock",
        blocks: [
          S("reader-pdf", "A PDF open in Kitab with its contents in the sidebar, the quick dock floating at the bottom with the page slider and zoom, and the status bar below", "The quick dock floats over the page, here at the bottom with the page navigator."),
          { p: "The dock floats over the page with paging and zoom, and any of these buttons you choose: sidebar, back and forward, zoom, fit width and fit page, find, bookmark, draw and mark up, rotate, focus mode, print and password. Pick which sit in the dock, which in the title bar, and which nowhere in Settings › Appearance › Toolbar." },
          {
            list: [
              "Put it at the top, bottom (the default, together with the page navigator), left or right in Settings › Appearance › Quick dock.",
              "**Compact dock** keeps only zoom, find and the ⋯ menu.",
              "**Fade the dock while reading** lets it fade until you move the pointer or press a key.",
              "In a narrow window, back and forward, rotate, fit and finally the sidebar button step aside. Their keys and the command palette still work.",
            ],
          },
          { p: "**Hide it.** The chevron at the dock's end, or [[Ctrl+.]], tucks the dock into a small handle at its edge. A click on the handle or [[Ctrl+.]] brings it back. The handle shows how many changes aren't saved yet, and Kitab remembers the hidden dock across launches." },
          S("dock-hidden", "A PDF page with the dock hidden: only a small handle with an up chevron remains at the bottom edge", "The hidden dock leaves a small handle at its edge."),
        ],
      },
      {
        id: "status-bar",
        title: "The status bar",
        blocks: [
          { p: "A slim bar along the bottom of the page. On the left: facts about the file. On the right: where you are. [[Ctrl+I]] shows or hides it, and each item has its own switch in Settings › Reading › Status bar:" },
          {
            table: {
              head: ["Item", "Shows"],
              rows: [
                ["Format", "PDF, EPUB, Markdown…"],
                ["File size", "The size of the file"],
                ["Pages", "The page count; lines in Markdown"],
                ["Word count", "Markdown"],
                ["Reading time", "Markdown, at 230 words a minute"],
                ["Paper size", "The current page's, or a book's text size"],
                ["File path", "Where the file is, after the other facts. Your home folder shows as ~"],
                ["Position", "On the right: page 3 of 40, or how far through"],
                ["Zoom", "On the right"],
              ],
            },
          },
          { p: "Point at the facts for the file's location. In a signed PDF, the status bar also shows the signature's state; click it to open the [Signatures pane](#pane-signatures)." },
        ],
      },
      {
        id: "focus-mode",
        title: "Focus mode",
        blocks: [
          S("focus-mode", "Focus mode: only the PDF page on a dark background, with two quiet buttons in the bottom-right corner"),
          { p: "[[F11]] hides the tabs, the sidebar, the dock and the page navigator for distraction-free reading. Choose which of them it hides in Settings › Reading › Focus mode. Point at the top edge for the window buttons and a way back." },
          { p: "Two quiet buttons stay in the bottom-right corner: the sidebar (contents and bookmarks; [[Ctrl+B]] works too) and Exit focus mode." },
        ],
      },
      {
        id: "presentation",
        title: "Presentation mode",
        blocks: [
          S("presentation", "Presentation mode: one PDF page fitted to the screen on black", undefined, 1280, 720),
          { p: "[[F5]] shows the document full screen on black, one page at a time, fitted to the screen. The next page is drawn ahead." },
          {
            table: {
              head: ["Go", "Keys and mouse"],
              rows: [
                ["Forward", "[[Right]], [[Down]], [[Space]], [[PgDn]], a click or the wheel"],
                ["Back", "[[Left]], [[Up]], [[PgUp]] or a right click"],
                ["First / last page", "[[Home]] / [[End]]"],
                ["End the presentation", "[[Esc]] or [[F5]], on the page you were showing"],
              ],
            },
          },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Tabs
  {
    id: "tabs",
    title: "Tabs, windows and split view",
    icon: "tabs",
    summary: "Many documents in one window, side by side when you need it.",
    topics: [
      {
        id: "tab-basics",
        title: "Tabs",
        blocks: [
          {
            list: [
              "Every document opens in a tab. Tabs shrink to share the bar, then the bar scrolls and the open tab stays in view. **All tabs** lists them all.",
              "Each tab shows an icon for the file's format: book, comic, text or page.",
              "[[Ctrl+Tab]] and [[Ctrl+Shift+Tab]] (or [[Ctrl+PgDn]] and [[Ctrl+PgUp]]) move between tabs; [[Alt+1]] to [[Alt+8]] go to a tab and [[Alt+9]] to the last.",
              "[[Ctrl+W]] or [[Ctrl+F4]] closes a tab, and so does a middle-click. [[Ctrl+Shift+T]] reopens the last one you closed.",
              "Right-click a tab for Close, Close other tabs, Close tabs to the right, Reopen closed tab and more.",
              "The house button, or [[Ctrl+Shift+H]], goes to Home.",
            ],
          },
          { p: "The + after the tabs opens Home, with Open file… and your recent books. Settings › Appearance › Tabs › The + button can make it open the Open dialog straight away instead. **Show how many tabs are open** puts a small count at the right of the title bar." },
          { p: "Prefer full titles? Settings › Appearance › Tabs can list the tabs in the sidebar instead of along the top, with the page you're on." },
          { p: "Closing a tab or the window with unsaved Markdown edits or a filled-in form asks first." },
        ],
      },
      {
        id: "split-view",
        title: "Split view",
        blocks: [
          { p: "Open a second document beside the first with [[Ctrl+\\]] (Split view with the next tab), or Open to the side on a tab or a Home card. Click a pane to make it the active one." },
        ],
      },
      {
        id: "window",
        title: "The window",
        blocks: [
          { p: "Kitab draws its own title bar. The Find, Draw, Password, Open file… and More (⋯) buttons sit beside the window buttons and act on the open tab; in a narrow window only More stays up top and the rest go back to the dock. The window can't be made smaller than 640 × 460, where the title bar, Home and Settings stop fitting." },
          S("more-menu", "The More menu: rows of theme and accent swatches, Dense, Compact and Standard density, then Contents, Go to page, Focus mode, Print, Page tools, Sign, Password, Command palette, Keyboard shortcuts and Settings", "The ⋯ menu: theme, accent and density, then the commands."),
          { p: "The ⋯ menu holds the theme, accent and density, then Contents, Go to page…, Focus mode, Print…, Page tools…, Sign…, Password…, Command palette, Keyboard shortcuts and Settings…." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Home & library
  {
    id: "home-library",
    title: "Home and library",
    icon: "library",
    summary: "Shelves, collections, favourites, covers, watched folders, reading stats and library search.",
    topics: [
      {
        id: "home",
        title: "Home",
        blocks: [
          S("home-library", "Home with a search box, Settings and Open file buttons, the shelves Recent, All, Favourites, In progress, Finished and + Collection, and a row of book covers drawn from each document's first page"),
          { p: "Home shows what you were reading and where you stopped. Its title, search box and shelf chips stay put while the books scroll beneath them, and only the rows on screen are drawn, so it stays quick with a big library." },
          { p: "**Shelves:** Recent, All, Favourites, In progress, Finished, and your own collections. When the library's documents come only from watched folders, Home opens on All." },
          { p: "**Covers.** Each book shows its own first page as its cover, drawn in the background and kept on disk so each is drawn once. Favourites wear a red ribbon. In Settings › Home choose Covers (a shelf of first pages) or List (a small cover, title, author, format and progress on each row), and the cover size: Small, Medium or Large." },
        ],
      },
      {
        id: "collections",
        title: "Favourites and collections",
        blocks: [
          { p: "Right-click a book for Favourite, Mark finished, a tick for each collection and New collection…. A document can be in several collections, so they work as tags. Right-click a collection to rename or delete it." },
          { p: "The same menu has Open, Open to the side, Copy file path and Show in folder." },
        ],
      },
      {
        id: "library-search",
        title: "Library search",
        blocks: [
          { p: "The search box on Home finds words across every document you've opened. Each document is indexed in the background a few seconds after it opens, and again when the file changes. Prefix matches count, accents are ignored, and Bangla and Arabic marks stay with their letters." },
          { p: "Results show the page and the words around the match. Click one to open the document at that page." },
        ],
      },
      {
        id: "watched-folders",
        title: "Watched folders",
        blocks: [
          S("settings-library", "Settings, Library: Watched folders, with No watched folders yet, and Add folder and Look again now buttons"),
          { p: "Settings › Library › Add folder… watches a folder. Its documents, and those in the folders inside it, join your library without being opened: they show in Home › All and can go in collections. Library search covers each one once it has been opened." },
          { p: "Kitab looks in your folders when it starts and when you press Look again now, and says how many documents it found. Nothing is moved or changed. Removing a folder asks first, and takes the documents found only there out of your library; ones you've opened, favourites and documents in a collection stay. Nothing on disk is touched." },
        ],
      },
      {
        id: "reading-stats",
        title: "Reading stats and Continue reading",
        blocks: [
          { p: "Above the books, Home can show your reading stats: time read today and this week, your current and longest streak, and a bar for each day of the last four weeks. A day counts with a minute of reading. Time counts while you're active in a document (input in the last two minutes) and is stored per day in Kitab's library on your computer." },
          { p: "**Continue reading** shows up to three books you're partway through, on Recent. Turn either off in Settings › Home." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Find
  {
    id: "find",
    title: "Find",
    icon: "find",
    summary: "Find words in a PDF, a book or a Markdown file, with every hit highlighted.",
    topics: [
      {
        id: "find-in-document",
        title: "Find in a document",
        blocks: [
          S("find", "The find bar over a PDF with the word serif typed, showing 2 of 328, and every match highlighted on the page"),
          { p: "Press [[Ctrl+F]] or [[/]], or the search button in the title bar. Every hit is highlighted on the pages, and the bar counts them: \"2 of 328\". Find works the same in PDFs, books and Markdown." },
          {
            table: {
              head: ["Action", "Keys"],
              rows: [
                ["Next result", "[[F3]] or [[Enter]]"],
                ["Previous result", "[[Shift+F3]] or [[Shift+Enter]]"],
                ["Match case", "[[Alt+C]]"],
                ["Whole word", "[[Alt+W]]"],
                ["Close find", "[[Esc]]"],
              ],
            },
          },
          { p: "To find the words you've selected, right-click the selection and choose Find. To search every document at once, use [library search](#library-search) on Home." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Sidebar
  {
    id: "sidebar",
    title: "The sidebar",
    icon: "outline",
    summary: "Contents, Marks, Pages, Bookmarks and Signatures, one pane at a time.",
    topics: [
      {
        id: "sidebar-basics",
        title: "Showing the sidebar",
        blocks: [
          { p: "[[Ctrl+B]] or [[F12]], or the dock's sidebar button, shows and hides the sidebar, and it has its own hide button in its header. Kitab remembers whether it was open. Pick a pane with the icons along its top. In a small window the sidebar narrows to give the page room." },
        ],
      },
      {
        id: "pane-contents",
        title: "Contents",
        blocks: [
          { p: "The document's contents list, with the current chapter marked and page numbers on the right. Click an entry to go there. Right-click it for Go to page and Copy title. In Markdown, the headings make the outline." },
        ],
      },
      {
        id: "pane-marks",
        title: "Marks",
        blocks: [
          S("marks-pane", "The Marks pane listing a highlight, an underline, a strike-through, a note, a text box, a pen drawing and a rectangle, each with its page number, and Export to Markdown or CSV at the top"),
          { p: "Every mark you've made and every annotation already in the PDF, in page order, with the words a highlight covers and a note's text. Click one to go to its page. Export them all to Markdown or CSV from the top of the pane." },
          { p: "The bin on a row deletes a mark ([[Ctrl+Z]] brings it back). Annotations the PDF already had are outlined in red until you save, when they are taken out of the file; the arrow on the row keeps one after all." },
        ],
      },
      {
        id: "pane-pages",
        title: "Pages",
        blocks: [
          S("pages-pane", "The Pages pane with page thumbnails drawn at each page's own shape, the current page ringed in the accent colour"),
          { p: "Thumbnails at each page's true shape, with the current page ringed and bookmarked pages marked. Only the visible ones are drawn. Click one to go to its page." },
        ],
      },
      {
        id: "pane-bookmarks",
        title: "Bookmarks",
        blocks: [
          { p: "[[Ctrl+D]], or Bookmark this page in the page's right-click menu, bookmarks the page, named after its chapter; press it again to remove the bookmark. Bookmarked pages carry a ribbon, and the Bookmarks pane lists them. In Markdown, a bookmark is named after its section (\"Lists\"). Books that re-flow don't take bookmarks yet." },
        ],
      },
      {
        id: "pane-signatures",
        title: "Signatures",
        blocks: [
          { p: "In a signed PDF, a shield icon adds the Signatures pane: one card per signature. See [Verifying signatures](#verify)." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Markdown
  {
    id: "markdown",
    title: "Markdown",
    icon: "markdown",
    summary: "GitHub-style Markdown with code, Mermaid, math, live reload and an editor.",
    topics: [
      {
        id: "md-rendering",
        title: "What renders",
        blocks: [
          S("markdown", "A Markdown file in Kitab: Inline formatting with bold, italic, strike-through, inline code and links, with the heading outline in the sidebar"),
          { p: "Markdown reads like a document, not a code file, with its headings as an outline in the sidebar. Kitab renders GitHub-style tables, task lists, footnotes, code blocks, quotes, collapsible details, local images, links between files and to headings, and frontmatter as a properties card. Find, zoom, bookmarks and session restore work as in any document." },
          { p: "Unsafe HTML and scripts are blocked. Remote images are blocked too, and show a placeholder naming the site they would come from." },
        ],
      },
      {
        id: "md-code",
        title: "Code blocks",
        blocks: [
          S("markdown-code", "Go, SQL and Bash code blocks with syntax colours, each with a language label and a copy button, and a horizontal scroll bar under the Bash block"),
          { p: "Code blocks have syntax colours that follow the theme: Rust, JavaScript and TypeScript, Python, Go, C and C++, C#, Java, shell, SQL, YAML, JSON, HTML, CSS and more. The copy icon copies a block and shows a tick for a moment." },
          { p: "A block wider than the page has its own scroll bar: drag it, or click along it to jump. [[Shift]]+wheel scrolls a block sideways; the plain wheel scrolls the page." },
          { p: "Settings › Reading › Code colours picks the palette: Kitab (follows the theme), GitHub, One and Solarized (each with a light and a dark variant that follow the theme), Dracula, Monokai and Nord." },
        ],
      },
      {
        id: "md-mermaid",
        title: "Mermaid diagrams",
        blocks: [
          S("markdown-mermaid", "A Markdown file of Mermaid diagrams, with a flowchart drawn in the theme's colours and the list of diagram types in the sidebar"),
          { p: "Fenced `mermaid` blocks are drawn as diagrams: flowcharts, sequence, class, state, entity-relationship, Gantt, pie, mindmap, timeline and git graphs, in the theme's colours and sharp at any zoom. They are drawn in Rust, with no browser or JavaScript. A diagram that can't be read shows its source and why." },
        ],
      },
      {
        id: "md-math",
        title: "Math",
        blocks: [
          S("markdown-math", "Math in Markdown: inline math in a sentence, and a typeset integral and a matrix with its determinant on their own lines"),
          { p: "`$$…$$` and fenced `math` blocks are typeset: fractions, roots, sub- and superscripts, sums and integrals with limits, matrices and cases, Greek and symbols. Inline `$…$` reads as one line of Unicode (πr², x₁ ≤ α). TeX that can't be read shows its source with a mark." },
        ],
      },
      {
        id: "md-typography",
        title: "Typeface and text size",
        blocks: [
          S("markdown-typography", "The Markdown typography panel open above the dock: Typeface with System, Serif and Mono, Text size with minus and plus at 100 %, and Default size"),
          { p: "The Aa button in Markdown's dock opens a small panel: pick the typeface (System, Serif or Mono) and change the text size. It stays open while you try them. System is the default; Settings › Reading › Markdown typeface sets it too." },
        ],
      },
      {
        id: "md-edit",
        title: "Editing",
        blocks: [
          S("markdown-edit", "Editing Markdown: the source with line numbers on the left, Save and Done at the top, and the live preview on the right"),
          { p: "Press Edit, or [[Ctrl+E]], to edit the source beside a live preview. The editor has undo and redo, the clipboard, input methods, list continuation and line numbers. [[Ctrl+S]] saves. Closing with unsaved changes asks first." },
        ],
      },
      {
        id: "md-live",
        title: "Live reload, selecting and printing",
        blocks: [
          { p: "**Live reload.** When the Markdown file you're reading is saved by another editor, Kitab shows the new version in place within a second and keeps your position. Only the tab on screen is checked, and not while you edit in Kitab." },
          { p: "**Select text.** Drag across paragraphs, headings, lists, tables and code; double-click a word, triple-click a paragraph or a line of code, [[Shift]]+click to extend. [[Ctrl+C]] copies, and the right-click menu offers Copy and Find." },
          { p: "**Print.** [[Ctrl+P]] lays the document out on A4, with headings, lists, tables, quotes and code, and prints it like any other document." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Books
  {
    id: "books",
    title: "Books and comics",
    icon: "logo",
    summary: "EPUB, MOBI, FB2 and text that re-flow in your typeface, plus comics and XPS.",
    topics: [
      {
        id: "book-reading",
        title: "Reading a book",
        blocks: [
          S("book-epub", "An EPUB open in Kitab: the chapter list in the sidebar, the text set on the theme's own page colour, and the dock showing 12 pt text size"),
          { p: "EPUB, MOBI, FB2 and text files fill the window: pages re-flow when the window settles at a new size, and the reader keeps its place in the chapter. Zoom changes the text size (8 to 32 pt, remembered) and [[Ctrl+0]] resets it." },
          { p: "Books read in the theme's colours: the page is the window's own surface and the text its reading colour, in every theme; pictures keep theirs. Fixed-layout books, comics, XPS and PDF stay on white paper." },
          { p: "Books in Bangla, Arabic, Hebrew, Chinese, Japanese, Korean and other scripts use your system's fonts. Right-to-left books turn the other way." },
        ],
      },
      {
        id: "book-typography",
        title: "Typography",
        blocks: [
          { p: "Settings › Reading › Books sets the typeface, line spacing and margins for EPUB, MOBI, FB2 and text files, over the book's own styles. Pages and the contents' page numbers follow at once, at the same place in the book." },
          {
            table: {
              head: ["Setting", "Choices"],
              rows: [
                ["Typeface", "Book's own, Serif, Sans, Mono"],
                ["Line spacing", "Book's own, 1.2, 1.5, 1.8, 2.2"],
                ["Margins", "Book's own, Narrow, Medium, Wide"],
              ],
            },
          },
        ],
      },
      {
        id: "fixed-layout",
        title: "Fixed-layout books, comics and XPS",
        blocks: [
          { p: "Fixed-layout EPUBs show one page per spread at their own size, like a PDF, and don't re-flow. CBZ comics read page by page, and XPS pages keep their own size. All of them have contents, find where there's text, and thumbnails." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Markup
  {
    id: "markup",
    title: "Markup and annotations",
    icon: "highlight",
    summary: "Highlight, draw, add notes and text, move marks with Select, and save them into the PDF.",
    topics: [
      {
        id: "markup-bar",
        title: "The markup bar",
        blocks: [
          S("markup-draw", "The markup bar over a PDF page, with a red ellipse, a red line and a blue arrow drawn on the page and the dock's save button showing 3 unsaved marks"),
          { p: "Press [[Ctrl+Shift+D]], or the pen in the title bar, to show the markup bar over the page. Its tools, from the left:" },
          {
            table: {
              head: ["Tool", "What it does"],
              rows: [
                ["Select", "Drag a drawing, shape, highlight, text box or note to move it ([[Shift]] keeps it level or upright). [[Delete]] removes the picked one, [[Esc]] lets it go, [[Ctrl+Z]] undoes a move"],
                ["Pen", "Draw freehand"],
                ["Highlighter", "Draw a see-through stroke"],
                ["Line, Arrow", "[[Shift]] keeps them at 45°"],
                ["Rectangle", "[[Shift]] makes a square"],
                ["Cover", "A solid box over something you want hidden on the page"],
                ["Ellipse", "[[Shift]] makes a circle"],
                ["Text", "Click where it goes and type on the page"],
                ["Note", "Click to pin a note; its text shows when you point at it, and a click edits it"],
                ["Eraser", "Click a mark to remove it"],
              ],
            },
          },
          { p: "Then come six colours, a + with 84 more and a field for any hex code (#RRGGBB or #RGB), three widths, and undo and redo ([[Ctrl+Z]], [[Ctrl+Y]] or [[Ctrl+Shift+Z]]). [[Esc]] puts the pen down. Typing in notes and text boxes works with input methods (Bangla, Arabic, CJK)." },
          { note: "Cover is only a cover. The text underneath stays in the file, and the tool's tooltip says so." },
        ],
      },
      {
        id: "select-tool",
        title: "Moving and deleting marks",
        blocks: [
          S("markup-select", "The markup bar with the Select tool's tooltip: Select, drag a mark to move it, Delete removes it"),
          { p: "The Select tool, first on the markup bar, picks up any mark you've made: drag it somewhere else, or press [[Delete]] to remove it. You can also delete a mark from the [Marks pane](#pane-marks), or click it with the Eraser." },
        ],
      },
      {
        id: "highlight-text",
        title: "Highlighting text",
        blocks: [
          { p: "Select text and press [[H]] to highlight it, or right-click the selection for Highlight, Underline or Strike through." },
        ],
      },
      {
        id: "saving-marks",
        title: "Saving marks into the PDF",
        blocks: [
          { p: "Marks are kept as you draw. In a PDF, [[Ctrl+S]] or Save in the dock writes them into the file as standard PDF annotations, so other readers show them too. The dock's save button counts what isn't saved yet." },
          { p: "Kitab adds the marks after the file's original bytes, without changing them, so signatures already in the file stay valid. Comics, XPS and fixed-layout books keep their marks in Kitab instead." },
          { p: "Settings › Reading › Drawing and markup turns the tools off if you never use them." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Forms
  {
    id: "forms",
    title: "PDF forms",
    icon: "check",
    summary: "Fill in text fields, checkboxes, radio buttons and lists, and save without breaking a signature.",
    topics: [
      {
        id: "filling",
        title: "Filling in a form",
        blocks: [
          S("pdf-form", "A PDF form with tinted fields: Full name, City, a Newsletter checkbox, Small and Large radio buttons with Large chosen, Branch, and an Interests list, and the dock's save button showing 2 unsaved changes"),
          { p: "Fillable fields have a light tint. Fill them in right on the page, at the field's size:" },
          {
            list: [
              "Click a checkbox to tick it, and a radio button to choose it.",
              "Click a list to pick from its choices.",
              "Click a text field to type in it. [[Tab]] moves to the next text field.",
            ],
          },
          { p: "A field shown on several pages fills in on all of them. Read-only and signature fields can't be typed into." },
          { note: "Kitab never runs a document's scripts, so a form's own calculations, formats and checks don't run: values go in as you type them." },
        ],
      },
      {
        id: "saving-forms",
        title: "Saving a filled form",
        blocks: [
          { p: "[[Ctrl+S]] writes the values into the file after its original bytes. A signature stays valid, and the Signatures pane reads \"form filled in since signing\". Closing a tab or the window with a form filled in but not saved asks first." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Signatures
  {
    id: "signatures",
    title: "Signatures",
    icon: "shield",
    summary: "Check whether a PDF's signatures are valid, and sign PDFs with your own certificate.",
    topics: [
      {
        id: "verify",
        title: "Verifying signatures",
        blocks: [
          S("signatures-pane", "A signed lease agreement with the Signatures pane open: the card says Signer not trusted, names the signer, the time, reason and place, says the signed version is intact, and offers Go to page and Trusted certificates"),
          { p: "Kitab checks every signature when a PDF opens, on your computer: the signed bytes against the signed digest, the signature against the signer's certificate, the certificate chain against the certificates you trust, the certificate's validity at signing time, how much of the file is covered, and what changed in later revisions (form filling, annotations or other changes)." },
          { p: "The status bar shows the result: valid, signer not trusted, changed after signing, or broken. Click it to open the Signatures pane, with one card per signature: its signer, time, reason, place and findings in plain words, Go to page, and Trusted certificates… whenever the signer isn't trusted." },
          { p: "Supported: ECDSA P-256 and P-384, RSA PKCS #1 v1.5 and PSS, with SHA-1 to SHA-512." },
        ],
      },
      {
        id: "trusted-certificates",
        title: "Trusted certificates",
        blocks: [
          S("settings-signing", "Settings, Signing: Trusted certificates, with No trusted certificates yet and an Add certificate button"),
          { p: "A signature from a certificate you don't trust shows as \"signer not trusted\". Add your organisation's root certificate, or the signer's own, in Settings › Signing › Add certificate…. Remove takes one off. Checking never asks the internet." },
        ],
      },
      {
        id: "sign",
        title: "Signing a PDF",
        blocks: [
          S("sign-dialog", "The Sign this PDF dialog: a Choose button for the certificate file, the certificate file's password, Reason and Place, and Next: place it"),
          {
            list: [
              "Choose Sign… from the ⋯ menu or the command palette.",
              "Choose your certificate file (.pfx or .p12, with an ECDSA P-256 or P-384 or an RSA key) and type its password. Add a reason and a place if you like.",
              "Press Next: place it, and click on a page where the signature goes.",
              "Pick where the signed copy is saved.",
              "Check the summary of the signer, the page and the file, and confirm.",
            ],
            ordered: true,
          },
          { p: "The original stays as it is. The signature is added as an incremental update, so signatures already in the file stay valid. Your key and its password never leave your computer." },
          { note: "Encrypted PDFs can't be signed yet. Save a copy without the password first ([Passwords](#passwords))." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Passwords
  {
    id: "passwords",
    title: "Passwords",
    icon: "lock",
    summary: "Open password-protected PDFs, and add, change or remove a password.",
    topics: [
      {
        id: "open-locked",
        title: "Opening a protected PDF",
        blocks: [
          S("password-prompt", "A locked PDF: This document is password protected, a password field and an Open button"),
          { p: "A protected PDF asks for its password in its tab. Type it and press [[Enter]] or Open. Kitab doesn't keep it." },
        ],
      },
      {
        id: "set-password",
        title: "Adding, changing or removing a password",
        blocks: [
          S("password-dialog", "The Add a password dialog with two password fields and a Save copy button"),
          { p: "The lock in the title bar, ⋯ › Password…, or the palette puts a password on a PDF, changes it, or takes it off. Kitab saves a new copy (AES-256) and leaves the open file alone. A PDF whose owner restricts it keeps its password unless you opened it with the owner password." },
          { p: "The password fields show dots and refuse copy and cut, and the password is never stored. Keep it somewhere safe: nobody can recover it." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Page tools
  {
    id: "page-tools",
    title: "Page tools",
    icon: "tools",
    summary: "Delete, move, turn, compress, number, watermark, extract and merge, on your computer.",
    topics: [
      {
        id: "page-tools-dialog",
        title: "Page tools",
        blocks: [
          S("page-tools", "The Page tools dialog: Delete pages, Move pages, Turn pages, Compress, Page numbers, Watermark, Save text and Save pictures, with a Pages field and Save"),
          { p: "Page tools… is in the ⋯ menu, the page and tab menus, and the palette. Each tool writes a new file and reports what it did; the original stays as it is." },
          {
            table: {
              head: ["Tool", "What it does"],
              rows: [
                ["Delete pages", "A copy without the pages you list (at least one has to stay)"],
                ["Move pages", "Moves the pages you list before another page"],
                ["Turn pages", "Turns the pages you list right, left or upside down"],
                ["Compress", "A smaller copy, and says how much smaller"],
                ["Page numbers", "Numbers the pages, starting at the number you choose"],
                ["Watermark", "Puts your text, like DRAFT, on the pages"],
                ["Save text", "All the text, page by page, as a plain .txt file"],
                ["Save pictures", "Every picture on the pages, as PNG files in a folder you choose"],
              ],
            },
          },
          { p: "Numbers and watermarks sit level on turned pages too. Page tools work on PDFs." },
        ],
      },
      {
        id: "extract-merge",
        title: "Save pages, merge, and save one page",
        blocks: [
          { p: "**Save pages as a new PDF** ([[Ctrl+Shift+E]]): type pages and ranges such as `1-3, 7, 10-`. It works on password-protected PDFs while they're open." },
          { p: "**Merge with other PDFs** (the page's right-click menu): pick more files, and Kitab saves them, in order, after the open one as a new PDF." },
          { p: "**Save page as…** (the page's right-click menu): save one page as a PDF, an image or plain text, choosing the image resolution." },
          { p: "A message reports each saved file, with Open and Show in folder." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Printing
  {
    id: "printing",
    title: "Printing",
    icon: "print",
    summary: "Print with your system's own print dialog, or from the command line.",
    topics: [
      {
        id: "print",
        title: "Printing a document",
        blocks: [
          { p: "Press [[Ctrl+P]], or choose Print… in the ⋯ menu, the palette or a toolbar button. PDFs that forbid printing say so." },
          {
            list: [
              "**Windows:** Windows' own print dialog opens straight away, with the printer, copies and which pages: all, the current page, or ranges like 1-3, 7. Pages print shrunk to fit the paper and turned to match it.",
              "**macOS:** the system print panel opens as a sheet on the window.",
              "**Linux:** the desktop's print portal (GNOME, KDE, and inside Flatpak). Without a print portal, Kitab's own sheet lists the printers CUPS knows, with the default chosen.",
            ],
          },
          { p: "Kitab's own sheet chooses all pages, this page or a range; fit to paper, actual size or shrink to fit; and whether pages turn to match the paper. Pages are drawn at the printer's resolution, up to 300 dpi, in the background, with progress in a message." },
          { p: "Markdown prints too, laid out on A4. To print without opening a window, see [Command line](#command-line)." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Palette
  {
    id: "command-palette",
    title: "Command palette",
    icon: "search",
    summary: "Every command by name, with its keys.",
    topics: [
      {
        id: "palette",
        title: "Using the palette",
        blocks: [
          S("command-palette", "The command palette over a PDF, filtered by the word zoom to Zoom in and Zoom out, each with its keys"),
          { p: "Press [[Ctrl+K]] or [[Ctrl+Shift+P]], or choose Command palette in the ⋯ menu. Type a few letters (\"gtp\" finds Go to page), choose with [[Up]] and [[Down]], and press [[Enter]] to run. Commands that need a document hide on Home. The palette shows the keys in force, including your own from keymap.json." },
        ],
      },
      { id: "palette-commands", title: "Every command", blocks: [{ palette: true }] },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Shortcuts
  {
    id: "keyboard",
    title: "Keyboard shortcuts",
    icon: "keyboard",
    summary: "Every default key, and how to change them with keymap.json.",
    topics: [
      {
        id: "shortcut-tables",
        title: "The defaults",
        blocks: [
          { p: "These are the default keys, generated from the list Settings › Shortcuts and [[Ctrl+/]] show. Kitab binds the same keys on Windows, macOS and Linux, so on a Mac they use Ctrl too." },
        ],
      },
      ...KEY_GROUPS.map((g) => ({ id: `keys-${slug(g.title)}`, title: g.title, blocks: [{ keytable: g }] })),
      {
        id: "keymap",
        title: "Your own keys: keymap.json",
        blocks: [
          { p: "Settings › Shortcuts › Edit keymap.json (or the palette) creates a `keymap.json` in Kitab's data folder listing every action, and opens it. Your bindings go on top of the defaults, Zed-style. `//` comments are allowed, and `null` removes a key:" },
          { code: '[\n  {\n    "context": "Reader",\n    "bindings": {\n      "ctrl-k": "FindNext",\n      "n": null\n    }\n  }\n]' },
          { p: "Saved changes apply when you switch back to Kitab. Mistakes show in a message and keep the keys you had." },
        ],
      },
      {
        id: "keyboard-navigation",
        title: "Moving between controls",
        blocks: [
          { p: "[[Tab]] and [[Shift+Tab]] move through the buttons and controls (title bar, tabs, dock, Home, Settings, dialogs) with an accent ring on the focused one; [[Enter]] or [[Space]] presses it. In a menu, [[Up]] and [[Down]] pick a row and [[Enter]] runs it. The ring shows only once you press [[Tab]], and a click hands the keys back to the page, so the reader's one-key shortcuts keep working." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Settings
  {
    id: "settings",
    title: "Settings",
    icon: "palette",
    summary: "Every section of Settings, and what each choice does.",
    topics: [
      {
        id: "settings-open",
        title: "Opening and searching Settings",
        blocks: [
          { p: "Press [[Ctrl+,]], or choose Settings in the ⋯ menu or on Home. Type in **Search settings** above the sections to list the matching settings; pick one, or press [[Enter]], to open its section, where it glows briefly. Settings are saved to `settings.json` in Kitab's data folder." },
        ],
      },
      {
        id: "settings-appearance",
        title: "Appearance",
        blocks: [
          S("settings-appearance", "Settings, Appearance: the Layout presets Full and Minimal with small previews, and the start of the Theme cards"),
          {
            table: {
              head: ["Setting", "Choices"],
              rows: [
                ["Layout", "Full (everything, with motion and depth) or Minimal (fewer controls, no motion, faster: flat surfaces, a compact dock, the page counter, sidebar closed). Change any part and it becomes Custom"],
                ["Theme", "Dark, Light, Paper, Ink (true black for OLED screens), Nord, Noctua, Vampire and Gray. [[Ctrl+Shift+L]] moves to the next theme"],
                ["Accent", "Theme, Amber, Rose, Blue, Emerald, Grape, Wine, Mono, or your own with a hex field and a hue strip. Kitab adjusts the lightness so the accent stays visible on every theme"],
                ["Density", "Dense (most on screen), Compact (the default) or Standard (roomier controls)"],
                ["Interface size", "90 % to 200 % of your system's scaling. Text, buttons, icons and pages grow together, at once"],
                ["Messages", "Where notes like \"Saved\" appear: top or bottom, on the left, in the centre or on the right. Top centre is the default"],
                ["Tabs", "Along the top (like a browser) or in the sidebar (full titles, more page height); what the + button opens; whether to show how many tabs are open"],
                ["Quick dock", "Top, bottom, left or right; Compact dock; Fade the dock while reading"],
                ["Toolbar", "For each of Sidebar, Back and forward, Zoom, Fit width and fit page, Find, Bookmark, Draw and mark up, Rotate, Focus mode, Print and Password: Title bar, Dock or Off. More (⋯) sits in the title bar or the dock. Open file… shows or hides. A small window previews your choices"],
                ["Motion", "Animations (menus, dialogs, panels and lists fade and slide), and Flat surfaces (shadows become hairlines)"],
              ],
            },
          },
          { p: "Try the themes in the Kitab window below. It is Kitab rebuilt in HTML, opened on Settings › Appearance:" },
          { mock: "appearance" },
        ],
      },
      {
        id: "settings-reading",
        title: "Reading",
        blocks: [
          S("settings-reading", "Settings, Reading: the Page navigator styles Scrubber, Stepper, Counter and Hidden, each with a small preview"),
          {
            table: {
              head: ["Setting", "Choices"],
              rows: [
                ["Page navigator", "Scrubber, Stepper, Counter or Hidden. Click the page number in any style, or press [[Ctrl+G]], to type a page"],
                ["Page corners", "Square, Soft or Round"],
                ["Scroll bar", "Show the scroll bar along the right edge of the pages"],
                ["Markdown typeface", "System, Serif or Mono"],
                ["Code colours", "Kitab, GitHub, One, Solarized, Dracula, Monokai or Nord, each card with a preview"],
                ["Books", "Typeface, line spacing and margins for EPUB, MOBI, FB2 and text (see [Typography](#book-typography))"],
                ["Status bar", "A switch for the bar and for each item (see [The status bar](#status-bar))"],
                ["Drawing and markup", "Show the markup tools"],
                ["Focus mode", "What [[F11]] hides: the tabs, the sidebar, the quick dock, the page navigator"],
              ],
            },
          },
        ],
      },
      {
        id: "settings-home",
        title: "Home",
        blocks: [
          S("settings-home", "Settings, Home: a small Home preview that follows the choices, and the Layout cards Covers and List"),
          { p: "Layout (Covers or List), Cover size (Small, Medium or Large), and above the books: Reading stats and Continue reading. A small Home at the top follows every choice." },
        ],
      },
      {
        id: "settings-library",
        title: "Library",
        blocks: [{ p: "Watched folders: Add folder…, Look again now, and Remove… on each folder. See [Watched folders](#watched-folders)." }],
      },
      {
        id: "settings-file-types",
        title: "File types",
        blocks: [
          S("settings-file-types", "Settings, File types: PDF document, EPUB book and Markdown document marked Default, and MOBI book with a Set up button"),
          { p: "Shows whether Kitab is the default app for PDF, EPUB, Markdown, MOBI, FB2, CBZ, XPS and text, with Set up on each type that isn't (Make default where another app has it), Set up all, and Refresh. It checks again when you come back to Kitab." },
          {
            list: [
              "**Windows:** Set up registers the type for your account, then Windows asks you to pick Kitab in Default apps (Open Default apps takes you there). Thumbnails shows whether File Explorer shows Kitab's page previews of PDFs, books and Markdown; it never takes over another reader's thumbnails.",
              "**macOS:** Set up makes Kitab the default app for that type in Finder. Set up all does every one but plain text.",
              "**Linux:** Set up adds Kitab to your apps, with file-manager thumbnails, when no package did, and makes it the default through `xdg-mime`.",
              "**Flatpak:** choose Kitab in your system's Default Applications settings, or with Open With in your file manager.",
            ],
          },
          { p: "**Remove Kitab's file types** takes Kitab off the Open with lists and stops its thumbnails, for your account. On macOS it hands PDFs, EPUBs and text back to Preview, Books and TextEdit. Your files are not touched." },
        ],
      },
      {
        id: "settings-shortcuts",
        title: "Shortcuts",
        blocks: [
          S("settings-shortcuts", "Settings, Shortcuts: Your own keys with an Edit keymap.json button, then the Files and tabs shortcuts with each key drawn as a key cap"),
          { p: "Every keyboard and mouse shortcut, and Edit keymap.json for your own. [[Ctrl+/]] opens this list from anywhere. See [Keyboard shortcuts](#keyboard)." },
        ],
      },
      {
        id: "settings-signing",
        title: "Signing",
        blocks: [{ p: "Trusted certificates, with Add certificate… and Remove. See [Trusted certificates](#trusted-certificates)." }],
      },
      {
        id: "settings-advanced",
        title: "Advanced",
        blocks: [
          S("settings-advanced", "Settings, Advanced: Clear cached pages, the Welcome to Kitab guide with an Open button, and Restore default settings"),
          {
            table: {
              head: ["Setting", "What it does"],
              rows: [
                ["Clear cached pages", "Frees the memory held by rendered pages, fonts and images. Pages render again as you read"],
                ["Welcome guide", "Opens Welcome to Kitab in a tab"],
                ["Restore default settings", "Puts every setting back the way Kitab comes, after asking. Your library, marks, bookmarks and open tabs stay"],
                ["Clear reading history", "Forgets Recent files, reading positions and closed tabs, after asking. Your marks, bookmarks and open tabs stay"],
                ["Optimize the database", "Compacts the file that holds your library, marks and bookmarks, and shows its size before and after"],
                ["Data folder", "Show opens Kitab's data folder"],
              ],
            },
          },
        ],
      },
      {
        id: "settings-about",
        title: "About",
        blocks: [
          S("settings-about", "Settings, About: Kitab Reader version 0.3.4, Check for updates with a Check now button, and Check on start-up switched on"),
          { p: "Your version, Check for updates (Check now), Check on start-up, and links to the source code, What's new (the changelog), Report a problem, the privacy policy, the licence and third-party notices." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── Updates & privacy
  {
    id: "privacy",
    title: "Updates and privacy",
    icon: "monitor",
    summary: "The one network request Kitab makes on its own, and where your data lives.",
    topics: [
      {
        id: "updates",
        title: "Updates",
        blocks: [
          { p: "Ten seconds after Kitab opens, and every six hours while it stays open, it asks GitHub's public API (`api.github.com/repos/t21dev/kitab/releases/latest`) for the latest release number and its notes. The request carries nothing about you or your files; GitHub sees your IP address, as with any web request." },
          { p: "When a newer release is out, an Update button appears in the title bar. It shows the release notes with Later, Skip this version and Download. Download opens the release page in your browser: Kitab never downloads or installs an update by itself." },
          { p: "Turn the check off in Settings › About › Check on start-up. Check now still works when you press it. In the Flatpak, Kitab doesn't check at all: Flatpak brings the updates." },
        ],
      },
      {
        id: "no-telemetry",
        title: "No telemetry",
        blocks: [
          {
            list: [
              "Your files stay on your computer. Kitab reads documents from your disk and never uploads them.",
              "There is no account, no analytics and no telemetry. The update check is the only request Kitab makes on its own.",
              "If Kitab crashes, a report is written to the `crash` folder in its data folder, and goes nowhere unless you send it yourself.",
              "Signatures are checked on your computer. Signing keys and passwords never leave it.",
              "Links you click open in your browser. Remote images in Markdown are blocked.",
            ],
          },
          { p: "The full policy is on the [privacy page](/privacy)." },
        ],
      },
      {
        id: "data-folder",
        title: "Where your data lives",
        blocks: [
          { p: "Kitab keeps your library list, reading positions, tabs, bookmarks, highlights and notes, settings (`settings.json`), your keys (`keymap.json`), a cache of page thumbnails and covers, a search index of your books' text, how long you read each day, and the certificates you trust for signatures (`trusted-certs/`), all in one folder. Delete it and all of it is gone." },
          {
            table: {
              head: ["System", "Data folder"],
              rows: [
                ["Windows", "`%APPDATA%\\t21\\Kitab`"],
                ["macOS", "`~/Library/Application Support/dev.t21.kitab`"],
                ["Linux", "`$XDG_DATA_HOME/kitab` (usually `~/.local/share/kitab`)"],
              ],
            },
          },
          { p: "Settings › Advanced › Data folder › Show opens it." },
        ],
      },
      {
        id: "portable",
        title: "Portable mode",
        blocks: [
          { p: "The Windows portable zip keeps Kitab's data in a `data` folder next to `kitab.exe` instead, so it can live on a USB stick. It does this because of the file named `portable` beside the app: any copy of Kitab with that file next to it works the same way." },
        ],
      },
    ],
  },

  // ─────────────────────────────────────────────────────────────── FAQ
  {
    id: "faq",
    title: "Questions",
    icon: "more",
    summary: "Answers to what people ask most.",
    topics: FAQ.map((f) => ({ id: `faq-${slug(f.q).split("-").slice(0, 6).join("-")}`, title: f.q, blocks: [{ p: f.a }] })),
  },
];

/** Inline markup to plain text, for search and snippets. */
export function plain(s: string): string {
  return s
    .replace(/\[\[([^\]]+)\]\]/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/\\\\/g, "\\");
}

/** Inline markup to Markdown, for the /docs.md mirror. */
export function md(s: string): string {
  return s.replace(/\[\[([^\]]+)\]\]/g, "`$1`").replace(/\]\(#/g, "](https://kitab.t21.dev/docs#");
}
