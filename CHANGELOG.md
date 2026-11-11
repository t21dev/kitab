# Changelog

All notable changes to Kitab Reader are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.8] - 2026-11-11

### Fixed
- Markdown editor: a click puts the caret where you click, and dragging selects text again. Since 0.3.6 every click landed near the top of the file.
- Markdown editor: long lines have a scroll bar along the bottom (drag it, or click the track), and the file has one down the right edge. Shift+wheel and sideways swipes scroll sideways. The line numbers stay put while the text scrolls, and a drag selection keeps going when the pointer leaves the editor.
- While you edit Markdown, the dock and the page navigator sit over the preview, so they no longer cover the source.

## [0.3.7] - 2026-11-11

### Added
- Kitab Reader is on the Microsoft Store. The Store version sets up its file types and File Explorer thumbnails when it installs, and leaves updates to the Store, so it doesn't check GitHub for them. Settings › File types and Settings › About say so.

### Fixed
- Memory no longer grows with every tab. A tab out of sight keeps only the pages it showed: the one you just left stays sharp for a minute, so going back and forth is instant, and older ones keep a small soft copy that shows at once and sharpens as the page draws again. Nineteen open PDFs, most of them large scans, went from about 2 GB to about 230 MB in a maximised window (850 MB to 165 MB at 1400×900).
- MuPDF's cache of decoded images and fonts is trimmed after every piece of work, not only after drawing a page, so a big artbook left in a background tab no longer holds hundreds of megabytes.
- Closing a tab frees its pages' graphics memory too.
- With many tabs open, the window's close button no longer slides off the edge of the title bar. The tabs fill the room the bar has left, show whole tabs only (no half-hidden tab after Home), and the gap kept for dragging the window is smaller.

## [0.3.6] - 2026-11-09

### Fixed
- Markdown tables: columns get room by what they hold, text wraps inside its cell instead of running past the table or into the next column, and a short word never breaks. A table too wide even so scrolls sideways.
- Long text no longer spills out of its box anywhere: text fields (notes, form fields, search, signing) slide to keep the caret in view, menu items and collection names end in "…", long tooltips wrap, a long signer name stays in the status bar, and the update notes' tables wrap.
- The Markdown editor scrolls sideways for long lines and follows the caret, so nothing is cut off at the edge.
- Wide formulas and a details block's long summary stay inside the reading column; many printers scroll in the Print sheet; very deep contents lists stop indenting past the sidebar.

## [0.3.5] - 2026-11-09

### Added
- Discard, beside Save in the dock: lets go of every unsaved mark, deletion and form value, after asking. The file stays as it is.
- Filled-in form fields can be undone and redone (Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z), like marks. Undoing a radio button turns the one that was on back on.

### Changed
- With Select, choosing a colour or a thickness changes the picked mark (one undo step), not only what you draw next.

## [0.3.4] - 2026-11-09

### Added
- A Select tool, first on the markup bar: drag a drawing, shape, highlight, text box or note to move it (Shift keeps it level or upright), Delete removes the picked one, Esc lets it go. Ctrl+Z undoes a move.
- The hidden dock's handle shows how many changes aren't saved yet.
- Settings › Reading › Status bar › File path: the status bar can show where the file is (your home folder as ~).

### Changed
- The update notes show tables and code blocks properly (the checksums too), each scrolling sideways in its own box instead of breaking the dialog. Numbered lists and quotes read as they should.
- Right-clicking selected text gives a menu for the selection (Copy, Find, Highlight, Underline, Strike through), without back, forward and zoom.

