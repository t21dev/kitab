// SPDX-License-Identifier: AGPL-3.0-or-later
// All site copy lives here. The React pages, the Markdown mirrors (/index.md …),
// llms.txt and llms-full.txt are generated from this one file, so they never
// disagree. Rules (DESIGN.md § Principles): plain sentences, sentence case, no
// invented numbers. Every figure on the site must be a measured one with a source.

import { KEY_GROUPS } from "./app-keys";

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
  status: "Released for Windows, macOS and Linux; still in development toward 1.0.0.",
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
      "Fill PDF forms right on the page, and no document scripts ever run. Open password-protected files of every standard kind.",
      "Check whether a signature is valid and whether the file changed after signing, or sign it yourself with a certificate that never leaves your computer. Merge, split, compress and watermark without uploading anything.",
    ],
    points: ["Mark up any page with a pen, a highlighter, lines, arrows, boxes, text and notes. In a PDF they save into the file, so other readers show them too."],
    phase: 6,
  },
  {
    id: "privacy",
    title: "Your files stay where they are",
    body: [
      "There is no telemetry and no account. The only request Kitab makes on its own is the update check, which asks GitHub for the latest version number and nothing else, and one switch in Settings turns it off. Your library, highlights and settings live in one folder. Delete it and they are gone.",
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
  { what: "Forms", value: "Fill in and save, without running document scripts" },
  { what: "Passwords", value: "Every standard encryption type, with permissions explained" },
  { what: "Signatures", value: "Verify and sign, with keys that stay on your computer" },
  { what: "PDF tools", value: "Merge, split, rotate, compress, number pages, watermark" },
  { what: "Platforms", value: "Windows, macOS and Linux" },
  { what: "Licence", value: "AGPL-3.0, free and open source" },
];

export type FaqItem = { q: string; a: string };

export const FAQ: FaqItem[] = [
  {
    q: "Can I download Kitab Reader now?",
    a: "Yes. Every release is on GitHub Releases for Windows, macOS and Linux: an installer or portable zip for Windows, a .dmg for macOS, and a Flatpak, AppImage, .deb, .rpm or .tar.gz for Linux. Homebrew and Scoop users can install it from the t21dev tap and bucket.",
  },
  {
    q: "Is it free?",
    a: "Yes. Kitab is free and open source under the GNU AGPL-3.0-or-later licence. There is no paid tier and no account.",
  },
  {
    q: "What does the AGPL licence mean for me?",
    a: "You can use Kitab for anything, at home or at work, for free, and read and change its source. If you share a changed version of Kitab, or let people use it over a network, you have to share your changes under the same licence. Kitab is built on MuPDF by Artifex, which is under the AGPL too.",
  },
  {
    q: "What is it built with?",
    a: "Kitab is written in Rust. Its interface is drawn on the GPU with GPUI, and MuPDF reads and renders the documents. There is no browser engine inside, which keeps it small and quick to start.",
  },
  {
    q: "Which file formats does it open?",
    a: "PDF, EPUB and MOBI without DRM, FB2, XPS, CBZ comics, plain text and Markdown. Kitab is a document reader, so image files stay with your image viewer. DjVu, CBR and Kindle KF8 (AZW3) aren't supported yet.",
  },
  {
    q: "Windows says it protected my PC, or macOS won't open Kitab. Is it safe?",
    a: "The downloads aren't code-signed yet, so your system warns about them. On Windows, choose More info, then Run anyway. On macOS, right-click Kitab in Applications and choose Open, the first time only. Every release lists the SHA-256 checksum of each file, so you can check a download first, and the source is public.",
  },
  {
    q: "Does Kitab send my documents or usage data anywhere?",
    a: "No. There is no telemetry and no account. Kitab reads files from your disk and never uploads them. The only request it makes on its own is the update check, which asks GitHub for the latest version number and sends nothing about you or your files. Settings › About › Check on start-up turns it off.",
  },
  {
    q: "Where does Kitab keep my library, marks and settings?",
    a: "In one data folder: %APPDATA%\\t21\\Kitab on Windows, ~/Library/Application Support/dev.t21.kitab on macOS, and ~/.local/share/kitab on Linux. Settings › Advanced › Data folder opens it. Delete it and everything Kitab stored is gone. Marks you saved into a PDF stay in that PDF.",
  },
  {
    q: "Can I run Kitab without installing it?",
    a: "Yes, on Windows. Unzip the portable zip and run kitab.exe. It keeps its data in a data folder next to the app, because of the file named portable beside it, so it can live on a USB stick.",
  },
  {
    q: "How do I make Kitab my default PDF reader?",
    a: "Open Settings › File types. It shows whether Kitab opens PDF, EPUB, Markdown, MOBI, FB2, CBZ, XPS and text files, with Set up on each type and Set up all. On Windows, Windows then asks you to pick Kitab in Default apps. In the Flatpak, use your system's Default Applications settings.",
  },
  {
    q: "Do my highlights and drawings show in other PDF readers?",
    a: "Yes. In a PDF, Ctrl+S saves them into the file as standard annotations, after the file's original bytes, so other readers show them and signatures already in the file stay valid.",
  },
  {
    q: "Can it fill PDF forms and open password-protected PDFs?",
    a: "Yes. Click a field on the page to fill it in, and Ctrl+S saves the values into the file. Kitab never runs a document's scripts, so a form's own calculations don't run. It opens password-protected PDFs, can add, change or remove a password on a copy, and never stores a password.",
  },
  {
    q: "Can I change the keyboard shortcuts?",
    a: "Yes. Settings › Shortcuts › Edit keymap.json creates a file listing every action. Add your own keys there, or set one to null to remove it, and they apply when you switch back to Kitab.",
  },
  {
    q: "How is the speed measured?",
    a: "A benchmark tool starts Kitab and watches its window until the first page is drawn and stable. It runs each scenario 15 times after two warm-up runs and reports the median. Results are published with each release.",
  },
  {
    q: "Which operating systems are supported?",
    a: "Windows 10 and 11, macOS on Apple silicon and Intel, and Linux on x86_64 and aarch64, from the same code. Windows on Arm runs the x64 build. WinGet and Flathub listings are planned for version 1.0.",
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

/** Every default shortcut, generated from the app's own list (crates/kitab-ui/src/shortcuts.rs,
 * via `npm run sync` into app-keys.ts). Rows are [keys, action]. */
export const SHORTCUTS: ShortcutGroup[] = KEY_GROUPS.map((g) => ({ area: g.title, keys: g.rows.map(([what, keys]) => [keys, what]) }));

export type Platform = { os: string; formats: string; when: string };

export const PLATFORMS: Platform[] = [
  { os: "Windows 10 and 11", formats: "Installer (.exe) for your account, or a portable zip. x64; Windows on Arm runs the x64 build", when: "Every release" },
  { os: "macOS", formats: "DMG for Apple silicon and for Intel", when: "Every release" },
  { os: "Linux", formats: "Flatpak, AppImage, deb, rpm and tar.gz for x86_64; all but the Flatpak also for aarch64", when: "Every release" },
  { os: "Package managers", formats: "brew install --cask t21dev/tap/kitab · scoop install t21dev/kitab", when: "Every release" },
  { os: "WinGet and Flathub", formats: "Listings in the WinGet and Flathub catalogues", when: "Planned for 1.0" },
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
    path: "/docs",
    file: "docs.html",
    md: "docs.md",
    title: "Kitab Reader documentation",
    description:
      "How to use Kitab Reader: install it, read PDFs, books and Markdown, find, mark up, fill forms, sign, print, page tools, every setting and every keyboard shortcut. Searchable.",
    changefreq: "weekly",
    priority: 0.9,
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
