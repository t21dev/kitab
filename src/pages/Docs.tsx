// SPDX-License-Identifier: AGPL-3.0-or-later
// /docs: every feature, in sections and topics (src/docs-content.ts), with an instant search.
// Without JavaScript the whole page is there and the search box stays hidden; once React
// hydrates, the box appears: "/" focuses it, typing filters the topics and marks the words
// (every word must match the start of a word in the topic), Up and Down pick a result,
// Enter jumps to it, Esc clears. ?q= in the address opens the page searched.
import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { PALETTE, type KeyGroup } from "../app-keys";
import { Icon } from "../components/Icon";
import { PageHeader } from "../components/Layout";
import { DOCS, plain, type Block, type DocSection, type Topic } from "../docs-content";
import { KitabApp } from "../mock/KitabApp";

// ---------------------------------------------------------------- search

/** The words of a block, for search. */
function blockText(b: Block): string {
  if ("p" in b) return plain(b.p);
  if ("note" in b) return plain(b.note);
  if ("list" in b) return b.list.map(plain).join(" ");
  if ("img" in b) return b.img.caption ?? "";
  if ("table" in b) return [...b.table.head, ...b.table.rows.flat()].map(plain).join(" ");
  if ("code" in b) return b.code;
  if ("keytable" in b) return b.keytable.rows.flat().join(" ");
  if ("palette" in b) return PALETTE.map((c) => `${c.label} ${c.keys.join(" ")}`).join(" ");
  return "";
}

type Entry = { section: DocSection; topic: Topic; hay: string; text: string };

const INDEX: Entry[] = DOCS.flatMap((section) =>
  section.topics.map((topic) => {
    const text = topic.blocks.map(blockText).join(" ").replace(/\s+/g, " ");
    return { section, topic, text, hay: `${section.title} ${topic.title} ${text}`.toLowerCase() };
  }),
);

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const words = (q: string) => q.toLowerCase().split(/\s+/).filter(Boolean);
/** Each word matches at the start of a word ("tab" finds tabs, not Kitab; "sign" finds signature);
 * a one-letter word ("g" in "ctrl g") only on its own. */
const pattern = (w: string) => `(?<![\\p{L}\\p{N}])${esc(w)}${w.length === 1 ? "(?![\\p{L}\\p{N}])" : ""}`;

/** Best first: the whole query in a title, then every word in the title, then the whole query in the text. */
function score(e: Entry, phrase: string, tests: RegExp[]): number {
  const title = e.topic.title.toLowerCase();
  let s = 0;
  if (title.includes(phrase)) s += 8;
  if (tests.every((t) => t.test(title))) s += 4;
  if (e.section.title.toLowerCase().includes(phrase)) s += 2;
  if (e.hay.includes(phrase)) s += 1;
  return s;
}

function snippet(text: string, tests: RegExp[]): string {
  const at = Math.min(...tests.map((t) => text.search(t)).filter((i) => i >= 0));
  if (!Number.isFinite(at)) return text.slice(0, 110) + (text.length > 110 ? "…" : "");
  const start = Math.max(0, at - 40);
  const cut = text.slice(start, start + 120);
  return `${start > 0 ? "…" : ""}${cut}${start + 120 < text.length ? "…" : ""}`;
}

// ---------------------------------------------------------------- inline text

/** Wrap matches of `re` in <mark>. */
function hl(s: string, re: RegExp | null, key: string): ReactNode {
  if (!re || !s) return s;
  const parts = s.split(re);
  if (parts.length === 1) return s;
  return parts.map((p, i) => (i % 2 ? <mark key={`${key}-${i}`}>{p}</mark> : <Fragment key={`${key}-${i}`}>{p}</Fragment>));
}

