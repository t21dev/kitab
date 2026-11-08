// SPDX-License-Identifier: AGPL-3.0-or-later
// Kitab's window, rebuilt in HTML so a visitor can use it: the title bar with
// Home and tabs (shell.rs), the sidebar, the reading area, the quick dock with
// the page navigator (reader/dock.rs, reader.rs), Find (reader/find.rs), the
// markup bar (reader/markup.rs) and the status bar. Sizes are the app's at
// Compact density; colours are its theme tokens (themes.css).
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent as RPointerEvent } from "react";
import { DockBtn, IconBtn, KIcon } from "./atoms";
import { BookText, countHits, findRegex, HlScope, MarkdownDoc, MD_OUTLINE, PdfPage, searchStrings } from "./Content";
import { DOCS, MARKDOWN_SOURCE, MD_FACTS, type Doc, type Page } from "./docs";
import { Frame, useWin } from "./Frame";
import { HomeView } from "./Home";
import type { KIconName } from "./icons";
import { MoreMenu, SettingsModal, type MenuAction, type Section } from "./Settings";
import { useMock } from "./store";

export type Start = {
  tabs: string[];
  /** null: Home is showing. */
  active: string | null;
  sidebar?: boolean;
  find?: string;
  markup?: boolean;
  page?: number;
  zoom?: number;
  settings?: Section;
};

const TITLE_H = 44;
const STATUS_H = 26;

// ── Title bar ────────────────────────────────────────────────────────────

function TitleBar({
  tabs,
  active,
  onHome,
  onTab,
  onClose,
}: {
  tabs: string[];
  active: string | null;
  onHome: () => void;
  onTab: (id: string) => void;
  onClose: (id: string) => void;
}) {
  const { w } = useWin();
  // The room tabs have: the bar less Home, the divider, +, a 96 px drag area and
  // the three window buttons (shell.rs). Crowded tabs share it, 120 px at least.
  const room = Math.max(0, w - (8 + 28 + 17 + 32 + 96 + 3 * 46 + 12));
  const crowded = tabs.length * 224 > room;
  const tabW = crowded ? Math.max(120, Math.min(220, (room - 4 * tabs.length) / Math.max(1, tabs.length))) : undefined;
  return (
    <div className="k-title" style={{ height: TITLE_H }}>
      <IconBtn icon="home" label="Home  Ctrl+Shift+H" on={active === null} onClick={onHome} />
      {tabs.length ? <span className="k-title__div" /> : null}
      <div className="k-tabs" style={{ maxWidth: room }}>
        {tabs.map((id) => {
          const d = DOCS[id];
          const on = id === active;
          return (
            <div key={id} className="k-tab" data-on={on || undefined} style={tabW ? { width: tabW } : undefined}>
              <button type="button" className="k-tab__hit" onClick={() => onTab(id)} aria-current={on ? "page" : undefined}>
                <KIcon name={d.icon} size={14} />
                <span className="k-tab__title">{d.title}</span>
              </button>
              <button type="button" className="k-tab__x" aria-label={`Close ${d.title}`} title="Close tab  Ctrl+W" onClick={() => onClose(id)}>
                <KIcon name="close" size={11} />
              </button>
            </div>
          );
        })}
      </div>
      {tabs.length ? <IconBtn icon="plus" label="New tab: Home and Open file…" onClick={onHome} /> : null}
      <span className="k-title__drag" />
      <span className="k-winbtn" aria-hidden="true">
        <KIcon name="win-min" size={14} />
      </span>
      <span className="k-winbtn" aria-hidden="true">
        <KIcon name="win-max" size={14} />
      </span>
      <span className="k-winbtn k-winbtn--close" aria-hidden="true">
        <KIcon name="close" size={14} />
      </span>
    </div>
  );
}

// ── Markup ───────────────────────────────────────────────────────────────

type Tool = "pen" | "highlight" | "line" | "arrow-diagonal" | "square" | "cover" | "circle" | "text" | "note" | "eraser";
const TOOLS: [Tool, string][] = [
  ["pen", "Pen"],
  ["highlight", "Highlighter"],
  ["line", "Line  Shift keeps it at 45°"],
  ["arrow-diagonal", "Arrow  Shift keeps it at 45°"],
  ["square", "Rectangle  Shift for a square"],
  ["cover", "Cover  a filled box over what you hide (the text under it stays in the file)"],
  ["circle", "Ellipse  Shift for a circle"],
  ["text", "Text  click where it goes"],
  ["note", "Note  click to pin one"],
  ["eraser", "Eraser  click a mark"],
];
// Ink colours (kitab-theme/src/markup.rs): marks keep them in every theme.
const PEN: [string, string][] = [
  ["Black", "#1F1D1A"],
  ["Red", "#D0342C"],
  ["Blue", "#1F6FD1"],
  ["Green", "#2E8B57"],
  ["Amber", "#E09A10"],
  ["Purple", "#7B4FD6"],
];
const HIGHLIGHTER: [string, string][] = [
  ["Yellow", "#FFE03D"],
  ["Green", "#7EE081"],
  ["Cyan", "#5CD6F0"],
  ["Pink", "#FF8AC2"],
  ["Orange", "#FFAE4D"],
];
const PEN_W = [1, 2, 4];
const HL_W = [8, 12, 18];

