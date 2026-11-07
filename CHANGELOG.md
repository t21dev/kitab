# Changelog

All notable changes to Kitab Reader are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- Fit width and fit page follow a window resize even when no new page has to be drawn.
- The Settings window shrinks to fit small windows instead of running off the left edge.
