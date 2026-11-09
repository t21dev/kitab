// SPDX-License-Identifier: AGPL-3.0-or-later
// Site search on Ctrl+K. Opened by keyboard many times a day by some visitors,
// so it gets a short opacity fade only (emil-design-eng: no motion on keyboard actions).
import { useEffect, useMemo, useRef, useState } from "react";
import { SITE, STEPS } from "../content";
import { DOCS } from "../docs-content";
import { Icon } from "./Icon";

type Item = { title: string; hint: string; href: string };

const ITEMS: Item[] = [
  ...STEPS.map((s) => ({ title: s.title, hint: "Feature", href: `/#${s.id}` })),
  { title: "Documentation", hint: "Page", href: "/docs" },
  ...DOCS.map((d) => ({ title: d.title, hint: "Docs", href: `/docs#${d.id}` })),
  { title: "Download and release status", hint: "Page", href: "/download" },
  { title: "Keyboard shortcuts", hint: "Page", href: "/shortcuts" },
  { title: "Changelog", hint: "Page", href: "/changelog" },
  { title: "Privacy", hint: "Page", href: "/privacy" },
  { title: "Roadmap", hint: "Section", href: "/#roadmap" },
  { title: "Questions", hint: "Section", href: "/#faq" },
  { title: "The plan", hint: "GitHub", href: SITE.plan },
  { title: "Source code", hint: "GitHub", href: SITE.repo },
  { title: "llms.txt", hint: "For machines", href: "/llms.txt" },
];

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const opener = useRef<Element | null>(null);

  const hits = useMemo(() => {
    const t = q.trim().toLowerCase();
    return t ? ITEMS.filter((i) => `${i.title} ${i.hint}`.toLowerCase().includes(t)) : ITEMS;
  }, [q]);

  useEffect(() => {
    if (open) {
      opener.current = document.activeElement;
      setQ("");
      setSel(0);
      requestAnimationFrame(() => input.current?.focus());
      document.documentElement.style.overflow = "hidden";
    } else {
      document.documentElement.style.overflow = "";
      (opener.current as HTMLElement | null)?.focus?.();
    }
  }, [open]);

  const go = (i: Item | undefined) => {
    if (!i) return;
    onClose();
    location.href = i.href;
  };

  return (
    <div className="cmdk" data-open={open || undefined} aria-hidden={!open}>
      <div className="cmdk__scrim" onClick={onClose} />
      <div className="cmdk__panel" role="dialog" aria-modal="true" aria-label="Search the site">
        <div className="cmdk__field">
          <Icon name="search" size={16} />
          <input
            ref={input}
            id="site-search"
            value={q}
            placeholder="Search the site"
            autoComplete="off"
            aria-controls="site-search-results"
            aria-activedescendant={hits[sel] ? `hit-${sel}` : undefined}
            onChange={(e) => {
              setQ(e.target.value);
              setSel(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") onClose();
              else if (e.key === "ArrowDown") {
                e.preventDefault();
                setSel((s) => (s + 1) % Math.max(1, hits.length));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSel((s) => (s - 1 + hits.length) % Math.max(1, hits.length));
              } else if (e.key === "Enter") go(hits[sel]);
            }}
          />
          <kbd>Esc</kbd>
        </div>
        <ul className="cmdk__list" id="site-search-results" role="listbox">
          {hits.length ? (
            hits.map((h, i) => (
              <li key={h.href + h.title} id={`hit-${i}`} role="option" aria-selected={i === sel}>
                <a href={h.href} onMouseEnter={() => setSel(i)} onClick={onClose}>
                  {h.title}
                  <small>{h.hint}</small>
                </a>
              </li>
            ))
          ) : (
            <li className="cmdk__empty">No match for “{q}”. Try “forms” or “privacy”.</li>
          )}
        </ul>
        <div className="cmdk__foot">
          <span>
            <kbd>↑</kbd> <kbd>↓</kbd> move
          </span>
          <span>
            <kbd>↵</kbd> open
          </span>
          <span>
            <kbd>Esc</kbd> close
          </span>
        </div>
      </div>
    </div>
  );
}
