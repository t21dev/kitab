// SPDX-License-Identifier: AGPL-3.0-or-later
import { useEffect, useRef, useState, type ReactNode } from "react";
import { SITE } from "../content";
import type { SiteTheme } from "../themes";
import { Icon } from "./Icon";
import { CommandPalette } from "./CommandPalette";

export const THEME_KEY = "kitab-theme";
type Choice = SiteTheme | "system";

function readChoice(): Choice {
  const t = document.documentElement.getAttribute("data-theme");
  return t === "light" || t === "dark" ? t : "system";
}

function applyChoice(c: Choice) {
  const root = document.documentElement;
  if (c === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", c);
  try {
    if (c === "system") localStorage.removeItem(THEME_KEY);
    else localStorage.setItem(THEME_KEY, c);
  } catch {
    // Storage may be blocked; the choice still applies to this page view.
  }
}

const THEME_ITEMS: { id: Choice; label: string; icon: "sun" | "moon" | "monitor" }[] = [
  { id: "system", label: "Match system", icon: "monitor" },
  { id: "light", label: "Light", icon: "sun" },
  { id: "dark", label: "Dark", icon: "moon" },
];

function ThemeMenu() {
  const [open, setOpen] = useState(false);
  const [choice, setChoice] = useState<Choice>("system");
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => setChoice(readChoice()), []);
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    const key = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", down);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  const current = THEME_ITEMS.find((i) => i.id === choice) ?? THEME_ITEMS[0];
  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        className="iconbtn"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Theme: ${current.label}`}
        title="Theme"
        onClick={() => setOpen((o) => !o)}
      >
        <Icon name={current.icon} size={16} />
      </button>
      <div role="menu" aria-label="Theme" className="menu" data-open={open || undefined}>
        {THEME_ITEMS.map((i) => (
          <button
            key={i.id}
            type="button"
            role="menuitemradio"
            aria-checked={choice === i.id}
            className="menu__item"
            onClick={() => {
              applyChoice(i.id);
              setChoice(i.id);
              setOpen(false);
            }}
          >
            <Icon name={i.icon} size={15} />
            <span>{i.label}</span>
            {choice === i.id ? <Icon name="check" size={14} className="menu__check" /> : null}
          </button>
        ))}
      </div>
    </div>
  );
}

function Header({ onSearch }: { onSearch: () => void }) {
  return (
    <header className="nav">
      <div className="wrap nav__in">
        <a href="/" className="brand" aria-label="Kitab Reader home">
          <span className="brand__mark">
            <Icon name="logo" size={16} />
          </span>
          Kitab
        </a>
        <button type="button" className="pill" onClick={onSearch} aria-label="Search the site (Ctrl K)" aria-haspopup="dialog">
          <Icon name="search" size={15} />
          <span className="pill__text">Formats, shortcuts, privacy…</span>
          <kbd>Ctrl K</kbd>
        </button>
        <nav className="nav__links" aria-label="Main">
          <a href="/#features">Features</a>
          <a href="/shortcuts">Shortcuts</a>
          <a href="/download">Download</a>
        </nav>
        <div className="nav__end">
          <ThemeMenu />
          <a className="btn btn--raised btn--sm" href={SITE.repo}>
            <Icon name="github" size={15} />
            <span className="hide-xs">GitHub</span>
          </a>
        </div>
      </div>
    </header>
  );
}

function Footer() {
  return (
    <footer className="colophon">
      <div className="wrap colophon__in">
        <nav aria-label="Footer" className="colophon__links">
          <a href="/download">Download</a>
          <a href="/shortcuts">Shortcuts</a>
          <a href="/changelog">Changelog</a>
          <a href="/privacy">Privacy</a>
          <a href={SITE.plan}>The plan</a>
          <a href={SITE.repo}>Source</a>
          <a href="/llms.txt">llms.txt</a>
        </nav>
        <p>
          Kitab Reader. Free software under the AGPL-3.0-or-later. Uses MuPDF by Artifex (AGPL). Set in Inter and Literata. Made by{" "}
          <a href={SITE.org.url}>Team 21</a> and <a href={SITE.author.github}>TriptoAfsin</a>. This site sets no cookies, runs no
          analytics and loads nothing from other servers.
        </p>
      </div>
    </footer>
  );
}

export function Layout({ children }: { children: ReactNode }) {
  const [searching, setSearching] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearching((s) => !s);
      }
    };
    addEventListener("keydown", onKey);
    return () => removeEventListener("keydown", onKey);
  }, []);
  return (
    <div className="shell">
      <a href="#main" className="skip">
        Skip to content
      </a>
      <Header onSearch={() => setSearching(true)} />
      <main id="main">{children}</main>
      <Footer />
      <CommandPalette open={searching} onClose={() => setSearching(false)} />
    </div>
  );
}

export function PageHeader({ kicker, title, lead }: { kicker: string; title: string; lead?: ReactNode }) {
  return (
    <div className="pagehead">
      <div className="wrap">
        <p className="kicker">{kicker}</p>
        <h1>{title}</h1>
        {lead ? <p className="lede">{lead}</p> : null}
      </div>
    </div>
  );
}
