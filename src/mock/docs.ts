// SPDX-License-Identifier: AGPL-3.0-or-later
// What the mock windows have open. The PDF guide is the app's own "Welcome to
// Kitab" (assets/sample/welcome.html on main); the books are public-domain
// openings; the Markdown file describes Kitab's real render paths.

export type Block =
  | { t: "kicker" | "h1" | "h2" | "h3" | "p" | "lede" | "foot" | "label"; s: string }
  | { t: "card"; items: [string, string][] }
  | { t: "table"; rows: [string, string][]; script?: boolean }
  | { t: "chart" }
  | { t: "lines"; rows: [string, string][] };

/** A fixed-layout page: its size in points and what is printed on it. */
export type Page = { w: number; h: number; paper: string; blocks: Block[]; pad?: number; small?: boolean };
export type OutlineRow = { title: string; page: number; depth: number };

export type DocKind = "pdf" | "book" | "md";
export type Doc = {
  id: string;
  kind: DocKind;
  title: string;
  icon: "file" | "book" | "markdown";
  format: string;
  author?: string;
  progress: number;
  favourite?: boolean;
  /** pdf */
  pages?: Page[];
  size?: string;
  outline?: OutlineRow[];
  /** book */
  chapter?: string;
  text?: string[];
  bookPages?: number;
};

const A4: [number, number] = [595, 842];

const WELCOME: Page[] = [
  {
    w: A4[0], h: A4[1], paper: "A4",
    blocks: [
      { t: "kicker", s: "Kitab Reader · a short guide" },
      { t: "h1", s: "Welcome to Kitab" },
      { t: "lede", s: "A reader for your PDFs and books that opens fast, stays out of the way, and keeps your files on your computer." },
      { t: "h2", s: "Open something" },
      { t: "p", s: "Drop a file onto the window, press Ctrl+O, or use Open file… on Home. Each document gets its own tab, and Kitab remembers the page you were on, so closing a book never loses your place. Home lists what you read lately, with how far through each one you are." },
      {
        t: "card",
        items: [
          ["PDF and XPS", "Forms, passwords, signatures and annotations work."],
          ["EPUB, MOBI, FB2, TXT", "Laid out as pages in your theme's colours."],
          ["Comics (CBZ)", "One page at a time, full width."],
          ["Markdown", "With code colours, math and diagrams, and an editor beside it."],
        ],
      },
      { t: "h3", s: "One window, every format" },
      { t: "p", s: "Tabs along the top, contents and bookmarks on the left, the page in the middle, and the dock at the bottom for paging and zoom." },
    ],
  },
  {
    w: A4[0], h: A4[1], paper: "A4",
    blocks: [
      { t: "kicker", s: "Getting around" },
      { t: "h2", s: "Find your way" },
      { t: "p", s: "The sidebar holds the book's contents, your bookmarks, the pages and the marks you've made. Click any entry to jump there; Alt+← takes you back to where you were. The dock at the bottom shows the page number: drag its slider to move through the book, or click the number and type a page." },
      { t: "h3", s: "Search" },
      { t: "p", s: "Ctrl+F finds words in the open document, and F3 steps through the matches. The search box on Home looks through every document you have opened, and a result opens at the page it found." },
      { t: "h3", s: "Keys worth knowing" },
      {
        t: "table",
        rows: [
          ["Next and previous page", "→ ←  or  N P"],
          ["Go to a page", "Ctrl+G"],
          ["Contents and bookmarks", "Ctrl+B"],
          ["Bookmark this page", "Ctrl+D"],
          ["Zoom in and out", "Ctrl+=  Ctrl+-"],
          ["Fit the page or the width", "Ctrl+0  Ctrl+2"],
          ["Reopen the tab you closed", "Ctrl+Shift+T"],
          ["Every command, by name", "Ctrl+K"],
        ],
      },
    ],
  },
  {
    w: A4[0], h: A4[1], paper: "A4",
    blocks: [
      { t: "kicker", s: "Making notes" },
      { t: "h2", s: "Mark it up" },
      { t: "p", s: "Press Ctrl+Shift+D for the markup tools: a pen, a highlighter, lines, arrows, boxes, text and sticky notes. Select some text and press H to highlight it. In a PDF, Ctrl+S saves your marks into the file as ordinary annotations, so any other reader shows them too." },
      { t: "p", s: "A highlight says: come back here. An arrow points at what matters, and a box keeps a part of the page together." },
      { t: "card", items: [["About covers.", "The Cover tool draws a solid box over something, on screen and in print. The words underneath are still in the file, so don't send a covered PDF to hide a secret."]] },
      { t: "h3", s: "Passwords, signatures and pages" },
      { t: "p", s: "The lock in the title bar puts a password on a copy of a PDF, or takes one off. Sign… in the ⋯ menu signs a PDF with your certificate, and Kitab checks the signatures already in a file when it opens. Page tools… deletes, moves and turns pages, compresses a file, and adds page numbers or a watermark. Each of these saves a new file and leaves the original alone." },
    ],
  },
  {
    w: A4[0], h: A4[1], paper: "A4",
    blocks: [
      { t: "kicker", s: "Make it yours" },
      { t: "h2", s: "Read your way" },
      { t: "p", s: "Kitab comes with several themes, from Paper to Ink; Ctrl+Shift+L moves to the next one, and Settings (Ctrl+,) holds the rest: the accent colour, how dense the interface is, where the tabs and dock sit, and the typeface and margins for books." },
      { t: "p", s: "F11 is focus mode: the tabs, sidebar and dock step aside and the page has the screen. F5 presents a document full screen, one page at a time." },
      { t: "h3", s: "Every script, the right way round" },
      {
        t: "table",
        script: true,
        rows: [
          ["Bangla", "বই পড়া আনন্দের।"],
          ["Hindi", "किताब पढ़ना अच्छा है।"],
          ["Arabic", "الكِتابُ خَيْرُ جَلِيسٍ"],
          ["Japanese", "本を読むのは楽しい。"],
        ],
      },
      { t: "h3", s: "Yours, and only yours" },
      { t: "p", s: "Kitab sends nothing about you or your files anywhere. The only time it goes online is a quick check for a new version, which you can turn off in Settings › About. Your library, reading positions and marks stay in a small database on this computer." },
      { t: "foot", s: "Kitab Reader is free software under the GNU AGPL 3.0, made by t21. Source, issues and new versions: github.com/t21dev/kitab." },
    ],
  },
];

