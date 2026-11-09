// SPDX-License-Identifier: AGPL-3.0-or-later
// The documents' own content: PDF pages as white sheets (sizes in points), the
// Markdown file as Kitab lays it out (DESIGN.md § Markdown reading), and books
// reflowed in the reading face. Find highlights hits in all three.
import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { KIcon } from "./atoms";
import type { Block, Doc, Page } from "./docs";

// ── Find ─────────────────────────────────────────────────────────────────

type Hl = { re: RegExp | null; cur: number; ctr: { i: number } };
const HlCtx = createContext<Hl>({ re: null, cur: -1, ctr: { i: 0 } });

export function findRegex(query: string, matchCase: boolean, wholeWord: boolean): RegExp | null {
  const q = query.trim();
  if (!q) return null;
  const esc = q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(wholeWord ? `\\b${esc}\\b` : esc, matchCase ? "g" : "gi");
}

export function countHits(strings: string[], re: RegExp | null): number {
  if (!re) return 0;
  return strings.reduce((n, s) => n + (s.match(re)?.length ?? 0), 0);
}

/** Hits are counted in render order, so the n-th hit on screen is hit n. */
export function HlScope({ re, cur, children }: { re: RegExp | null; cur: number; children: ReactNode }) {
  return <HlCtx.Provider value={{ re, cur, ctr: { i: 0 } }}>{children}</HlCtx.Provider>;
}

export function HL({ s }: { s: string }) {
  const { re, cur, ctr } = useContext(HlCtx);
  if (!re) return <>{s}</>;
  const out: ReactNode[] = [];
  let at = 0;
  for (const m of s.matchAll(re)) {
    const i = m.index ?? 0;
    if (i > at) out.push(s.slice(at, i));
    const n = ctr.i++;
    out.push(
      <mark key={i} className="k-hit" data-cur={n === cur || undefined}>
        {m[0]}
      </mark>,
    );
    at = i + m[0].length;
  }
  if (at < s.length) out.push(s.slice(at));
  return <>{out}</>;
}

/** Every string the renderers pass through HL, in the same order. */
export function searchStrings(doc: Doc): string[] {
  if (doc.kind === "md") return MD_STRINGS;
  if (doc.kind === "book") return [doc.chapter ?? "", ...(doc.text ?? [])];
  const out: string[] = [];
  for (const p of doc.pages ?? []) {
    for (const b of p.blocks) {
      if ("s" in b) out.push(b.s);
      else if (b.t === "card") b.items.forEach(([a, c]) => out.push(a, c));
      else if (b.t === "table" || b.t === "lines") b.rows.forEach(([a, c]) => out.push(a, c));
    }
  }
  return out;
}

// ── PDF pages ────────────────────────────────────────────────────────────

const SEED = "A highlight says: come back here.";

