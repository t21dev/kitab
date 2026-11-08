// SPDX-License-Identifier: AGPL-3.0-or-later
// All site copy lives here. The React pages, the Markdown mirrors (/index.md …),
// llms.txt and llms-full.txt are generated from this one file, so they never
// disagree. Rules (DESIGN.md § Principles): plain sentences, sentence case, no
// invented numbers. Every figure on the site must be a measured one with a source.

export const SITE = {
  name: "Kitab Reader",
  shortName: "Kitab",
  url: "https://kitab.t21.dev",
  tagline: "Your library has a fast lane now.",
  subtitle: "Rust underneath, a calm page on top. Kitab never makes you wait.",
  description:
    "Kitab Reader is a beautiful, modern PDF reader built with an eye for performance: native, drawn on the GPU, open in a blink and idle at 0 % CPU. Free and open source, for PDF, EPUB, Markdown and comics, on Windows, macOS and Linux. Your files stay on your computer.",
  repo: "https://github.com/t21dev/kitab",
  releases: "https://github.com/t21dev/kitab/releases",
  plan: "https://github.com/t21dev/kitab/blob/main/docs/PLAN.md",
  design: "https://github.com/t21dev/kitab/blob/main/DESIGN.md",
  issues: "https://github.com/t21dev/kitab/issues",
  org: { name: "Team 21", short: "t21", url: "https://t21.dev", github: "https://github.com/t21dev", email: "contact@t21.dev" },
  author: { name: "TriptoAfsin", github: "https://github.com/TriptoAfsin" },
  license: { name: "AGPL-3.0-or-later", url: "https://www.gnu.org/licenses/agpl-3.0.html" },
  status: "In development. There is no release to download yet.",
  phase: "Phase 10, polish, accessibility and distribution before 1.0.0",
  /** The hero badge. The release in it comes from GitHub at build time when GitHub answers. */
  latest: "0.2",
  latestNote: "printing, signatures and a faster reader",
  locale: "en",
} as const;

/** A feature in the story. The stage beside it shows Kitab doing it (src/mock/scenes.ts). */
export type Step = {
  id: string;
  title: string;
  body: string[];
  points?: string[];
  phase: number;
};

/** The feature story, in reading order. Copy follows mockup B; layout follows mockup A. */
export const STEPS: Step[] = [
  {
    id: "pages",
    title: "Pages keep their own size",
    body: [
      "A landscape chart sits between portrait pages instead of being squeezed to one width. A4, A5, Letter and receipt-sized pages each keep their shape at one zoom, and print at their own orientation.",
    ],
    phase: 0,
  },
  {
    id: "speed",
    title: "Fast is a feature",
    body: [
      "A reader should be open before you have finished double-clicking. Kitab shows the first page as soon as its window appears, and stays smooth with dozens of tabs open and while you resize the window.",
      "Speed is tested, not assumed. A benchmark starts Kitab and watches its window until the page is drawn and stable, fifteen times per scenario.",
    ],
    points: ["Results are published with each release, along with the machine they ran on."],
    phase: 1,
  },
  {
    id: "tabs",
    title: "One window for everything you read",
    body: [
      "Open as many documents as you like in tabs. Close Kitab and open it again: every tab comes back on the same page, at the same zoom. Home shows what you were reading and where you stopped.",
    ],
    points: ["PDF, EPUB, MOBI, FB2, XPS, CBZ, Markdown and text", "Reopen closed tabs, split view, and a command palette (Ctrl+K)"],
    phase: 2,
  },
  {
    id: "find",
    title: "Find works the same everywhere",
    body: [
      "Press Ctrl+F in a PDF, a book or a Markdown file. Every hit is highlighted on the page, with match case, whole word and accents ignored.",
    ],
    phase: 3,
  },
  {
    id: "markdown",
    title: "Markdown, rendered properly",
    body: [
      "Markdown renders like GitHub, with Mermaid diagrams drawn in your theme's colours, highlighted code and typeset math, and reloads when you save the file. Books reflow in your choice of typeface, line spacing and margins.",
    ],
    phase: 5,
  },
  {
    id: "forms",
    title: "Fill it, sign it, print it",
    body: [
      "Fill PDF forms, with totals and dates worked out the way the form expects, and no document scripts ever run. Open password-protected files of every standard kind.",
      "Check whether a signature is valid and whether the file changed after signing, or sign it yourself with a certificate that never leaves your computer. Merge, split, compress, watermark and redact without uploading anything.",
    ],
    points: ["Mark up any page with a pen, a highlighter, lines, arrows, boxes, text and notes. In a PDF they save into the file, so other readers show them too."],
    phase: 6,
  },
  {
    id: "privacy",
    title: "Your files stay where they are",
    body: [
      "There is no telemetry and no account. Kitab makes no network request until you turn something on, like the update check, and each setting says so where you turn it on. Your library, highlights and settings live in one folder. Delete it and they are gone.",
    ],
    phase: 2,
  },
];