type Mark = { id: number; page: number; tool: Tool; color: string; w: number; pts: [number, number][] };

function MarkShape({ m, eraser, onErase }: { m: Mark; eraser: boolean; onErase: (id: number) => void }) {
  const [a, b] = [m.pts[0], m.pts[m.pts.length - 1]];
  const common = {
    onClick: eraser ? () => onErase(m.id) : undefined,
    className: eraser ? "k-mark k-mark--erasable" : "k-mark",
  };
  const stroke = { stroke: m.color, strokeWidth: m.w, fill: "none", strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  switch (m.tool) {
    case "pen":
    case "highlight":
      return <path {...common} {...stroke} opacity={m.tool === "highlight" ? 0.38 : 1} d={"M" + m.pts.map((p) => p.join(" ")).join("L")} />;
    case "line":
      return <line {...common} {...stroke} x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} />;
    case "arrow-diagonal": {
      const ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
      const head = 6 + m.w * 2.5;
      const h1 = [b[0] - head * Math.cos(ang - 0.5), b[1] - head * Math.sin(ang - 0.5)];
      const h2 = [b[0] - head * Math.cos(ang + 0.5), b[1] - head * Math.sin(ang + 0.5)];
      return <path {...common} {...stroke} d={`M${a.join(" ")}L${b.join(" ")}M${h1.join(" ")}L${b.join(" ")}L${h2.join(" ")}`} />;
    }
    case "square":
    case "cover": {
      const x = Math.min(a[0], b[0]);
      const y = Math.min(a[1], b[1]);
      const r = { x, y, width: Math.abs(b[0] - a[0]), height: Math.abs(b[1] - a[1]) };
      return m.tool === "cover" ? <rect {...common} {...r} fill={m.color} /> : <rect {...common} {...stroke} {...r} />;
    }
    case "circle":
      return <ellipse {...common} {...stroke} cx={(a[0] + b[0]) / 2} cy={(a[1] + b[1]) / 2} rx={Math.abs(b[0] - a[0]) / 2} ry={Math.abs(b[1] - a[1]) / 2} />;
    case "note":
      return (
        <g {...common} transform={`translate(${a[0] - 10} ${a[1] - 10})`}>
          <rect width="20" height="20" rx="5" fill={m.color} />
          <path d="M6 5h8a1 1 0 0 1 1 1v5.5L11.5 15H6a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM7.5 8h5M7.5 10.5h3" stroke="#fff" strokeWidth="1.1" fill="none" strokeLinecap="round" />
        </g>
      );
    case "text":
      return (
        <text {...common} x={a[0]} y={a[1]} fill={m.color} fontSize={[12, 16, 22][PEN_W.indexOf(m.w)] ?? 16} fontFamily="Inter Variable, Segoe UI, sans-serif">
          Text
        </text>
      );
    default:
      return null;
  }
}

// ── Reader ───────────────────────────────────────────────────────────────

/** Thumbnails: 132 px for an A4 page's width, every page at its true size. */
const THUMB = 132 / 595;
const ZOOM_STEPS = [10, 25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400];
type Zoom = { mode: "width" } | { mode: "page" } | { mode: "pct"; pct: number };
type Pane = "contents" | "marks" | "pages" | "bookmarks";