### Fixed
- The dock no longer fades away while a menu it opened (such as Markdown's typography panel) is still showing.
- Default size in Markdown's typography panel lines up with the rows above it.

## [0.3.3] - 2026-11-09

### Changed
- Dialogs (Print, Password, Sign, Page tools, Save page, Extract, naming a collection, the update notes) leave the way they came, fading and sinking, instead of vanishing. The command palette fades in and out quickly, footnotes, the note writer and a form list's choices open with a short drop, and switching Settings sections or sidebar panes brings the new one in with a gentle rise.
- Picking a setting from Settings search scrolls to it and makes it glow briefly; each section opens at the top.

### Fixed
- Settings › About › View update did nothing: the update notes opened under the Settings window.
- The update notes no longer give the new version's release date to the one you have.

## [0.3.2] - 2026-11-09

### Added
- An Open file… button in the title bar, beside More. Settings › Appearance › Toolbar shows or hides it.
- Settings › Appearance › Tabs › The + button: a new tab shows Home (as before) or opens the Open dialog straight away.
- Markdown's dock has a typography button: a small panel to pick the typeface (System, Serif, Mono) and change the text size, which stays open while you try them.
- Fill in PDF forms: click a checkbox to tick it, a radio button to choose it, a list to pick from its choices, and a text field to type in it, right on the page at the field's size. Tab moves to the next text field. Fillable fields have a light tint. Save (Ctrl+S) writes the values into the file after its original bytes, so a signature stays valid and reads "form filled in since signing". A field shown on several pages fills in on all of them; read-only and signature fields can't be typed into.

### Changed
- Hiding the dock slides it away toward its edge before the handle appears, and showing it slides it back the same way (the return was too short to notice before).
- Closing a tab or the window with a form filled in but not saved asks first, as it does for unsaved Markdown edits.
- Removing a watched folder (Settings › Library) asks first, and takes the documents found only in that folder out of your library. Documents you've opened, favourites and documents in a collection stay; nothing on disk is touched.

## [0.3.1] - 2026-11-08

### Added
- Select text in Markdown: drag across paragraphs, headings, lists, tables and code; double-click a word, triple-click a paragraph or a line of code, Shift+click to extend. Ctrl+C copies, and the page menu offers Copy and Find.
- Code blocks wider than the page have a scroll bar: drag it, or click along it to jump. Shift+wheel scrolls a block sideways.

### Changed
- Markdown bookmarks are named after their section ("Lists"), not a page number, and the right-click menu in Markdown keeps to what applies there: back and forward, zoom, Bookmark this section, Print… and the file.

### Fixed
- In Markdown, keyboard shortcuts such as Ctrl+D (bookmark) and Ctrl+C (copy) did nothing after clicking in the text: the click didn't give the document the keyboard.
- Long lines in a Markdown code block couldn't be scrolled to; they were cut off at the edge.
- The mouse wheel over a code block scrolled the block sideways as well as the page; it scrolls only the page now.

## [0.3.0] - 2026-11-08

### Added
- Print Markdown: Ctrl+P (or Print… in the ⋯ menu) lays the document out on A4, with headings, lists, tables, quotes and code, and prints it like any other document.
- Settings › Appearance › Messages: put notes like "Saved" or "Couldn't print" at the top or bottom, on the left, in the centre or on the right. Top centre is the new default.
- Markdown's typeface: System (the default), Serif or Mono, from the new font button in the dock or Settings › Reading.
- Settings › Appearance › Interface size: 90 % to 200 % of your system's scaling (100 % is the default and looks as before). Text, buttons, icons and pages grow together, at once. In a narrow window Home's Settings and Open file… become icon buttons.
- Settings › Home: show the books as Covers or a List (a small cover, title, author, format and progress on each row), choose the cover size, and turn the reading stats and Continue reading on or off. A small Home at the top of the page follows every choice.
- Search in Settings: type in the box above the sections to list the matching settings; pick one (or press Enter) to open its section.

### Changed
- Markdown reads in your system's font by default instead of a serif.
- A code block's Copy is an icon (a check for a moment after copying).
- Covers on Home load four at a time and more of them are kept, so scrolling a big library no longer flashes empty cards. Covers that arrive quickly appear at once and only ones drawn for the first time fade in; cards scrolled straight past aren't drawn, and a loading cover's place is a quiet block instead of a generated cover.
- Kitab uses next to no CPU while you read or leave it open: it used to redraw its window on every screen refresh even when nothing changed (about 2 % of a core). Scrolling costs about a third of what it did.
- Less memory while reading: fewer pages far from the one you're on are kept (those on screen and the next three always are), and MuPDF's cache of fonts and images is kept to 48 MB.
- The tab count sits at the right of the title bar. The + isn't shown when nothing is open.
- Home's pinned header has room below the shelf chips and a thin line once the books scroll beneath it. Its search box is the same height as the buttons beside it.
- Markdown follows the chosen page navigator (Stepper moves a screen at a time; Counter shows how far through you are).

### Fixed
- Printing failed with "Couldn't print: page 1: it didn't draw" once you'd zoomed or scrolled. Pages always print now.
- Windows' print dialog said "Printing from Win32 application"; it says "Kitab - Print", and Windows names Kitab properly elsewhere too (Task Manager, Open with).
- In focus mode, the top of Home's Settings and Open file… buttons didn't respond: the window buttons' strip lay over them. Home now starts below it.
- Print (Ctrl+P, or Print… in the ⋯ menu) crashed Kitab on Windows. Windows' print dialog now opens on the window as it should.
- Kitab remembers where its window was and how big, and whether it was maximised, and opens there next time. A place on a screen that's no longer connected is ignored.
- On Home, every book card is the same height: a long title takes two lines and ends in "…" (the whole title shows on hover), and the author line keeps its place.
- Hovering the hidden dock's handle, and other floating buttons and cards, no longer turns them into a grey glow: their fill stays solid.
- The window opens on the monitor it was last on, at the same place and size, also on a monitor with different scaling.
- In Settings › Appearance › Toolbar, choosing Dock (or Title bar) for Focus mode, Print and other rows works; the click used to land on another row.
- Markdown's percentage showed 0 % while the slider moved; it now follows the scroll position.
- Favourite and Mark finished show on Recent at once.
- The text cursor no longer shows over the dock or the page navigator.

## [0.2.0] - 2026-11-08

### Added
- Downloads for every common system: macOS on Intel, and on Linux a Flatpak, an AppImage, a .deb and an .rpm beside the .tar.gz, for x86_64 and aarch64 (the Flatpak for x86_64). The release notes list each file's SHA-256 checksum.
- Homebrew and Scoop: `brew install --cask t21dev/tap/kitab` and `scoop install t21dev/kitab`.
- Printing on macOS and Linux uses the system's own print dialog, with the printer, copies, pages and paper: macOS's print panel as a sheet on the window, and on Linux the desktop's print portal (GNOME, KDE, and inside Flatpak). Linux without a print portal keeps Kitab's own sheet.
- Middle-click scrolling in documents and on Home, as in a browser: click the wheel and move the pointer above or below the mark to scroll (faster further away); any click ends it. Holding the wheel while dragging scrolls only while it's held.
- An open-tab count beside the tabs (Settings › Appearance › Tabs; off by default).
- Settings › Appearance › Toolbar shows a small window that follows the choices: which buttons sit in the title bar, which in the dock, where the dock is, and whether it's compact or fades.
- A scroll bar along the right of the pages: drag it, or click beside the thumb to jump. Settings › Reading › Scroll bar turns it off.
- Cover, a markup tool for a solid box over something you want hidden on the page. It's saved into a PDF as a filled square annotation, so other readers show it too. Only a cover: the text underneath stays in the file, and the tool's tooltip says so.
- A welcome guide: a new Home offers "Welcome to Kitab", a four-page PDF about finding your way, marking up, themes and focus mode. Settings › Advanced opens it again.
- Favourites wear a red ribbon on their cover on Home.
- Settings › Advanced › Restore default settings (asks first; your library, marks and tabs stay).
- Focus mode keeps two quiet buttons in the bottom-right corner: the sidebar (contents and bookmarks; Ctrl+B works too) and Exit focus mode.
- Tabs slide in when they open, and a closed tab's place narrows away instead of the others jumping.
- Icons beside the reading stats on Home.

### Changed
- Home is built for big libraries: the title, search and shelf chips stay put while the books scroll beneath them, only the rows on screen are drawn, and a scroll bar runs down the right. Books show their own first page as the cover (drawn in the background for the cards on screen, kept on disk so each is drawn once), and a cover lifts a little under the pointer.
- With 400 documents in a watched folder Home uses about 96 MB, scrolling through them about 127 MB, and once you've been reading for half a minute the covers are let go (about 137 MB with a document open, down from over 200). `tools/bench_library.py` repeats the measurement.
- Adding a watched folder says it's looking, and then how many documents it found; Look again now says so even when there's nothing new. Home shows "Looking in your folders…" meanwhile.
- When the library's documents come only from watched folders (nothing opened yet), Home opens on All instead of saying there are no books.
- The + in the title bar opens Home (Open file… and your recent books) instead of going straight to the Open dialog; Ctrl+O still does.
- Home's Settings button matches Open file…, with an icon and a word.
- About › Links: each row is the link, with an arrow, instead of an Open button.
- In the Flatpak, Kitab doesn't check for updates (Flatpak brings them, and the sandbox has no network), and Settings › File types points to the system's Default Applications instead of setting them itself. A second launch opens its files as tabs in the running Kitab there too.
- Hovering a tab, button or row shows a clear highlight in every theme (it was nearly invisible in the dark ones).

### Fixed
- Kitab opens on computers whose graphics can't run it: when Direct3D 11 fails on the first graphics adapter (an old GPU or driver, some virtual machines and remote desktops), it tries the others and then Windows' software renderer, instead of failing to start.
- Ctrl+P prints again (and the keys for Password, Sign and Page tools work outside presentation mode). On Windows, Print opens Windows' own print dialog straight away, with the printer, copies and which pages (all, the current page, or ranges like 1-3, 7); the pages print shrunk to fit the paper and turned to match it.
- A code block's Copy button says Copied, with a tick, for a moment after it copies.
- The window can be dragged by its title bar again, and minimise, maximise and close work: a click there was being swallowed before Windows saw it.
- The page slider moves smoothly with the pointer, also in short documents (with two pages it used to stay put until halfway).
- Pages no longer flicker while scrolling in a large window or on a high-DPI screen: the rendered-page cache grows to hold what's on screen, the next page is ready before it scrolls in, and tabs in the background keep their pages.
- File types shows Kitab as the default app when it is: newer Windows 11 keeps the choice somewhere Kitab didn't look. The page also checks again when you come back to Kitab.
- The pointer over the markup bar is an ordinary arrow, not the drawing cross.
- Toasts show above Settings and dialogs instead of under their shade.

## [0.1.0] - 2026-11-08

### Added
- Keyboard navigation: Tab and Shift+Tab move through the buttons and controls (title bar, tabs, dock, Home, Settings, dialogs) with an accent ring on the focused one; Enter or Space presses it. In a menu, Up and Down pick a row and Enter runs it. The ring shows only once you press Tab, and a click hands the keys back to the page, so the reader's one-key shortcuts keep working.
- Gray theme: plain neutral greys on #191919 (Settings › Appearance, or the ⋯ menu).
- Windows downloads: an installer for your account (no administrator prompt) and a portable zip. kitab.exe carries its own C runtime, so a clean Windows needs nothing else (no Visual C++ Redistributable). The uninstaller takes Kitab's file types off with `kitab --unregister`. macOS gets a .dmg, Linux a .tar.gz with install.sh.
- A footnote mark's tooltip says Show the note instead of the page it points at.
- Shelves on Home: Recent, All, Favourites, In progress, Finished, and your own collections (a document can be in several, so they work as tags). Right-click a book for Favourite, Mark finished, a tick per collection and New collection…; right-click a collection to rename or delete it. Settings › Library watches folders: their documents join the library (Home › All) without being opened, found at start-up and on Look again now.
- Library search: the search box on Home finds words across every document you've opened (indexed in the background a few seconds after each opens, again when the file changes; prefix matches, accents ignored, Bangla and Arabic marks kept with their letters). Results show the page and the words around the match; clicking one opens the document at that page.
- Reading stats on Home: time read today and this week, the current and longest streak (a day counts with a minute of reading), and a bar per day for four weeks. Time counts while you're active in a document (input in the last two minutes), stored per day in Kitab's library.
- Page tools… (⋯ menu, the page and tab menus, or the palette): delete, move or turn pages, compress, add page numbers or a watermark, or save the text or the pictures. Each writes a new file and reports what it did (compress says how much smaller). Numbers and watermarks sit level on turned pages too.
- Sign… (⋯ menu or the palette) signs a PDF with a certificate file (.pfx or .p12; ECDSA P-256/P-384 or RSA keys): choose the file and type its password, add a reason and place if you like, click on a page where the signature goes, pick where the signed copy is saved, and confirm a summary of the signer, page and file. The signature is appended as an incremental update, so signatures already in the file stay valid; other verifiers (pyhanko) find Kitab's signatures intact, valid and trusted. Encrypted PDFs aren't signed yet (save a copy without the password first).
- Signatures are verified when a PDF opens, on your computer and off the UI thread: the signed bytes against the signed digest, the signature against the signer's certificate (ECDSA P-256/P-384, RSA PKCS #1 v1.5 and PSS; SHA-1 to SHA-512), the certificate chain against the certificates you trust, the certificate's validity at signing time, how much of the file is covered, and what changed in later revisions (form filling, annotations, or other changes). The status bar shows the result (valid, signer not trusted, changed after signing, broken); clicking it opens the new Signatures pane in the sidebar, one card per signature with its signer, time, reason, place and findings in plain words. Settings › Signing lists the trusted certificates, with Add certificate… and Remove.
- Password (the lock in the title bar, ⋯ › Password…, or the palette): put a password on a PDF, change it, or take it off. Kitab saves a new copy (AES-256) and leaves the open file alone. A PDF whose owner restricts it keeps its password unless it was opened with the owner password. The password fields show dots and refuse copy and cut, and the password is never stored.
- Print (Ctrl+P, ⋯ › Print…, the palette, or a toolbar button): choose all pages, this page or a range, fit to paper, actual size or shrink to fit, and whether pages turn to match the paper; then the system print dialog picks the printer and copies. Pages are drawn at the printer's resolution (up to 300 dpi) in the background, with progress in a toast. PDFs that forbid printing say so. On macOS and Linux the sheet also lists the printers CUPS knows (the default chosen) and takes the copies; the pages go to the printer as a PDF of vector pages.
- `kitab --print <file> [--printer NAME] [--to FILE]` prints without a window, and Explorer's right-click Print uses it once Kitab is set up for the file type. On macOS and Linux it prints through CUPS, and `--to` keeps the PDF it would have sent.
- Delete annotations a PDF already has: the bin on a row of the Marks pane, or the eraser on the page. They are outlined in red until you save (Ctrl+S or the dock's Save, which counts them), when they are taken out of the file incrementally; the arrow on the row keeps one after all.
- Toolbar buttons in the title bar: Find, Draw and More (⋯) now sit beside the window buttons, and the dock keeps paging and zoom. Settings › Appearance › Toolbar puts each button in the title bar, the dock, or nowhere (More is always in one of the two). They act on the open tab and stay put however many tabs are open; in a narrow window only More stays up top and the rest go back to the dock.
- A status bar along the bottom of the page replaces the floating file info chip: format, file size, pages (lines in Markdown), word count and reading time for Markdown, and paper or text size on the left; your position (page 3 of 40, or how far through) and zoom on the right. Each item has a switch in Settings › Reading › Status bar, and Ctrl+I shows or hides the bar.
- Footnotes open beside their mark: clicking a note number (1, [2], *, †) in a book or PDF shows the note in a small card instead of jumping away; Go to note jumps (Alt+Left comes back), Esc or a click elsewhere closes it. Other links still jump.
- Book typography (Settings › Reading › Books): typeface (the book's own, serif, sans or mono), line spacing and margins for EPUB, MOBI, FB2 and text files, applied over the book's own styles. Pages and the contents' page numbers follow at once, at the same place in the book.
- Mermaid diagrams in Markdown are drawn: flowcharts, sequence, class, state, entity-relationship, Gantt, pie, mindmap, timeline and git graphs, in the theme's colours, sharp at any zoom. A diagram that can't be read shows its source and why. Drawn off the UI thread in pure Rust (no browser or JavaScript).
- Math in Markdown: `$$…$$` and ```math blocks are typeset (fractions, roots, sub- and superscripts, sums and integrals with limits, matrices and cases, Greek and symbols); inline `$…$` reads as one line of Unicode (πr², x₁ ≤ α). TeX that can't be read shows its source with a mark.
- Markdown live reload: when the Markdown file you're reading changes on disk (saved from another editor), Kitab shows the new version in place within a second, keeping your position. Only the tab on screen is checked; not while you edit in Kitab.
- Presentation mode (F5, or the palette): the document full screen on black, one page at a time fitted to the screen. Right, Down, Space, PgDn, a click or the wheel go forward; Left, Up, PgUp or a right click go back; Home and End jump; Esc or F5 ends it on the page you were showing. The next page is drawn ahead.
- keymap.json: your own keys on top of the defaults, Zed-style (`[{ "context": "Reader", "bindings": { "ctrl-k": "FindNext", "n": null } }]`, `//` comments allowed, null removes a key). Edit keymap.json (Settings › Shortcuts or the palette) creates one listing every action and opens it; saved changes apply when you switch back to Kitab. Mistakes show in a toast and keep the keys you had. The palette shows the keys in force.
- Command palette (Ctrl+K or Ctrl+Shift+P, or ⋯ › Command palette): every command by name with its keys; type a few letters ("gtp" finds Go to page), Up and Down to choose, Enter to run. Commands that need a document hide on Home.
- Settings › About: version, Check for updates (asks GitHub for the latest release; on start-up and every six hours, which Check on start-up turns off), and links to the source, changelog, issues, privacy policy, licence and notices. A newer release shows an Update button in the title bar with its notes, Later and Download (opens the release page).
- Settings › File types: whether Kitab is the default for PDF, EPUB, Markdown, MOBI, FB2, CBZ, XPS and text, with Set up on each type that isn't set up (Make default where another app has it), Set up all, Open Default apps, Explorer thumbnails status, and Remove to take Kitab's file types off your account. On Linux, Set up adds Kitab to your apps (with file-manager thumbnails) when no package did, and makes it the default through xdg-mime; Remove takes it out of mimeapps.list. On macOS, Set up makes Kitab.app the default through Launch Services, and Remove hands PDFs, EPUBs and text back to Preview, Books and TextEdit.
- Settings › Advanced: clear cached pages, clear reading history (keeps marks, bookmarks and open tabs; asks first), optimize the library database (shows the size before and after), and show the data folder.
- Choose the quick dock's buttons (Settings › Appearance › Dock items): sidebar, back and forward, zoom, fit, find, bookmark, draw, rotate and minimal mode. More and the hide chevron always stay.
- A Pages pane in the sidebar: thumbnails at each page's true shape, the current page ringed, bookmarked pages marked; only the visible ones render, and a click goes to the page.
- The file info chip folds to a small i (its chevron, or Ctrl+I) and a click on the i unfolds it, remembered across launches.
- Bookmarks: Ctrl+D (or the page's right-click menu) bookmarks the page, named after its chapter, and again removes it. Bookmarked pages carry a ribbon, and the sidebar's Bookmarks pane lists them. Re-flowing books don't take bookmarks yet.
- A Marks pane in the sidebar (the highlighter icon beside Contents): every mark and every annotation already in the PDF, in page order, with the words a highlight covers and a note's text; a click goes to the page, and Kitab's own marks can be deleted there. Export them to Markdown or CSV.
- Hide the quick dock to its edge (the chevron at its end, or Ctrl+.): it becomes a small handle, and a click on the handle or Ctrl+. brings it back. Remembered across launches.
- Highlight, underline or strike through selected text (H, or the right-click menu), and add notes and text boxes with the new Text and Note tools: notes are pins whose text shows when you point at them, and a click edits them. Typing works with input methods (Bangla, Arabic, CJK). In a PDF they save as Highlight, Underline, StrikeOut, Text and FreeText annotations.
- A file info chip at the bottom left: format, file size, pages and paper size (a book shows its text size, Markdown its lines and words); point at it for the file's location. Settings › Reading › File info hides it.
- Draw and mark up pages (Ctrl+Shift+D, or the pen in the dock): pen, highlighter, line, arrow, rectangle, ellipse and eraser, with six colours plus a picker of 84 more and any hex code (#RRGGBB or #RGB), three widths, undo and redo, and Shift for 45° lines, squares and circles. Marks are kept as you draw; in a PDF, Save in the dock (Ctrl+S) writes them into the file as standard annotations without touching the original bytes, so signatures stay valid. Comics, XPS and fixed-layout books keep their marks in Kitab. Settings › Reading › Drawing and markup turns the tools off.
- Fixed-layout EPUBs show one page per spread at their own size, like a PDF, and don't re-flow. In right-to-left books (Arabic, Hebrew, manga) the left arrow goes to the next page.
- Books (EPUB, MOBI, FB2, TXT) fill the window: pages re-flow when the window settles at a new size, and zoom changes the text size (8–32 pt, remembered; Ctrl+0 resets it). The reader keeps its place in the chapter.
- Kitab opens EPUB, MOBI, FB2, CBZ comics, XPS and plain-text files, with their contents lists, search and thumbnails. Books in Bangla, Arabic, Hebrew, Chinese, Japanese, Korean and other scripts use the system's fonts. Kindle KF8 (.azw3) books aren't supported yet, and image files are not opened.
- Tabs show an icon for the file's format: book, comic, text or page.
- Windows Explorer shows Kitab's thumbnails for PDF and Markdown once Kitab is the default app for them (`kitab_shell.dll`, registered with Settings › Reading › Default app). It never takes over another reader's thumbnails.
- File-manager thumbnails: `kitab --thumbnail` draws page 1 of a PDF, or a small page preview of a Markdown or text file (headings, lists, code, tables). Linux file managers use it through `dev.t21.kitab.thumbnailer`.
- Settings › Reading › Code colours picks the syntax palette for Markdown code blocks: Kitab (follows the theme), GitHub, One and Solarized (each with a light and a dark variant that follow the theme), Dracula, Monokai and Nord. Each card shows a small preview.
- Kitab has its app icon (a bookmark ribbon on amber) and document icons that name the format (PDF, Markdown, and EPUB, MOBI, FB2, CBZ, XPS, TXT for later). Settings › Reading › Default app registers Kitab for PDF and Markdown and opens Default apps; macOS and Linux packages declare the same types.
- Edit Markdown in place (Edit, or Ctrl+E): the source beside a live preview, with undo/redo, clipboard, input methods, list continuation, line numbers and Ctrl+S to save. Closing with unsaved changes asks first.
- Code blocks in Markdown have syntax colours that follow the theme (Rust, JavaScript and TypeScript, Python, Go, C and C++, C#, Java, shell, SQL, YAML, JSON, HTML, CSS and more).
- The sidebar has its own hide button in its header.
- Markdown files open in Kitab: GitHub-style tables, task lists, footnotes, code blocks with Copy, quotes, collapsible details, frontmatter shown as a properties card, local images, heading outline in the sidebar, links between files and to headings, find, zoom and session restore. Unsafe HTML, scripts and remote images are blocked.
- Arabic and Hebrew text now reads right to left in the interface and in Markdown (a fix in Kitab's copy of the UI toolkit).
- Opening a file while Kitab is running (double-clicking a PDF, say) adds a tab to the open window and brings it forward, instead of starting a second Kitab.
- A + button after the tabs opens the system Open dialog (Ctrl+O).
- The Settings window's left corners are properly rounded.
- Layout presets in Settings › Appearance: Full, or Minimal (no motion, flat surfaces, a compact dock with only zoom, find and ⋯, the page counter, sidebar closed) for a leaner, faster window. Change any part and it becomes Custom.
- Kitab remembers whether the sidebar was open (Ctrl+B, F12 or the dock's sidebar button) the next time it starts.
- Smoother scrolling through long and large PDFs: pages of one document render on several threads at once, a page shows a quick soft render while its sharp one draws (when that is faster), the next page in the scroll direction renders ahead, and pages you scroll past are no longer rendered after the fact.
- The header row is replaced by a quick dock floating over the page (sidebar, back/forward, zoom, fit, find, rotate, ⋯). Put it at the top, bottom (default, together with the page navigator), left or right in Settings › Appearance, and let it fade while you read.
- Tabs can be listed in the sidebar instead of along the top (Settings › Appearance › Tabs), with full titles and the page you're on.
- Minimal mode (F11): hide the tabs, sidebar, dock and page navigator for distraction-free reading; choose which in Settings › Reading. Point at the top edge for the window buttons and a way back.
- Find in document (Ctrl+F or /): hits highlighted on the pages, "3 of 41", F3 / Enter and Shift+F3 / Shift+Enter to step, match case (Alt+C) and whole word (Alt+W).
- Select text on the page by dragging, double-clicking a word or triple-clicking a line, and copy it with Ctrl+C or the right-click menu, which also offers Find for the selection. Copying follows the document's copy permission.
- Links in PDFs work: page links jump (Alt+Left comes back), web and mail links open in the browser after showing the address on hover. Other kinds of links are ignored.
- Many tabs: tabs shrink to share the bar, then the bar scrolls, the active tab stays in view, and an All tabs button lists them all.
- The page menu is shorter: back/forward, zoom and rotate are one row of icon buttons, and one "Save page as…" entry opens a dialog with the page, the format (PDF, image or text) and the image resolution.
- Long menus scroll instead of running off short windows.
- Save one page as a PDF, a PNG image (150 dpi) or plain text, from the page's right-click menu.
- Save pages as a new PDF (Ctrl+Shift+E): type pages and ranges such as 1-3, 7, 10-; works on password-protected PDFs while they are open.
- Merge PDFs: pick more files and save them, in order, after the open one as a new PDF.
- Split view: open a second document beside the first (Ctrl+\, "Open to the side" on a tab or Home card); click a pane to make it the active one.
- A toast reports saved files, with Open or Show in folder.
- Ink, a true-black theme for OLED screens.
- Accent presets (Theme, Amber, Rose, Blue, Emerald, Grape, Wine, Mono) and a custom accent with a hex field and a hue strip; every accent is adjusted per theme to stay visible.
- Settings › Shortcuts lists every keyboard and mouse shortcut (Ctrl+/).
- Right-click a page to copy its text, copy it as an image (150 dpi PNG) or copy its page number.
- Settings (Ctrl+,) with Appearance and Reading sections and live previews for themes, density, page navigator and page corners, saved to `settings.json`.
- Page navigator styles (Scrubber, Stepper, Counter, Hidden); the page number is a field you can type into (or press Ctrl+G); the scrubber track can be clicked and dragged.
- Optional rounded page corners (Square, Soft, Round).
- The ⋯ menu, and right-click menus for pages, tabs, Home cards and contents entries (copy file path, show in folder, close others, and more).
- Keyboard and mouse: Alt+←/→ and the mouse back/forward buttons for history, Ctrl+wheel zoom, Alt+1…9 tabs, Ctrl+PgUp/PgDn, Ctrl+F4, Ctrl+Shift+T reopen closed tab, F12 contents, middle-click to close a tab.
- Opening and closing motion for menus, Settings, the sidebar and its rows, tabs and Home cards, with a setting to turn it off.
- The reader opens PDFs through MuPDF, remembers each book's place, lists recent books on Home and restores the last session's tabs.
- Theme files for Dark, Light, Paper, Nord, Noctua and Vampire (`assets/themes/`), checked against DESIGN.md's contrast rules in tests.
- Project plan, design system and contributor rules (`docs/PLAN.md`, `DESIGN.md`, `AGENTS.md`).
- Test corpus: 17 PDF, 11 EPUB and 10 Markdown fixtures generated by `tests/fixtures/tools/generate_fixtures.py`, an `expected.json` oracle checked with MuPDF and pyHanko, and six real-world files fetched with pinned hashes.

### Fixed
- Books (EPUB, MOBI, FB2, text) read in the theme's colours: the page is the window's own surface and the text its reading colour, in every theme; pictures keep theirs. Fixed-layout books, comics, XPS and PDF stay on white paper.
- F11 is now Focus mode everywhere (menu, palette, dock, Settings), so it no longer shares a name with the Minimal layout preset.
- The signature card names what changed ("Comments added after signing" or "Changed after signing"), says the signed version is intact instead of seeming to contradict itself, labels Reason and Place, and offers Trusted certificates… whenever the signer isn't trusted.
- The update dialog's button says Skip this version, which is what it does.
- Vampire's error colour is orange, no longer nearly the accent; Light's search box has a visible edge; back and forward in the dock are plainly off when there's nowhere to go; the password sheet waits until both fields are filled before saying they don't match.
- The window appears finished: the tab and dock no longer slide in after it shows (they did for about 0.2 s, redrawing the window 30 times).
- Less memory: the next page is rendered ahead only once you start scrolling, pages kept in memory are capped lower, and after 20 s in the background, tabs you aren't looking at give back their rendered pages and MuPDF trims its cache. Opening a PDF without signatures no longer reads the whole file a second time to look for them.
- Opening no longer shows a stand-in window before the real one. The window is created off screen (DWM cloak), draws its first page, and then appears in one piece; if a page is slow it appears after 0.7 s anyway. Opening your last session doesn't flash Home first.
- Opening a document renders each visible page once (it was two or three times, plus a throwaway preview): the window's first page costs about 25 % less CPU and 20 MB less memory.
- Find highlights no longer hide the matched words in the Light and Paper themes.
- The password prompt of a locked PDF takes typing again (letters that are reader shortcuts, like j and k, were swallowed), focuses itself, and has an Open button.
- The dock fits the reading area at ordinary window sizes instead of scrolling: back and forward, rotate, fit and finally the sidebar button step aside as it narrows (their keys and the palette still work), and the page track shortens.
- The find bar sits below the markup bar when both are open, and the markup bar never runs past the reading area.
- The window can't be made smaller than 640 × 460, where the title bar, Home and Settings stop fitting.
- Error messages stay until closed, wrap to show the whole reason, and have a close button.
- Clicking beside a dialog you type into (Password, Sign, Print, Page tools, Save pages, collection name) no longer closes it and loses what you typed; Esc and Cancel still do.
- Opening a PDF and a Markdown file together no longer leaves the start-up splash on screen: a Markdown tab and Home close it, and it never stays more than about three seconds.
- Small windows no longer look broken: the dock stays inside the reading area and scrolls along its length (a mouse wheel over it scrolls it), the sidebar narrows to give the page room, and the window buttons always stay on screen.
- Fit width and fit page follow a window resize even when no new page has to be drawn.
- The Settings window shrinks to fit small windows instead of running off the left edge.

[Unreleased]: https://github.com/t21dev/kitab/compare/v0.3.7...HEAD
[0.3.7]: https://github.com/t21dev/kitab/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/t21dev/kitab/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/t21dev/kitab/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/t21dev/kitab/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/t21dev/kitab/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/t21dev/kitab/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/t21dev/kitab/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/t21dev/kitab/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/t21dev/kitab/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/t21dev/kitab/releases/tag/v0.1.0