export const FORMATS = ["PDF", "EPUB", "MOBI", "FB2", "XPS", "CBZ", "Markdown", "TXT"];

export type Spec = { what: string; value: string };

/** What Kitab is made of, stated on its own terms. */
export const SPECS: Spec[] = [
  { what: "Engine", value: "MuPDF, for fast, accurate rendering of PDF, EPUB and more" },
  { what: "Interface", value: "Native, drawn on the GPU (DirectX, Metal, Vulkan). No browser engine inside" },
  { what: "Formats", value: "PDF, EPUB, MOBI, FB2, XPS, CBZ, Markdown and text" },
  { what: "Markdown", value: "GitHub-style, with Mermaid diagrams, highlighted code and math" },
  { what: "Library", value: "Home, collections, tags and full-text search" },
  { what: "Reading memory", value: "Page, zoom, rotation and layout per file, and the whole session" },
  { what: "Annotations", value: "Highlights in colours, notes, underline, strike and ink, with export" },
  { what: "Forms", value: "Fill, save and export data; totals and dates worked out" },
  { what: "Passwords", value: "Every standard encryption type, with permissions explained" },
  { what: "Signatures", value: "Verify and sign, with keys that stay on your computer" },
  { what: "PDF tools", value: "Merge, split, rotate, compress, watermark, redact" },
  { what: "Platforms", value: "Windows, macOS and Linux" },
  { what: "Licence", value: "AGPL-3.0, free and open source" },
];

export type FaqItem = { q: string; a: string };

export const FAQ: FaqItem[] = [
  {
    q: "Can I download Kitab Reader now?",
    a: "Not yet. Kitab is in development and has no release. Releases will appear on GitHub Releases for Windows, macOS and Linux. Watch the repository to hear when the first one lands.",
  },
  {
    q: "Is it free?",
    a: "Yes. Kitab is free and open source under the GNU AGPL-3.0-or-later licence. There is no paid tier and no account.",
  },
  {
    q: "What is it built with?",
    a: "Kitab is written in Rust. Its interface is drawn on the GPU with GPUI, and MuPDF reads and renders the documents. There is no browser engine inside, which keeps it small and quick to start.",
  },
  {
    q: "Which file formats does it open?",
    a: "PDF, EPUB and MOBI without DRM, FB2, XPS, CBZ comics, plain text and Markdown. Kitab is a document reader, so image files stay with your image viewer. DjVu, CBR and Kindle KF8 (AZW3) are planned after version 1.0.",
  },
  {
    q: "Does Kitab send my documents or usage data anywhere?",
    a: "No. There is no telemetry. Kitab reads files from your disk and never uploads them. It makes a network request only when you turn on a feature that needs one, such as the update check or online certificate revocation checks.",
  },
  {
    q: "How is the speed measured?",
    a: "A benchmark tool starts Kitab and watches its window until the first page is drawn and stable. It runs each scenario 15 times after two warm-up runs and reports the median. Results are published with each release.",
  },
  {
    q: "Can it fill PDF forms and open password-protected PDFs?",
    a: "Yes. Kitab fills every kind of form field and works out totals, currency and dates the way the form expects, without running any script inside the PDF. You can save the filled copy or export the data. It opens password-protected PDFs of every standard encryption type, and never stores a password unless you ask it to.",
  },
  {
    q: "Which operating systems are supported?",
    a: "Windows, macOS and Linux, from the same code: an installer, MSI and portable zip on Windows, a DMG on macOS, and AppImage, deb, rpm and Flatpak on Linux. From the first release it also installs with Homebrew, Scoop and winget.",
  },
  {
    q: "Who makes it?",
    a: "Team 21 (t21), an independent software studio, and TriptoAfsin. The code and the plan are public on GitHub.",
  },
];