const WELCOME_OUTLINE: OutlineRow[] = [
  { title: "Welcome to Kitab", page: 0, depth: 0 },
  { title: "Open something", page: 0, depth: 1 },
  { title: "Find your way", page: 1, depth: 0 },
  { title: "Search", page: 1, depth: 1 },
  { title: "Keys worth knowing", page: 1, depth: 1 },
  { title: "Mark it up", page: 2, depth: 0 },
  { title: "Passwords, signatures and pages", page: 2, depth: 1 },
  { title: "Read your way", page: 3, depth: 0 },
  { title: "Every script, the right way round", page: 3, depth: 1 },
  { title: "Yours, and only yours", page: 3, depth: 1 },
];

const SIZES: Page[] = [
  {
    w: 595, h: 842, paper: "A4",
    blocks: [
      { t: "label", s: "A4 portrait · 210 × 297 mm" },
      { t: "h1", s: "The north orchard" },
      { t: "p", s: "A survey of the old trees along the north wall, walked in the first week of autumn. Each row was counted twice, once going out and once coming back, and the two counts agreed." },
      { t: "p", s: "The chart on the next page is printed sideways on purpose. It keeps its own shape here instead of being squeezed to the width of this page." },
    ],
  },
  { w: 842, h: 595, paper: "A4", blocks: [{ t: "label", s: "A4 landscape · 297 × 210 mm" }, { t: "h2", s: "Rows walked, by month" }, { t: "chart" }] },
  {
    w: 420, h: 595, paper: "A5", small: true,
    blocks: [
      { t: "label", s: "A5 · 148 × 210 mm" },
      { t: "h2", s: "Field notes" },
      { t: "p", s: "Bring the long ladder for row six. The gate by the well sticks after rain; lift it before you push." },
      { t: "p", s: "The pear by the shed is older than the shed." },
    ],
  },
  {
    w: 612, h: 792, paper: "Letter",
    blocks: [
      { t: "label", s: "US Letter · 8.5 × 11 in" },
      { t: "p", s: "Dear Ms Rahman," },
      { t: "p", s: "Thank you for the survey of the north orchard. We would like to walk the rows with you before the frost, and to keep the oldest trees where they stand." },
      { t: "p", s: "With best wishes," },
    ],
  },
  {
    w: 227, h: 567, paper: "227 × 567 pt", small: true, pad: 18,
    blocks: [
      { t: "label", s: "Receipt · 80 mm roll" },
      { t: "h3", s: "Orchard supply" },
      { t: "lines", rows: [["Ladder hire", "1"], ["Twine, garden", "2"], ["Grafting wax", "1"], ["Gloves, pair", "3"]] },
      { t: "foot", s: "Thank you. Keep this receipt." },
    ],
  },
  {
    w: 595, h: 842, paper: "A4",
    blocks: [
      { t: "label", s: "A4 portrait · 210 × 297 mm" },
      { t: "h2", s: "Appendix" },
      { t: "p", s: "Every page in this file prints at its own size and the right way round." },
    ],
  },
];