const INLINE = /\[\[([^\]]+)\]\]|`([^`]+)`|\*\*([^*]+)\*\*|\[([^\]]+)\]\(([^)]+)\)/g;

/** [[key]], `code`, **bold** and [label](href), with search matches marked. */
function Inline({ text, re }: { text: string; re: RegExp | null }) {
  const out: ReactNode[] = [];
  let last = 0;
  let n = 0;
  for (const m of text.matchAll(INLINE)) {
    const i = m.index ?? 0;
    if (i > last) out.push(hl(text.slice(last, i), re, `t${n++}`));
    const k = `i${n++}`;
    if (m[1] !== undefined) out.push(<Keys key={k} keys={m[1]} re={re} />);
    else if (m[2] !== undefined) out.push(<code key={k}>{hl(m[2], re, k)}</code>);
    else if (m[3] !== undefined) out.push(<strong key={k}>{hl(m[3], re, k)}</strong>);
    else out.push(<a key={k} href={m[5]}>{hl(m[4], re, k)}</a>);
    last = i + m[0].length;
  }
  if (last < text.length) out.push(hl(text.slice(last), re, `t${n++}`));
  return <>{out}</>;
}

/** A key cap is a modifier, a named key, F1–F12, one character, or a digit range. */
const KEY = /^(Ctrl|Shift|Alt|Win|Esc|Tab|Enter|Space|Home|End|PgUp|PgDn|Up|Down|Left|Right|Del|Delete|Backspace|F\d{1,2}|.|\d–\d)$/;

/** "Ctrl+Shift+T" as key caps; words such as "wheel" or "Drag on the page" stay text. */
function Keys({ keys, re }: { keys: string; re: RegExp | null }) {
  if (keys.includes(" ") || keys.length > 1 && !keys.includes("+") && !KEY.test(keys)) return <span className="docs__mouse">{hl(keys, re, keys)}</span>;
  // "Ctrl++" would be Ctrl and +; none of the defaults use it, but split safely anyway.
  const parts = keys.endsWith("++") ? [...keys.slice(0, -2).split("+"), "+"] : keys === "+" ? ["+"] : keys.split("+");
  return (
    <span className="keys">
      {parts.map((p, i) => (
        <Fragment key={i}>
          {i > 0 ? <span className="keys__plus">+</span> : null}
          {KEY.test(p) ? <kbd>{hl(p, re, `${keys}${i}`)}</kbd> : <span className="docs__mouse">{hl(p, re, `${keys}${i}`)}</span>}
        </Fragment>
      ))}
    </span>
  );
}

/** "Ctrl+W / Ctrl+F4" → each alternative as caps, "or" between. "Ctrl+F / /" keeps the slash key. */
function KeyAlternatives({ keys, re }: { keys: string; re: RegExp | null }) {
  const alts = keys.split(" / ");
  return (
    <span className="keyalts">
      {alts.map((a, i) => (
        <Fragment key={i}>
          {i > 0 ? <span className="keyalts__or">or</span> : null}
          <Keys keys={a} re={re} />
        </Fragment>
      ))}
    </span>
  );
}

// ---------------------------------------------------------------- blocks

function KeyTable({ group, re }: { group: KeyGroup; re: RegExp | null }) {
  return (
    <div className="docs__table">
      <table>
        <thead>
          <tr>
            <th scope="col">Action</th>
            <th scope="col">Keys</th>
          </tr>
        </thead>
        <tbody>
          {group.rows.map(([what, keys]) => (
            <tr key={what}>
              <td>{hl(what, re, what)}</td>
              <td>
                <KeyAlternatives keys={keys} re={re} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PaletteTable({ re }: { re: RegExp | null }) {
  return (
    <div className="docs__table">
      <table>
        <thead>
          <tr>
            <th scope="col">Group</th>
            <th scope="col">Command</th>
            <th scope="col">Keys</th>
          </tr>
        </thead>
        <tbody>
          {PALETTE.map((c) => (
            <tr key={c.label}>
              <td className="docs__faint">{c.group}</td>
              <td>
                {hl(c.label, re, c.label)}
                {c.doc ? null : <span className="docs__tag">also on Home</span>}
              </td>
              <td>{c.keys.length ? <KeyAlternatives keys={c.keys.join(" / ")} re={re} /> : <span className="docs__faint">None by default</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function BlockView({ b, re }: { b: Block; re: RegExp | null }) {
  if ("p" in b)
    return (
      <p>
        <Inline text={b.p} re={re} />
      </p>
    );
  if ("note" in b)
    return (
      <p className="docs__note">
        <Inline text={b.note} re={re} />
      </p>
    );
  if ("list" in b) {
    const items = b.list.map((t) => (
      <li key={t}>
        <Inline text={t} re={re} />
      </li>
    ));
    return b.ordered ? <ol>{items}</ol> : <ul>{items}</ul>;
  }
  if ("img" in b)
    return (
      <figure className="docs__shot">
        <a href={b.img.src} aria-label={`${b.img.alt} (full size)`}>
          <img src={b.img.src} width={b.img.w} height={b.img.h} alt={b.img.alt} loading="lazy" decoding="async" />
        </a>
        {b.img.caption ? <figcaption>{hl(b.img.caption, re, "cap")}</figcaption> : null}
      </figure>
    );
  if ("table" in b)
    return (
      <div className="docs__table">
        <table>
          <thead>
            <tr>
              {b.table.head.map((h) => (
                <th key={h} scope="col">
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {b.table.rows.map((r) => (
              <tr key={r[0]}>
                {r.map((c, i) => (
                  <td key={i}>
                    <Inline text={c} re={re} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  if ("code" in b)
    return (
      <pre className="docs__code">
        <code>{hl(b.code, re, "code")}</code>
      </pre>
    );
  if ("keytable" in b) return <KeyTable group={b.keytable} re={re} />;
  if ("palette" in b) return <PaletteTable re={re} />;
  if ("mock" in b)
    return (
      <figure className="docs__mock">
        <div className="docs__mockframe">
          <KitabApp
            start={{ tabs: ["welcome"], active: "welcome", sidebar: true, settings: "Appearance" }}
            label="Kitab's Settings, Appearance, rebuilt in HTML: pick a theme and the window follows it"
          />
        </div>
        <figcaption>Kitab rebuilt in HTML from its own theme colours. Pick a theme; close Settings to see the page in it.</figcaption>
      </figure>
    );
  return null;
}

// ---------------------------------------------------------------- page

export function Docs() {
  const [q, setQ] = useState("");
  const [ready, setReady] = useState(false);
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  // Show the search box only once React runs it; read ?q= from a shared link.
  useEffect(() => {
    setReady(true);
    const fromUrl = new URLSearchParams(location.search).get("q");
    if (fromUrl) setQ(fromUrl);
  }, []);

  // "/" focuses the search box from anywhere on the page, unless you are typing somewhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "/" || e.ctrlKey || e.metaKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
      e.preventDefault();
      input.current?.focus();
      input.current?.select();
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);

  // Keep the query in the address, so a search can be shared; leave the hash alone.
  useEffect(() => {
    if (!ready) return;
    const url = new URL(location.href);
    if (q.trim()) url.searchParams.set("q", q.trim());
    else url.searchParams.delete("q");
    history.replaceState(history.state, "", url);
  }, [q, ready]);

  const w = useMemo(() => words(q), [q]);
  const tests = useMemo(() => w.map((x) => new RegExp(pattern(x), "iu")), [w]);
  const re = useMemo(() => (w.length ? new RegExp(`(${w.map(pattern).join("|")})`, "giu") : null), [w]);
  const hits = useMemo(() => {
    if (!w.length) return [];
    const phrase = w.join(" ");
    return INDEX.filter((e) => tests.every((t) => t.test(e.hay)))
      .map((e, i) => ({ e, i, s: score(e, phrase, tests) }))
      .sort((a, b) => b.s - a.s || a.i - b.i)
      .map((x) => x.e);
  }, [w, tests]);
  const shown = useMemo(() => new Set(hits.map((h) => h.topic.id)), [hits]);
  useEffect(() => setSel(0), [q]);

  const searching = w.length > 0;

  const jump = (id: string) => {
    const el = document.getElementById(id);
    if (!el) return;
    el.scrollIntoView({ block: "start" });
    history.replaceState(history.state, "", `${location.pathname}${location.search}#${id}`);
    // Move focus to the topic's heading, so keyboard and screen reader users land there too.
    el.querySelector<HTMLElement>("h3")?.focus({ preventScroll: true });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      if (q) {
        e.preventDefault();
        setQ("");
      } else input.current?.blur();
    } else if (e.key === "ArrowDown" && hits.length) {
      e.preventDefault();
      setSel((s) => Math.min(hits.length - 1, s + 1));
    } else if (e.key === "ArrowUp" && hits.length) {
      e.preventDefault();
      setSel((s) => Math.max(0, s - 1));
    } else if (e.key === "Enter" && hits[sel]) {
      e.preventDefault();
      jump(hits[sel].topic.id);
    }
  };

  const activeId = searching && hits[sel] ? `docs-hit-${sel}` : undefined;

  return (
    <>
      <PageHeader
        kicker="Docs"
        title="Kitab Reader documentation"
        lead="How to use every part of Kitab: reading, Markdown and books, markup, forms, signatures, page tools, printing, settings and every shortcut. The pictures are screenshots of Kitab 0.3.4."
      />
      <div className="docs wrap">
        <aside className="docs__side" aria-label="Documentation contents and search">
          <div className="docs__search" hidden={!ready}>
            <label htmlFor="docs-q" className="docs__label">
              Search the docs
            </label>
            <div className="docs__field">
              <Icon name="search" size={15} />
              <input
                ref={input}
                id="docs-q"
                type="search"
                autoComplete="off"
                spellCheck={false}
                placeholder="Try signature or Ctrl+G"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                onKeyDown={onKeyDown}
                role="combobox"
                aria-expanded={searching}
                aria-controls="docs-results"
                aria-activedescendant={activeId}
                aria-describedby="docs-count"
              />
              {q ? (
                <button type="button" className="docs__clear" aria-label="Clear the search" onClick={() => (setQ(""), input.current?.focus())}>
                  <Icon name="close" size={14} />
                </button>
              ) : (
                <kbd aria-hidden="true">/</kbd>
              )}
            </div>
            <p id="docs-count" className="docs__count" aria-live="polite">
              {searching ? (hits.length ? `${hits.length} ${hits.length === 1 ? "topic matches" : "topics match"}. Enter goes to the first.` : "Nothing matches. Try fewer or other words.") : ""}
            </p>
          </div>

          {searching ? (
            <ol id="docs-results" className="docs__results" role="listbox" aria-label="Matching topics">
              {hits.map((h, i) => (
                <li key={h.topic.id} id={`docs-hit-${i}`} role="option" aria-selected={i === sel}>
                  <a
                    href={`#${h.topic.id}`}
                    tabIndex={-1}
                    onClick={(e) => {
                      e.preventDefault();
                      jump(h.topic.id);
                    }}
                    onMouseEnter={() => setSel(i)}
                  >
                    <span className="docs__rtitle">{hl(h.topic.title, re, "rt")}</span>
                    <span className="docs__rsec">{h.section.title}</span>
                    <span className="docs__rsnip">{hl(snippet(h.text, tests), re, "rs")}</span>
                  </a>
                </li>
              ))}
            </ol>
          ) : (
            <nav className="docs__toc" aria-label="Documentation">
              <ol>
                {DOCS.map((s) => (
                  <li key={s.id}>
                    <a href={`#${s.id}`} className="docs__tocsec">
                      <Icon name={s.icon} size={15} />
                      {s.title}
                    </a>
                    {s.topics.length > 1 ? (
                      <ol>
                        {s.topics.map((t) => (
                          <li key={t.id}>
                            <a href={`#${t.id}`}>{t.title}</a>
                          </li>
                        ))}
                      </ol>
                    ) : null}
                  </li>
                ))}
              </ol>
            </nav>
          )}
        </aside>

        <div className="docs__body">
          {searching && !hits.length ? (
            <p className="docs__empty">
              No topic matches &ldquo;{q.trim()}&rdquo;. Every shortcut is under <a href="#keyboard" onClick={() => setQ("")}>Keyboard shortcuts</a>, and the{" "}
              <a href="#faq" onClick={() => setQ("")}>questions</a> are at the end.
            </p>
          ) : null}
          {DOCS.map((s) => {
            const any = !searching || s.topics.some((t) => shown.has(t.id));
            return (
              <section key={s.id} id={s.id} className="docs__section" aria-labelledby={`${s.id}-h`} hidden={!any}>
                <header className="docs__shead">
                  <span className="docs__icon">
                    <Icon name={s.icon} size={17} />
                  </span>
                  <div>
                    <h2 id={`${s.id}-h`}>{hl(s.title, re, "st")}</h2>
                    <p>{s.summary}</p>
                  </div>
                </header>
                {s.topics.map((t) => (
                  <section key={t.id} id={t.id} className="docs__topic" aria-labelledby={`${t.id}-h`} hidden={searching && !shown.has(t.id)}>
                    <h3 id={`${t.id}-h`} tabIndex={-1}>
                      <a href={`#${t.id}`} className="docs__anchor">
                        {hl(t.title, re, "tt")}
                      </a>
                    </h3>
                    {t.blocks.map((b, i) => (
                      <BlockView key={i} b={b} re={re} />
                    ))}
                  </section>
                ))}
              </section>
            );
          })}
        </div>
      </div>
    </>
  );
}