/** From docs/PLAN.md §8. `release` is the release a phase's work shipped in (CHANGELOG.md), not the plan's
 * target version: the work of phases 1 to 9 landed together in 0.1.0, and native print dialogs in 0.2.0. */
export type Phase = { n: number; title: string; release?: string; summary: string; state: "done" | "now" | "next" };

export const ROADMAP: Phase[] = [
  { n: 0, title: "Foundations and spikes", summary: "Workspace, CI, and the first spike: a GPU-drawn window showing real PDF pages, with start-up timed.", state: "done" },
  { n: 1, title: "PDF reader core", release: "0.1.0", summary: "Open (including password-protected files), progressive rendering, true page sizes, zoom, keyboard navigation, position memory.", state: "done" },
  { n: 2, title: "Shell, Home and Settings", release: "0.1.0", summary: "Tabs, session restore, Home, the themes, command palette, keymap.json.", state: "done" },
  { n: 3, title: "Find and navigation", release: "0.1.0", summary: "Find bar, outline, thumbnails, bookmarks, history, presentation mode.", state: "done" },
  { n: 4, title: "Books and other formats", release: "0.1.0", summary: "EPUB reflow, MOBI, FB2, CBZ, XPS, text, typography, footnotes.", state: "done" },
  { n: 5, title: "Markdown", release: "0.1.0", summary: "GitHub-flavoured Markdown, code, Mermaid, math, live reload.", state: "done" },
  { n: 6, title: "Annotations", release: "0.1.0", summary: "Highlights, notes, ink and shapes saved into the PDF, the Marks pane, export to Markdown or CSV.", state: "done" },
  { n: 7, title: "Native print", release: "0.1.0, 0.2.0", summary: "Print on all three systems, with each system's own print dialog since 0.2.0.", state: "done" },
  { n: 8, title: "Signatures", release: "0.1.0", summary: "Verify and sign PDFs, locally.", state: "done" },
  { n: 9, title: "PDF tools and library search", release: "0.1.0", summary: "Merge, split, compress, full-text library search, collections, reading stats.", state: "done" },
  { n: 10, title: "Polish and distribution", summary: "Accessibility, the full published benchmark, signed installers, winget and Flathub, then 1.0.0.", state: "now" },
];

export type ShortcutGroup = { area: string; keys: [string, string][] };