function PageBlock({ b, seed, onSeed }: { b: Block; seed?: boolean; onSeed?: () => void }) {
  switch (b.t) {
    case "card":
      return (
        <div className={`pg-card${b.items.length > 1 ? " pg-card--grid" : ""}`}>
          {b.items.map(([a, c]) => (
            <div key={a}>
              <b>
                <HL s={a} />
              </b>{" "}
              {b.items.length > 1 ? <br /> : null}
              <HL s={c} />
            </div>
          ))}
        </div>
      );
    case "table":
    case "lines":
      return (
        <table className={`pg-table${b.t === "lines" ? " pg-table--lines" : ""}${b.t === "table" && b.script ? " pg-table--script" : ""}`}>
          <tbody>
            {b.rows.map(([a, c]) => (
              <tr key={a}>
                <td>
                  <HL s={a} />
                </td>
                <td dir={/[؀-ۿ]/.test(c) ? "rtl" : undefined}>
                  <HL s={c} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      );
    case "chart":
      return <Chart />;
    case "p":
      if (seed && b.s.startsWith(SEED)) {
        return (
          <p className="pg-p">
            <span className="pg-mark" style={{ background: "rgba(255,224,61,.38)" }} onClick={onSeed}>
              <HL s={SEED} />
            </span>
            <HL s={b.s.slice(SEED.length)} />
          </p>
        );
      }
      return (
        <p className="pg-p">
          <HL s={b.s} />
        </p>
      );
    default: {
      const Tag = b.t === "h1" ? "h1" : b.t === "h2" ? "h2" : b.t === "h3" ? "h3" : "p";
      return (
        <Tag className={`pg-${b.t}`}>
          <HL s={b.s} />
        </Tag>
      );
    }
  }
}

function Chart() {
  const months = ["Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct"];
  const rows = [3, 5, 6, 8, 7, 9, 12, 10];
  return (
    <svg className="pg-chart" viewBox="0 0 720 360" aria-hidden="true">
      {[0, 1, 2, 3].map((i) => (
        <line key={i} x1="40" x2="700" y1={40 + i * 90} y2={40 + i * 90} stroke="#E4DED3" />
      ))}
      {rows.map((r, i) => (
        <g key={months[i]}>
          <rect x={70 + i * 80} y={310 - r * 22} width="44" height={r * 22} rx="3" fill={i === 6 ? "#C8862A" : "#B9B2A6"} />
          <text x={92 + i * 80} y="336" fontSize="14" textAnchor="middle" fill="#6B665E">
            {months[i]}
          </text>
        </g>
      ))}
    </svg>
  );
}

export function PdfPage({ page, seed, onSeed }: { page: Page; seed?: boolean; onSeed?: () => void }) {
  return (
    <div className={`pg${page.small ? " pg--small" : ""}`} style={{ width: page.w, height: page.h, padding: page.pad ?? (page.w > page.h ? 48 : 58) }}>
      {page.blocks.map((b, i) => (
        <PageBlock key={i} b={b} seed={seed} onSeed={onSeed} />
      ))}
    </div>
  );
}

// ── Books ────────────────────────────────────────────────────────────────

export function BookText({ doc }: { doc: Doc }) {
  return (
    <article className="k-book">
      <p className="k-book__kicker">{doc.author}</p>
      <h1>
        <HL s={doc.chapter ?? ""} />
      </h1>
      {(doc.text ?? []).map((t) => (
        <p key={t.slice(0, 20)}>
          <HL s={t} />
        </p>
      ))}
    </article>
  );
}

// ── Markdown ─────────────────────────────────────────────────────────────

const MD_P1 = ["Kitab picks a path by format. Fixed layouts go through MuPDF's page renderer, books are reflowed, and Markdown is laid out by Kitab itself. Every path ends in the same ", " view."];
const MD_TABLE: [string, string][] = [
  ["Format", "Path"],
  ["PDF, XPS, CBZ", "Page renderer"],
  ["EPUB, MOBI, FB2", "Reflow layout"],
  ["Markdown", "Kitab's own layout"],
];
const MD_TASKS: [boolean, string][] = [
  [true, "Open the file"],
  [true, "Draw the first page"],
  [false, "Read the rest"],
];
const MD_QUOTE = "Reading first, chrome quiet.";
const MD_STRINGS = ["How Kitab draws a page", ...MD_P1, "The zoom label", "The paths", ...MD_TABLE.flat(), ...MD_TASKS.map((t) => t[1]), MD_QUOTE];

export const MD_OUTLINE = [
  { title: "How Kitab draws a page", depth: 0, at: 0 },
  { title: "The zoom label", depth: 1, at: 0.14 },
  { title: "The paths", depth: 1, at: 0.5 },
];

type Tok = [string, string];
const CODE: Tok[][] = [
  [["com", "// A book shows its text size: zoom there changes the text, not the page."]],
  [["kw", "fn "], ["fn", "zoom_label"], ["pun", "("], ["", "book_pt"], ["pun", ": "], ["ty", "Option"], ["pun", "<"], ["ty", "f32"], ["pun", ">, "], ["", "percent"], ["pun", ": "], ["ty", "u32"], ["pun", ") -> "], ["ty", "String"], ["pun", " {"]],
  [["", "    "], ["kw", "match "], ["", "book_pt"], ["pun", " {"]],
  [["", "        "], ["ty", "Some"], ["pun", "("], ["", "pt"], ["pun", ") => "], ["fn", "format!"], ["pun", "("], ["str", '"{pt:.0} pt"'], ["pun", "),"]],
  [["", "        "], ["ty", "None"], ["pun", " => "], ["fn", "format!"], ["pun", "("], ["str", '"{percent} %"'], ["pun", "),"]],
  [["", "    "], ["pun", "}"]],
  [["pun", "}"]],
];
const CODE_TEXT = CODE.map((l) => l.map((t) => t[1]).join("")).join("\n");

function CodeBlock() {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const copy = () => {
    navigator.clipboard?.writeText(CODE_TEXT).catch(() => {});
    setCopied(true);
    window.clearTimeout(timer.current);
    // As long as the app shows it (COPIED_FOR, 1.5 s).
    timer.current = window.setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div className="md-code">
      <div className="md-code__head">
        <span>rust</span>
        <button type="button" className="md-copy" data-done={copied || undefined} onClick={copy} title="Copy code">
          <span className="md-copy__in" key={copied ? "y" : "n"}>
            {copied ? <KIcon name="check" size={12} /> : null}
            {copied ? "Copied" : "Copy"}
          </span>
        </button>
      </div>
      <pre>
        {CODE.map((line, i) => (
          <div key={i}>
            {line.map(([k, s], j) =>
              k ? (
                <span key={j} className={`tk-${k}`}>
                  {s}
                </span>
              ) : (
                s
              ),
            )}
            {line.length === 0 ? " " : null}
          </div>
        ))}
      </pre>
    </div>
  );
}

/** The flowchart, drawn in the theme's tokens like Kitab's Mermaid renderer. */
function Flowchart() {
  // One id per diagram: the page can show several windows, each with this chart.
  const arrow = `mm-arrow${useId().replace(/:/g, "")}`;
  const node = (x: number, y: number, w: number, label: string) => (
    <g key={label}>
      <rect x={x} y={y} width={w} height="34" rx="3" className="mm-node" />
      <text x={x + w / 2} y={y + 21.5} textAnchor="middle" className="mm-text">
        {label}
      </text>
    </g>
  );
  const edge = (d: string, k: string) => <path key={k} d={d} className="mm-edge" markerEnd={`url(#${arrow})`} />;
  const lab = (x: number, y: number, s: string) => (
    <g key={s}>
      <rect x={x - s.length * 3.1 - 5} y={y - 9} width={s.length * 6.2 + 10} height="16" className="mm-lab" />
      <text x={x} y={y + 3} textAnchor="middle" className="mm-small">
        {s}
      </text>
    </g>
  );
  return (
    <svg className="md-mermaid" viewBox="0 0 640 190" role="img" aria-label="Flowchart: Open file, then by format to the page renderer, the reflow layout or the native renderer, then the reader view">
      <defs>
        <marker id={arrow} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
          <path d="M0 0 10 5 0 10z" className="mm-head" />
        </marker>
      </defs>
      {edge("M92 95 H128", "e1")}
      {edge("M198 95 C 240 95, 240 30, 300 30", "e2")}
      {edge("M210 95 H300", "e3")}
      {edge("M198 95 C 240 95, 240 160, 300 160", "e4")}
      {edge("M470 30 C 510 30, 510 90, 536 92", "e5")}
      {edge("M470 95 H536", "e6")}
      {edge("M470 160 C 510 160, 510 100, 536 98", "e7")}
      {node(8, 78, 84, "Open file")}
      <g>
        <path d="M170 70 210 95 170 120 130 95z" className="mm-node" />
        <text x="170" y="99" textAnchor="middle" className="mm-text">
          Format?
        </text>
      </g>
      {lab(250, 48, "PDF / XPS / CBZ")}
      {lab(255, 95, "EPUB / MOBI")}
      {lab(250, 144, "Markdown")}
      {node(300, 13, 170, "MuPDF page renderer")}
      {node(300, 78, 170, "MuPDF reflow layout")}
      {node(300, 143, 170, "Native GPUI renderer")}
      {node(538, 78, 94, "Reader view")}
    </svg>
  );
}

export function MarkdownDoc() {
  return (
    <article className="md">
      <h1>
        <HL s="How Kitab draws a page" />
      </h1>
      <p>
        <HL s={MD_P1[0]} />
        <code>Reader</code>
        <HL s={MD_P1[1]} />
      </p>
      <h2>
        <HL s="The zoom label" />
      </h2>
      <CodeBlock />
      <h2>
        <HL s="The paths" />
      </h2>
      <Flowchart />
      <table>
        <thead>
          <tr>
            <th>
              <HL s={MD_TABLE[0][0]} />
            </th>
            <th>
              <HL s={MD_TABLE[0][1]} />
            </th>
          </tr>
        </thead>
        <tbody>
          {MD_TABLE.slice(1).map(([a, b]) => (
            <tr key={a}>
              <td>
                <HL s={a} />
              </td>
              <td>
                <HL s={b} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <ul className="md-tasks">
        {MD_TASKS.map(([done, s]) => (
          <li key={s}>
            <span className="md-box" data-done={done || undefined}>
              {done ? <KIcon name="check" size={11} /> : null}
            </span>
            <HL s={s} />
          </li>
        ))}
      </ul>
      <blockquote>
        <HL s={MD_QUOTE} />
      </blockquote>
    </article>
  );
}