function Reader({
  doc,
  start,
  onSettings,
  shown,
}: {
  doc: Doc;
  start: Start;
  onSettings: () => void;
  shown: boolean;
}) {
  const { w: W, h: H, s: scale } = useWin();
  const mock = useMock();
  const pdf = doc.kind === "pdf";
  const pages = doc.pages ?? [];
  const startHere = start.active === doc.id;

  const [sidebar, setSidebar] = useState(startHere ? (start.sidebar ?? true) && W >= 640 : W >= 640);
  // A small window starts with the sidebar closed (the size is known only once measured).
  const narrowed = useRef(false);
  useEffect(() => {
    if (W < 640 && !narrowed.current) {
      narrowed.current = true;
      setSidebar(false);
    }
  }, [W]);
  const [pane, setPane] = useState<Pane>("contents");
  const [zoom, setZoom] = useState<Zoom>(startHere && start.zoom ? { mode: "pct", pct: start.zoom } : { mode: "width" });
  const [bookPt, setBookPt] = useState(18);
  const [rot, setRot] = useState(0);
  const [frac, setFrac] = useState(0);
  const [cur, setCur] = useState(0);
  const [bookPage, setBookPage] = useState(1);
  const [hist, setHist] = useState<{ back: number[]; fwd: number[] }>({ back: [], fwd: [] });
  const [edit, setEdit] = useState<string | null>(null);
  const [menu, setMenu] = useState(false);
  const [dockHidden, setDockHidden] = useState(false);
  const [idle, setIdle] = useState(false);
  const [mdEdit, setMdEdit] = useState(false);

  // Find
  const [findOpen, setFindOpen] = useState(startHere && start.find !== undefined);
  const [query, setQuery] = useState(startHere ? (start.find ?? "") : "");
  const [matchCase, setMatchCase] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [hit, setHit] = useState(0);
  const findInput = useRef<HTMLInputElement>(null);

  // Markup
  const [markupOpen, setMarkupOpen] = useState(Boolean(startHere && start.markup));
  const [tool, setTool] = useState<Tool | null>(startHere && start.markup ? "highlight" : null);
  const [penC, setPenC] = useState(1);
  const [hlC, setHlC] = useState(0);
  const [size, setSize] = useState(1);
  const [marks, setMarks] = useState<Mark[]>([]);
  const [undo, setUndo] = useState<Mark[][]>([]);
  const [redo, setRedo] = useState<Mark[][]>([]);
  const [seed, setSeed] = useState(Boolean(startHere && start.markup && doc.id === "welcome"));
  const [saved, setSaved] = useState<{ ids: Set<number>; seed: boolean }>({ ids: new Set(), seed: false });
  const [note, setNote] = useState<string | null>(null);
  const draft = useRef<Mark | null>(null);
  const [draftMark, setDraftMark] = useState<Mark | null>(null);
  const nextId = useRef(1);

  const scroller = useRef<HTMLDivElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);

  const sideW = W >= 760 ? 240 : Math.round(Math.max(180, Math.min(260, W * 0.4)));
  const rw = W - (sidebar ? sideW + 1 : 0);
  const paneH = H - TITLE_H - STATUS_H;

  // Page geometry (points → px): zoom 100 % is 96 dpi, as on screen.
  const dims = (p: Page): [number, number] => (rot % 180 ? [p.h, p.w] : [p.w, p.h]);
  const first = pages[0] ? dims(pages[0]) : [595, 842];
  const zf =
    zoom.mode === "width"
      ? (rw - 56) / first[0]
      : zoom.mode === "page"
        ? Math.min((rw - 56) / first[0], (paneH - 60) / first[1])
        : (zoom.pct / 100) * (4 / 3);
  const zoomPct = Math.round(zf * 75);
  const [mdPct, setMdPct] = useState(100);
  const tops = useMemo(() => {
    let y = 22;
    return pages.map((p) => {
      const t = y;
      y += dims(p)[1] * zf + 22;
      return t;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pages, zf, rot]);

  const onScroll = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    const max = el.scrollHeight - el.clientHeight;
    setFrac(max > 0 ? el.scrollTop / max : 0);
    if (pdf) {
      const y = el.scrollTop + el.clientHeight * 0.3;
      let c = 0;
      tops.forEach((t, i) => t - 22 <= y && (c = i));
      setCur(c);
    }
  }, [pdf, tops]);

  // Start where the scene asks (again once the window is measured), until the visitor scrolls.
  const keep = useRef(0);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    if (startHere && start.page && !keep.current) {
      el.scrollTop = (tops[start.page] ?? 0) - 12;
    }
    onScroll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [zf]);

  const goPage = (n: number, record = true) => {
    const el = scroller.current;
    if (!el) return;
    const p = Math.max(0, Math.min(pages.length - 1, n));
    if (record) setHist((h) => ({ back: [...h.back, cur], fwd: [] }));
    el.scrollTop = tops[p] - 12;
  };

  const seek = (clientX: number) => {
    const t = track.current;
    const el = scroller.current;
    if (!t) return;
    const r = t.getBoundingClientRect();
    const f = Math.max(0, Math.min(1, (clientX - r.left) / r.width));
    if (doc.kind === "book") {
      setBookPage(1 + Math.round(f * ((doc.bookPages ?? 1) - 1)));
      setFrac(f);
      return;
    }
    if (el) el.scrollTop = f * (el.scrollHeight - el.clientHeight);
  };

  const step = (up: boolean) => {
    if (doc.kind === "book") return setBookPt((p) => Math.max(10, Math.min(32, p + (up ? 1 : -1))));
    if (doc.kind === "md") {
      const n = up ? ZOOM_STEPS.find((s) => s > mdPct) : [...ZOOM_STEPS].reverse().find((s) => s < mdPct);
      if (n && n >= 50 && n <= 200) setMdPct(n);
      return;
    }
    const now = Math.round(zf * 75);
    const next = up ? ZOOM_STEPS.find((s) => s > now + 0.5) : [...ZOOM_STEPS].reverse().find((s) => s < now - 0.5);
    if (next) setZoom({ mode: "pct", pct: next });
  };

  // Find: count, keep the current hit in range, and bring it into view.
  const strings = useMemo(() => searchStrings(doc), [doc]);
  const re = findOpen ? findRegex(query, matchCase, wholeWord) : null;
  const total = countHits(strings, re);
  const curHit = total ? ((hit % total) + total) % total : -1;
  useEffect(() => {
    if (findOpen) findInput.current?.focus({ preventScroll: true });
  }, [findOpen]);
  useEffect(() => {
    const el = scroller.current;
    const m = el?.querySelector<HTMLElement>(".k-hit[data-cur]");
    if (!el || !m || !shown) return;
    const dy = (m.getBoundingClientRect().top - el.getBoundingClientRect().top) / scale;
    if (dy < 40 || dy > el.clientHeight - 90) el.scrollTop += dy - el.clientHeight * 0.35;
  }, [curHit, query, findOpen, shown, scale]);

  // Markup
  const unsaved = marks.filter((m) => !saved.ids.has(m.id)).length + (seed && !saved.seed ? 1 : 0);
  const commit = (next: Mark[]) => {
    setUndo((u) => [...u, marks]);
    setRedo([]);
    setMarks(next);
  };
  const svgPoint = (e: RPointerEvent<SVGSVGElement>): [number, number] => {
    const svg = e.currentTarget;
    const pt = svg.createSVGPoint();
    pt.x = e.clientX;
    pt.y = e.clientY;
    const p = pt.matrixTransform(svg.getScreenCTM()?.inverse());
    return [Math.round(p.x * 10) / 10, Math.round(p.y * 10) / 10];
  };
  const down = (page: number) => (e: RPointerEvent<SVGSVGElement>) => {
    if (!tool || tool === "eraser" || e.button !== 0) return;
    const pt = svgPoint(e);
    const hl = tool === "highlight";
    const m: Mark = {
      id: nextId.current++,
      page,
      tool,
      color: hl ? HIGHLIGHTER[hlC][1] : PEN[penC][1],
      w: hl ? HL_W[size] : PEN_W[size],
      pts: [pt],
    };
    if (tool === "note" || tool === "text") {
      if (tool === "note") m.color = "#E09A10";
      commit([...marks, m]);
      return;
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    draft.current = m;
    setDraftMark(m);
  };
  const move = (e: RPointerEvent<SVGSVGElement>) => {
    const d = draft.current;
    if (!d) return;
    const pt = svgPoint(e);
    const free = d.tool === "pen" || d.tool === "highlight";
    d.pts = free ? [...d.pts, pt] : [d.pts[0], pt];
    setDraftMark({ ...d });
  };
  const up = () => {
    const d = draft.current;
    draft.current = null;
    setDraftMark(null);
    if (!d) return;
    const [a, b] = [d.pts[0], d.pts[d.pts.length - 1]];
    if (Math.hypot(b[0] - a[0], b[1] - a[1]) < 3 && d.pts.length < 4) return; // a click, not a mark
    commit([...marks, d]);
  };
  const erase = (id: number) => commit(marks.filter((m) => m.id !== id));
  const save = () => {
    const n = unsaved;
    if (!n) return;
    setSaved({ ids: new Set(marks.map((m) => m.id)), seed });
    setNote(`Saved ${n} mark${n === 1 ? "" : "s"} into the PDF`);
  };
  useEffect(() => {
    if (!note) return;
    const t = window.setTimeout(() => setNote(null), 3000);
    return () => window.clearTimeout(t);
  }, [note]);

  // Fade the dock while reading (1.8 s without the pointer moving).
  const idleTimer = useRef<number | undefined>(undefined);
  const wake = () => {
    if (!mock.fadeDock) return;
    setIdle(false);
    window.clearTimeout(idleTimer.current);
    idleTimer.current = window.setTimeout(() => !menu && !findOpen && edit === null && setIdle(true), 1800);
  };
  useEffect(() => {
    if (!mock.fadeDock) setIdle(false);
    else wake();
    return () => window.clearTimeout(idleTimer.current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mock.fadeDock]);

  const menuAction = (a: MenuAction) => {
    setMenu(false);
    if (a === "contents") setSidebar((s) => !s);
    if (a === "goto") setEdit("");
    if (a === "settings") onSettings();
  };

  // What fits in the dock (dock.rs): the least-used groups step aside first.
  const compact = mock.compactDock;
  const tight = rw < 820;
  const tighter = rw < 640;
  const tightest = rw < 520;
  const trackW = rw < 820 ? (rw < 520 ? Math.max(60, Math.min(100, rw - 330)) : Math.max(100, Math.min(220, rw - 470))) : 220;
  const count = pdf ? pages.length : (doc.bookPages ?? 1);
  const pageNow = pdf ? cur + 1 : bookPage;
  const zoomLabel = doc.kind === "book" ? `${bookPt} pt` : doc.kind === "md" ? `${mdPct} %` : `${zoomPct} %`;

  const pageField =
    edit !== null ? (
      <input
        className="k-pagefield k-pagefield--edit"
        autoFocus
        inputMode="numeric"
        aria-label="Go to page"
        value={edit}
        placeholder={String(pageNow)}
        onChange={(e) => setEdit(e.target.value.replace(/\D/g, ""))}
        onBlur={() => setEdit(null)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setEdit(null);
          if (e.key === "Enter") {
            const n = parseInt(edit, 10);
            if (n) {
              if (pdf) goPage(n - 1);
              else {
                const p = Math.max(1, Math.min(count, n));
                setBookPage(p);
                setFrac((p - 1) / Math.max(1, count - 1));
              }
            }
            setEdit(null);
          }
        }}
        style={{ width: Math.max(40, String(count).length * 9 + 22) }}
      />
    ) : (
      <button type="button" className="k-pagefield" title="Go to page  Ctrl+G" onClick={() => setEdit("")}>
        {pageNow}
      </button>
    );

  const navTrack = (
    <div
      ref={track}
      className="k-track"
      style={{ width: trackW }}
      onPointerDown={(e) => {
        dragging.current = true;
        e.currentTarget.setPointerCapture(e.pointerId);
        seek(e.clientX);
      }}
      onPointerMove={(e) => dragging.current && seek(e.clientX)}
      onPointerUp={() => (dragging.current = false)}
      onPointerCancel={() => (dragging.current = false)}
      role="slider"
      aria-label="Page"
      aria-valuemin={1}
      aria-valuemax={count}
      aria-valuenow={pageNow}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "ArrowRight" && pdf) goPage(cur + 1, false);
        if (e.key === "ArrowLeft" && pdf) goPage(cur - 1, false);
      }}
    >
      <span className="k-track__well">
        <i className="k-track__fill" style={{ width: `${frac * 100}%` }} />
        <i className="k-track__thumb" style={{ left: `calc(${frac * 100}% - 7px)` }} />
      </span>
    </div>
  );

  const dockInner = (
    <div className="k-dock" data-idle={idle || undefined} onPointerEnter={() => setIdle(false)}>
      {!compact && !tightest ? (
        <span className="k-group">
          <DockBtn icon="sidebar" label="Sidebar  Ctrl+B" onClick={() => setSidebar((s) => !s)} />
        </span>
      ) : null}
      {!compact && !tight && pdf ? (
        <span className="k-group">
          <DockBtn
            icon="arrow-left"
            label="Back  Alt+Left"
            disabled={!hist.back.length}
            onClick={() =>
              setHist((h) => {
                const to = h.back[h.back.length - 1];
                goPage(to, false);
                return { back: h.back.slice(0, -1), fwd: [...h.fwd, cur] };
              })
            }
          />
          <DockBtn
            icon="arrow-right"
            label="Forward  Alt+Right"
            disabled={!hist.fwd.length}
            onClick={() =>
              setHist((h) => {
                const to = h.fwd[h.fwd.length - 1];
                goPage(to, false);
                return { back: [...h.back, cur], fwd: h.fwd.slice(0, -1) };
              })
            }
          />
        </span>
      ) : null}
      <span className="k-nav">
        {doc.kind === "md" ? (
          <>
            {navTrack}
            <span className="tnum">{Math.round(frac * 100)} %</span>
          </>
        ) : (
          <>
            {pageField}
            {navTrack}
            <span className="tnum">{count}</span>
          </>
        )}
      </span>
      <span className="k-sep" />
      <span className="k-group">
        <DockBtn icon="minus" label="Zoom out  Ctrl+−" onClick={() => step(false)} />
        <span className="k-zoom tnum">{zoomLabel}</span>
        <DockBtn icon="plus" label="Zoom in  Ctrl+=" onClick={() => step(true)} />
        {!compact && !tighter && pdf ? (
          <>
            <DockBtn icon="fit-width" label="Fit width  Ctrl+2" on={zoom.mode === "width"} onClick={() => setZoom({ mode: "width" })} />
            <DockBtn icon="fit-page" label="Fit page  Ctrl+0" on={zoom.mode === "page"} onClick={() => setZoom({ mode: "page" })} />
          </>
        ) : null}
      </span>
      <span className="k-group k-group--more">
        <DockBtn icon="search" label="Find  Ctrl+F" on={findOpen} onClick={() => setFindOpen((o) => !o)} />
        {!compact && pdf ? (
          <DockBtn
            icon="pen"
            label="Draw and mark up  Ctrl+Shift+D"
            on={markupOpen}
            onClick={() => {
              setMarkupOpen((o) => !o);
              setTool((t) => (markupOpen ? null : (t ?? "pen")));
            }}
          />
        ) : null}
        {unsaved > 0 ? (
          <DockBtn icon="save" label={`Save ${unsaved} mark${unsaved === 1 ? "" : "s"} into the PDF  Ctrl+S`} onClick={save}>
            <b className="k-badge-n">{unsaved}</b>
          </DockBtn>
        ) : null}
        {!compact && !tight && pdf ? <DockBtn icon="rotate" label="Rotate  Ctrl+Shift+=" onClick={() => setRot((r) => (r + 90) % 360)} /> : null}
        <DockBtn icon="more-vertical" label="More" on={menu} onClick={() => setMenu((m) => !m)} />
        <DockBtn icon="chevron-down" label="Hide the dock  Ctrl+." onClick={() => setDockHidden(true)} />
      </span>
      {menu ? <MoreMenu onAction={menuAction} onClose={() => setMenu(false)} /> : null}
    </div>
  );

  const outline = pdf ? (doc.outline ?? []) : doc.kind === "md" ? MD_OUTLINE.map((o) => ({ title: o.title, depth: o.depth, page: -1, at: o.at })) : [];
  const activeRow = pdf ? outline.reduce((a, o, i) => (o.page <= cur ? i : a), 0) : doc.kind === "md" ? MD_OUTLINE.reduce((a, o, i) => (o.at <= frac + 0.02 ? i : a), 0) : 0;

  const fileInfo =
    doc.kind === "pdf"
      ? `PDF  ·  ${doc.size}  ·  ${pages.length} pages  ·  ${pages[cur]?.paper ?? ""}`
      : doc.kind === "md"
        ? MD_FACTS
        : `${doc.format}  ·  ${count} pages  ·  ${bookPt} pt`;
  const position = doc.kind === "md" ? `${Math.round(frac * 100)} %` : `Page ${pageNow} of ${count}`;

  return (
    <div className="k-body" onPointerMove={wake}>
      {sidebar ? (
        <aside className="k-side" style={{ width: sideW }}>
          <div className="k-side__head">
            <span className="k-side__title">{pane === "contents" ? "Contents" : pane === "pages" ? "Pages" : pane === "marks" ? "Marks" : "Bookmarks"}</span>
            <span className="k-side__tabs">
              {(
                [
                  ["contents", "outline", "Contents"],
                  ["marks", "highlight", "Marks"],
                  ...(pdf ? [["pages", "pages", "Pages"]] : []),
                  ["bookmarks", "bookmark", "Bookmarks"],
                ] as [Pane, KIconName, string][]
              ).map(([p, icon, label]) => (
                <IconBtn key={p} icon={icon} label={label} on={pane === p} onClick={() => setPane(p)} />
              ))}
              <IconBtn icon="sidebar" label="Hide sidebar  Ctrl+B" onClick={() => setSidebar(false)} />
            </span>
          </div>
          <div className="k-side__list">
            {pane === "contents" ? (
              outline.length ? (
                outline.map((o, i) => (
                  <button
                    key={o.title}
                    type="button"
                    className="k-row"
                    data-on={i === activeRow || undefined}
                    style={{ paddingLeft: 10 + o.depth * 21 }}
                    onClick={() => {
                      if (pdf) goPage(o.page);
                      else if ("at" in o) {
                        const el = scroller.current;
                        if (el) el.scrollTop = (o.at as number) * (el.scrollHeight - el.clientHeight);
                      }
                    }}
                  >
                    <span className="k-ring" />
                    <span className="k-row__title">{o.title}</span>
                    {pdf ? <span className="k-row__n tnum">{o.page + 1}</span> : null}
                  </button>
                ))
              ) : (
                <p className="k-side__empty">{doc.kind === "book" ? doc.chapter : "This document has no contents list."}</p>
              )
            ) : pane === "pages" ? (
              <div className="k-thumbs">
                {pages.map((p, i) => (
                  <button key={i} type="button" className="k-thumb" data-on={i === cur || undefined} onClick={() => goPage(i)}>
                    <span className="k-thumb__page" style={{ width: p.w * THUMB, height: p.h * THUMB }}>
                      <span style={{ transform: `scale(${THUMB})` }}>
                        <HlScope re={null} cur={-1}>
                          <PdfPage page={p} />
                        </HlScope>
                      </span>
                    </span>
                    <small className="tnum">{i + 1}</small>
                  </button>
                ))}
              </div>
            ) : pane === "marks" ? (
              <div className="k-marks">
                <p className="k-marks__export">Export  Markdown  CSV</p>
                {seed ? (
                  <div className="k-markrow">
                    <i style={{ background: HIGHLIGHTER[0][1] }} />
                    <span>
                      Highlight <small>3</small>
                    </span>
                    <p>A highlight says: come back here.</p>
                  </div>
                ) : null}
                {marks.map((m) => (
                  <div key={m.id} className="k-markrow">
                    <i style={{ background: m.color }} />
                    <span>
                      {TOOLS.find((t) => t[0] === m.tool)?.[1].split("  ")[0]} <small>{m.page + 1}</small>
                    </span>
                    <p>{saved.ids.has(m.id) ? "Saved into the file" : "Not saved into the file yet"}</p>
                  </div>
                ))}
              </div>
            ) : (
              <p className="k-side__empty">Ctrl+D bookmarks the page you're on.</p>
            )}
          </div>
        </aside>
      ) : null}
      <div className="k-read" data-tool={(markupOpen && tool) || undefined}>
        <HlScope re={re} cur={curHit}>
          {doc.kind === "pdf" ? (
            <div ref={scroller} className="k-pane k-pane--desk" onScroll={onScroll} onPointerDown={() => (keep.current = 1)} onWheel={() => (keep.current = 1)}>
              <div className="k-sheets" style={{ height: (tops[tops.length - 1] ?? 0) + (pages.length ? dims(pages[pages.length - 1])[1] * zf : 0) + 120 }}>
                {pages.map((p, i) => {
                  const [pw, ph] = dims(p);
                  return (
                    <div key={i} className="k-sheet" style={{ top: tops[i], width: pw * zf, height: ph * zf }}>
                      <div
                        className="k-sheet__in"
                        style={{
                          width: p.w,
                          height: p.h,
                          transform: `translate(${pw * zf / 2}px, ${ph * zf / 2}px) rotate(${rot}deg) scale(${zf}) translate(${-p.w / 2}px, ${-p.h / 2}px)`,
                        }}
                      >
                        <PdfPage page={p} seed={seed && i === 2} onSeed={tool === "eraser" ? () => { setUndo((u) => [...u, marks]); setSeed(false); } : undefined} />
                        <svg
                          className="k-ink"
                          viewBox={`0 0 ${p.w} ${p.h}`}
                          data-live={(markupOpen && tool && tool !== "eraser") || undefined}
                          onPointerDown={down(i)}
                          onPointerMove={move}
                          onPointerUp={up}
                          onPointerCancel={up}
                        >
                          {marks
                            .filter((m) => m.page === i)
                            .map((m) => (
                              <MarkShape key={m.id} m={m} eraser={markupOpen && tool === "eraser"} onErase={erase} />
                            ))}
                          {draftMark && draftMark.page === i ? <MarkShape m={draftMark} eraser={false} onErase={erase} /> : null}
                        </svg>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          ) : doc.kind === "md" ? (
            <div ref={scroller} className="k-pane" onScroll={onScroll} style={{ "--mdz": mdPct / 100 } as CSSProperties}>
              {mdEdit ? null : (
                <button type="button" className="k-edit" onClick={() => setMdEdit(true)}>
                  <KIcon name="pen" size={14} /> Edit
                </button>
              )}
              {mdEdit ? <MdEditor onDone={() => setMdEdit(false)} /> : <MarkdownDoc />}
            </div>
          ) : (
            <div ref={scroller} className="k-pane" onScroll={onScroll} style={{ "--bookpt": `${bookPt}px` } as CSSProperties}>
              <BookText doc={doc} />
            </div>
          )}
        </HlScope>

        {findOpen ? (
          <div className="k-find" style={{ width: Math.min(440, rw - 20) }}>
            <label className="k-find__field" data-none={(query && total === 0) || undefined}>
              <KIcon name="search" size={14} />
              <input
                ref={findInput}
                value={query}
                placeholder="Find in document"
                aria-label="Find in document"
                onChange={(e) => {
                  setQuery(e.target.value);
                  setHit(0);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") setHit((h) => h + (e.shiftKey ? -1 : 1));
                  if (e.key === "Escape") setFindOpen(false);
                }}
              />
              <span className="tnum">{query ? (total ? `${curHit + 1} of ${total}` : "No results") : ""}</span>
            </label>
            <IconBtn icon="match-case" label="Match case (Alt+C)" on={matchCase} onClick={() => setMatchCase((v) => !v)} />
            <IconBtn icon="whole-word" label="Whole word (Alt+W)" on={wholeWord} onClick={() => setWholeWord((v) => !v)} />
            <span className="k-find__sep" />
            <IconBtn icon="arrow-up" label="Previous (Shift+F3)" disabled={!total} onClick={() => setHit((h) => h - 1)} />
            <IconBtn icon="arrow-down" label="Next (F3)" disabled={!total} onClick={() => setHit((h) => h + 1)} />
            <IconBtn icon="close" label="Close (Esc)" onClick={() => setFindOpen(false)} />
          </div>
        ) : null}

        {markupOpen && pdf ? (
          <div className="k-markup">
            <div className="k-markup__bar" style={{ maxWidth: rw - 24 }}>
              {TOOLS.map(([t, label]) => (
                <DockBtn key={t} icon={t} label={label} on={tool === t} onClick={() => setTool(t)} />
              ))}
              <span className="k-sep" />
              {(tool === "highlight" ? HIGHLIGHTER : PEN).map(([name, c], i) => {
                const on = tool === "highlight" ? hlC === i : penC === i;
                return (
                  <button
                    key={name}
                    type="button"
                    className="k-swatch"
                    data-on={on || undefined}
                    aria-label={name}
                    aria-pressed={on}
                    title={name}
                    onClick={() => {
                      if (tool === "highlight") setHlC(i);
                      else {
                        setPenC(i);
                        if (!tool || tool === "eraser") setTool("pen");
                      }
                    }}
                  >
                    <i style={{ background: c }} />
                  </button>
                );
              })}
              <span className="k-sep" />
              {["Thin", "Medium", "Thick"].map((label, i) => (
                <button key={label} type="button" className="k-size" data-on={size === i || undefined} aria-label={label} aria-pressed={size === i} title={label} onClick={() => setSize(i)}>
                  <i style={{ width: 4 + 3 * i, height: 4 + 3 * i }} />
                </button>
              ))}
              <span className="k-sep" />
              <DockBtn
                icon="undo"
                label="Undo  Ctrl+Z"
                disabled={!undo.length}
                onClick={() => {
                  const prev = undo[undo.length - 1];
                  setRedo((r) => [...r, marks]);
                  setUndo((u) => u.slice(0, -1));
                  setMarks(prev);
                }}
              />
              <DockBtn
                icon="redo"
                label="Redo  Ctrl+Y"
                disabled={!redo.length}
                onClick={() => {
                  const next = redo[redo.length - 1];
                  setUndo((u) => [...u, marks]);
                  setRedo((r) => r.slice(0, -1));
                  setMarks(next);
                }}
              />
            </div>
            {note ? <p className="k-markup__note">{note}</p> : null}
          </div>
        ) : null}

        <div className="k-dockwrap" style={{ bottom: STATUS_H + (dockHidden ? 6 : 16) }} onPointerDown={() => (keep.current = 1)}>
          {dockHidden ? (
            <button type="button" className="k-handle" aria-label="Show the dock  Ctrl+." title="Show the dock  Ctrl+." onClick={() => setDockHidden(false)}>
              <KIcon name="chevron-up" size={14} />
            </button>
          ) : (
            dockInner
          )}
        </div>

        <div className="k-status" style={{ height: STATUS_H }}>
          <span className="k-status__info">{fileInfo}</span>
          <span className="tnum">{position}</span>
        </div>
      </div>
    </div>
  );
}

function MdEditor({ onDone }: { onDone: () => void }) {
  return (
    <div className="k-mdedit">
      <div className="k-mdedit__src">
        <div className="k-mdedit__bar">
          <KIcon name="pen" size={14} />
          <span>Editing</span>
          <span className="k-grow" />
          <small className="tnum">Ln 1, Col 1</small>
          <button type="button" className="k-btn k-btn--sm k-btn--primary" onClick={onDone}>
            Done
          </button>
        </div>
        <pre className="k-mdedit__lines">
          {MD_LINES.map((l, i) => (
            <div key={i} data-caret={i === 0 || undefined}>
              <span className="tnum">{i + 1}</span>
              {l || " "}
            </div>
          ))}
        </pre>
      </div>
      <div className="k-mdedit__preview">
        <MarkdownDoc />
      </div>
    </div>
  );
}

const MD_LINES = MARKDOWN_SOURCE.trimEnd().split("\n");

// ── The window ───────────────────────────────────────────────────────────

function Workspace({ start }: { start: Start }) {
  const [tabs, setTabs] = useState(start.tabs);
  const [active, setActive] = useState<string | null>(start.active);
  const [settings, setSettings] = useState<Section | null>(start.settings ?? null);
  const [section, setSection] = useState<Section>(start.settings ?? "Appearance");
  const [autoCheck, setAutoCheck] = useState(true);
  const open = (id: string) => {
    setTabs((t) => (t.includes(id) ? t : [...t, id]));
    setActive(id);
  };
  const close = (id: string) => {
    const i = tabs.indexOf(id);
    const next = tabs.filter((t) => t !== id);
    setTabs(next);
    if (active === id) setActive(next.length ? next[Math.min(i, next.length - 1)] : null);
  };
  return (
    <>
      <TitleBar tabs={tabs} active={active} onHome={() => setActive(null)} onTab={setActive} onClose={close} />
      {active === null ? (
        <HomeView
          onOpen={open}
          onSettings={() => {
            setSection("Appearance");
            setSettings("Appearance");
          }}
        />
      ) : null}
      {tabs.map((id) => (
        <div key={id} className="k-doc" hidden={id !== active}>
          <Reader
            doc={DOCS[id]}
            start={start}
            shown={id === active}
            onSettings={() => {
              setSection("Appearance");
              setSettings("Appearance");
            }}
          />
        </div>
      ))}
      {settings ? (
        <SettingsModal
          section={section}
          onSection={setSection}
          onClose={() => setSettings(null)}
          autoCheck={autoCheck}
          setAutoCheck={setAutoCheck}
        />
      ) : null}
    </>
  );
}

export function KitabApp({ start, label }: { start: Start; label: string }) {
  return (
    <Frame label={label}>
      <Workspace start={start} />
    </Frame>
  );
}