/** From docs/PLAN.md §3.10 on main. On macOS, Ctrl is Cmd. */
export const SHORTCUTS: ShortcutGroup[] = [
  {
    area: "Files and tabs",
    keys: [
      ["Ctrl+O", "Open a file"],
      ["Ctrl+W", "Close tab"],
      ["Ctrl+Shift+T", "Reopen closed tab"],
      ["Ctrl+Tab / Ctrl+Shift+Tab", "Next / previous tab"],
      ["Ctrl+1 … Ctrl+8, Ctrl+9", "Go to tab n, last tab"],
      ["Ctrl+Shift+PgUp / PgDn", "Move tab"],
      ["Ctrl+Shift+H", "Home"],
      ["Ctrl+N", "New window"],
      ["Ctrl+P", "Print"],
      ["Ctrl+S", "Save annotated copy"],
    ],
  },
  {
    area: "Navigate",
    keys: [
      ["← / →, PgUp / PgDn", "Previous / next page"],
      ["Space / Shift+Space", "Scroll down / up a screen"],
      ["Home / End", "First / last page"],
      ["n / p", "Next / previous page"],
      ["j / k", "Scroll down / up"],
      ["Ctrl+G", "Go to page"],
      ["Alt+← / Alt+→", "Back / forward in history"],
      ["F12", "Show or hide contents"],
    ],
  },
  {
    area: "Find",
    keys: [
      ["Ctrl+F or /", "Find"],
      ["F3 or Enter", "Next match"],
      ["Shift+F3 or Shift+Enter", "Previous match"],
      ["Alt+C", "Match case"],
      ["Alt+W", "Whole word"],
      ["Esc", "Close find"],
    ],
  },
  {
    area: "Zoom and view",
    keys: [
      ["Ctrl+= / Ctrl+−", "Zoom in / out"],
      ["Ctrl+scroll, pinch", "Zoom"],
      ["Ctrl+0", "Fit page"],
      ["Ctrl+1", "Actual size"],
      ["Ctrl+2", "Fit width"],
      ["Ctrl+6 / Ctrl+7 / Ctrl+8", "Single page / two pages / book view"],
      ["c", "Continuous scrolling"],
      ["Ctrl+Shift+− / Ctrl+Shift+=", "Rotate left / right"],
    ],
  },
  {
    area: "Read",
    keys: [
      ["F11", "Focus mode"],
      ["F5", "Presentation"],
      ["Ctrl+Shift+L", "Next theme"],
      ["Ctrl+B", "Show or hide the sidebar"],
      ["Ctrl+Alt+= / Ctrl+Alt+−", "Larger / smaller text (books)"],
    ],
  },
  {
    area: "Annotate",
    keys: [
      ["h", "Highlight the selection"],
      ["Ctrl+Alt+N", "Add a note"],
      ["Ctrl+D", "Bookmark"],
    ],
  },
  {
    area: "App",
    keys: [
      ["Ctrl+K", "Command palette"],
      ["Ctrl+,", "Settings"],
      ["?", "Shortcut cheat sheet"],
      ["Ctrl+Shift+F12", "Performance overlay"],
    ],
  },
];

export type Platform = { os: string; formats: string; when: string };

export const PLATFORMS: Platform[] = [
  { os: "Windows", formats: "Installer (.exe), MSI and a portable zip, for x64 and Arm64", when: "At release" },
  { os: "macOS", formats: "DMG for Apple silicon and Intel", when: "At release" },
  { os: "Linux", formats: "AppImage, deb, rpm and Flatpak", when: "At release" },
  { os: "Package managers", formats: "brew install --cask t21dev/tap/kitab · scoop install t21dev/kitab · winget install t21dev.Kitab", when: "From the first release" },
];

export type Route = {
  path: string;
  file: string; // output HTML path inside dist/
  md: string; // Markdown mirror path
  title: string;
  description: string;
  changefreq: "weekly" | "monthly";
  priority: number;
};

export const ROUTES: Route[] = [
  {
    path: "/",
    file: "index.html",
    md: "index.md",
    title: "Kitab Reader: your library has a fast lane now",
    description:
      "Rust underneath, a calm page on top. Kitab is a free, open source reader for PDF, EPUB, Markdown and comics, drawn on the GPU with GPUI, with tabs, split view, find, markup and no telemetry.",
    changefreq: "weekly",
    priority: 1,
  },
  {
    path: "/download",
    file: "download.html",
    md: "download.md",
    title: "Download Kitab Reader",
    description:
      "Kitab Reader release status and downloads for Windows, macOS and Linux. Installers, portable zip, DMG, AppImage, deb, rpm and Flatpak, from GitHub Releases.",
    changefreq: "weekly",
    priority: 0.8,
  },
  {
    path: "/shortcuts",
    file: "shortcuts.html",
    md: "shortcuts.md",
    title: "Kitab Reader keyboard shortcuts",
    description:
      "Every default keyboard shortcut in Kitab Reader: tabs, navigation, find, zoom, reading, annotations. Every one can be rebound.",
    changefreq: "monthly",
    priority: 0.6,
  },
  {
    path: "/changelog",
    file: "changelog.html",
    md: "changelog.md",
    title: "Kitab Reader changelog",
    description: "What changed in each Kitab Reader version, following Keep a Changelog and Semantic Versioning.",
    changefreq: "weekly",
    priority: 0.5,
  },
  {
    path: "/privacy",
    file: "privacy.html",
    md: "privacy.md",
    title: "Kitab Reader privacy",
    description:
      "What Kitab Reader and this website do with your data: no telemetry, no network unless you turn it on, keys stay on your computer.",
    changefreq: "monthly",
    priority: 0.5,
  },
];