const MD_SOURCE = `# How Kitab draws a page

Kitab picks a path by format. Fixed layouts go through MuPDF's page renderer, books are reflowed, and Markdown is laid out by Kitab itself. Every path ends in the same \`Reader\` view.

## The zoom label

\`\`\`rust
// A book shows its text size: zoom there changes the text, not the page.
fn zoom_label(book_pt: Option<f32>, percent: u32) -> String {
    match book_pt {
        Some(pt) => format!("{pt:.0} pt"),
        None => format!("{percent} %"),
    }
}
\`\`\`

## The paths

\`\`\`mermaid
flowchart LR
  A[Open file] --> B{Format?}
  B -->|PDF / XPS / CBZ| C[MuPDF page renderer]
  B -->|EPUB / MOBI / FB2| D[MuPDF reflow layout]
  B -->|Markdown| E[Native GPUI renderer]
  C --> F[Reader view]
  D --> F
  E --> F
\`\`\`

| Format | Path |
| --- | --- |
| PDF, XPS, CBZ | Page renderer |
| EPUB, MOBI, FB2 | Reflow layout |
| Markdown | Kitab's own layout |

- [x] Open the file
- [x] Draw the first page
- [ ] Read the rest

> Reading first, chrome quiet.
`;

export const MARKDOWN_SOURCE = MD_SOURCE;

/** The file's facts, from the source itself (the status bar shows them). */
export const MD_FACTS = (() => {
  const bytes = new TextEncoder().encode(MD_SOURCE).length;
  const lines = MD_SOURCE.trimEnd().split("\n").length;
  const words = MD_SOURCE.split(/\s+/).filter((w) => /[A-Za-z0-9]/.test(w)).length;
  return `Markdown  ·  ${(bytes / 1024).toFixed(1)} KB  ·  ${lines} lines  ·  ${words} words`;
})();

export const DOCS: Record<string, Doc> = {
  welcome: {
    id: "welcome", kind: "pdf", title: "Welcome to Kitab", icon: "file", format: "PDF", author: "t21",
    progress: 0.25, pages: WELCOME, size: "64.2 KB", outline: WELCOME_OUTLINE,
  },
  sizes: { id: "sizes", kind: "pdf", title: "Mixed page sizes", icon: "file", format: "PDF", progress: 0.1, pages: SIZES, size: "3.9 KB", outline: [] },
  notes: { id: "notes", kind: "md", title: "How Kitab draws a page", icon: "markdown", format: "MD", progress: 0.04 },
  pride: {
    id: "pride", kind: "book", title: "Pride and Prejudice", icon: "book", format: "EPUB", author: "Jane Austen", progress: 0.42, favourite: true,
    chapter: "Chapter 1", bookPages: 312,
    text: [
      "It is a truth universally acknowledged, that a single man in possession of a good fortune, must be in want of a wife.",
      "However little known the feelings or views of such a man may be on his first entering a neighbourhood, this truth is so well fixed in the minds of the surrounding families, that he is considered the rightful property of some one or other of their daughters.",
      "“My dear Mr. Bennet,” said his lady to him one day, “have you heard that Netherfield Park is let at last?”",
      "Mr. Bennet replied that he had not.",
    ],
  },
  odyssey: {
    id: "odyssey", kind: "book", title: "The Odyssey", icon: "book", format: "FB2", author: "Homer", progress: 0.17, chapter: "Book I", bookPages: 388,
    text: [
      "Tell me, O muse, of that ingenious hero who travelled far and wide after he had sacked the famous town of Troy.",
      "Many cities did he visit, and many were the nations with whose manners and customs he was acquainted; moreover he suffered much by sea while trying to save his own life and bring his men safely home; but do what he might he could not save his men, for they perished through their own sheer folly in eating the cattle of the Sun-god Hyperion; so the god prevented them from ever reaching home.",
    ],
  },
  meditations: {
    id: "meditations", kind: "book", title: "Meditations", icon: "book", format: "EPUB", author: "Marcus Aurelius", progress: 0.63, chapter: "Book One", bookPages: 196,
    text: [
      "From my grandfather Verus I learned good morals and the government of my temper.",
      "From the reputation and remembrance of my father, modesty and a manly character.",
      "From my mother, piety and beneficence, and abstinence, not only from evil deeds, but even from evil thoughts; and further, simplicity in my way of living, far removed from the habits of the rich.",
    ],
  },
  frankenstein: {
    id: "frankenstein", kind: "book", title: "Frankenstein", icon: "book", format: "MOBI", author: "Mary Shelley", progress: 1, chapter: "Letter 1", bookPages: 280,
    text: [
      "You will rejoice to hear that no disaster has accompanied the commencement of an enterprise which you have regarded with such evil forebodings.",
      "I arrived here yesterday, and my first task is to assure my dear sister of my welfare and increasing confidence in the success of my undertaking.",
    ],
  },
  moby: {
    id: "moby", kind: "book", title: "Moby-Dick; or, The Whale", icon: "book", format: "EPUB", author: "Herman Melville", progress: 0.08, favourite: true, chapter: "Chapter 1. Loomings", bookPages: 624,
    text: [
      "Call me Ishmael.",
      "Whenever I find myself growing grim about the mouth; whenever it is a damp, drizzly November in my soul; then, I account it high time to get to sea as soon as I can.",
      "This is my substitute for pistol and ball. There is nothing surprising in this. If they but knew it, almost all men in their degree, some time or other, cherish very nearly the same feelings towards the ocean with me.",
    ],
  },
};

/** Home's Recent shelf, newest first. */
export const RECENT = ["welcome", "pride", "notes", "meditations", "odyssey", "moby", "frankenstein", "sizes"];
