// SPDX-License-Identifier: AGPL-3.0-or-later
// Settings (settings_view.rs) and the ⋯ menu (shell.rs more_menu). Picking a
// theme here re-themes every mock window on the page, as it would the app.
import { useEffect, useRef } from "react";
import { KIcon, Switch } from "./atoms";
import type { KIconName } from "./icons";
import { setMock, useMock, useSiteDark } from "./store";
import { KTHEMES, type KTheme } from "./theme-list";

export type Section = "Appearance" | "About";
const RAIL: [string, KIconName][] = [
  ["Appearance", "sun"],
  ["Reading", "book"],
  ["Library", "folder-open"],
  ["File types", "file"],
  ["Shortcuts", "keyboard"],
  ["Signing", "shield"],
  ["Advanced", "settings"],
  ["About", "info"],
];
const VERSION = "0.2.1";

function useCurrentTheme(): string {
  const { theme } = useMock();
  const dark = useSiteDark();
  return theme ?? (dark ? "dark" : "light");
}

function ThemePreview({ t }: { t: KTheme }) {
  const c = t.c;
  const line = (w: string, bg: string) => <i style={{ width: w, background: bg }} />;
  return (
    <span className="tp" style={{ background: c.bg }}>
      <span className="tp__bar" style={{ borderColor: c.line }}>
        <i style={{ background: c.raised }} />
        <b style={{ background: c.accent }} />
      </span>
      <span className="tp__body">
        <span className="tp__side" style={{ background: c.panel }}>
          {line("90%", c.accent)}
          {line("70%", c.muted)}
          {line("80%", c.faint)}
        </span>
        <span className="tp__desk" style={{ background: c.desk }}>
          <span className="tp__page">
            <i className="tp__h" />
            <i />
            <i style={{ width: "90%" }} />
            <i style={{ width: "95%" }} />
            <i style={{ width: "70%" }} />
          </span>
        </span>
      </span>
    </span>
  );
}

function Row({ icon, title, body, children }: { icon?: KIconName; title: string; body: string; children: React.ReactNode }) {
  return (
    <div className="ks-row">
      {icon ? <KIcon name={icon} size={18} className="ks-row__icon" /> : null}
      <div className="ks-row__text">
        <span>{title}</span>
        <small>{body}</small>
      </div>
      {children}
    </div>
  );
}

function Appearance() {
  const current = useCurrentTheme();
  const { compactDock, fadeDock } = useMock();
  return (
    <>
      <section className="ks-group">
        <h4>Theme</h4>
        <div className="ks-cards">
          {KTHEMES.map((t) => (
            <button
              key={t.id}
              type="button"
              className="ks-card"
              aria-pressed={current === t.id}
              data-on={current === t.id || undefined}
              onClick={() => setMock({ theme: t.id })}
            >
              <ThemePreview t={t} />
              <span className="ks-card__label">
                {t.name}
                {current === t.id ? <KIcon name="check" size={14} /> : null}
              </span>
            </button>
          ))}
        </div>
      </section>
      <section className="ks-group">
        <h4>Quick dock</h4>
        <p className="ks-note">Zoom, fit, find, rotate and more, floating over the page. At the bottom it also holds the page navigator.</p>
        <Row title="Compact dock" body="Only zoom, find and the ⋯ menu.">
          <Switch on={compactDock} label="Compact dock" onChange={() => setMock({ compactDock: !compactDock })} />
        </Row>
        <Row title="Fade the dock while reading" body="It comes back when you move the pointer or press a key.">
          <Switch on={fadeDock} label="Fade the dock while reading" onChange={() => setMock({ fadeDock: !fadeDock })} />
        </Row>
      </section>
    </>
  );
}

function About({ autoCheck, setAutoCheck }: { autoCheck: boolean; setAutoCheck: (v: boolean) => void }) {
  return (
    <>
      <div className="ks-about">
        <b>Kitab Reader</b>
        <span>Version {VERSION}</span>
        <p>Your library has a fast lane now. Rust underneath, a calm page on top. Free software by t21.</p>
      </div>
      <section className="ks-group">
        <h4>Updates</h4>
        <Row icon="download" title="Check for updates" body={`You have version ${VERSION}.`}>
          <span className="k-btn k-btn--sm">Check now</span>
        </Row>
        <Row
          icon="refresh"
          title="Check on start-up"
          body="Shortly after Kitab opens, and every six hours, it asks GitHub for the latest version number. Nothing about you or your files is sent."
        >
          <Switch on={autoCheck} label="Check on start-up" onChange={() => setAutoCheck(!autoCheck)} />
        </Row>
      </section>
      <section className="ks-group">
        <h4>Links</h4>
        <Row icon="shield" title="Privacy policy" body="No telemetry; your files stay on your computer.">
          <KIcon name="external" size={14} className="ks-row__icon" />
        </Row>
        <Row icon="scale" title="Licence" body="GNU Affero General Public License 3.0 or later.">
          <KIcon name="external" size={14} className="ks-row__icon" />
        </Row>
      </section>
    </>
  );
}

export function SettingsModal({
  section,
  onSection,
  onClose,
  autoCheck,
  setAutoCheck,
}: {
  section: Section;
  onSection: (s: Section) => void;
  onClose: () => void;
  autoCheck: boolean;
  setAutoCheck: (v: boolean) => void;
}) {
  return (
    <div className="ks-scrim" onMouseDown={onClose}>
      <div className="ks" role="dialog" aria-label="Settings" onMouseDown={(e) => e.stopPropagation()}>
        <nav className="ks-rail">
          <p className="ks-rail__title">Settings</p>
          {RAIL.map(([name, icon]) => {
            const live = name === "Appearance" || name === "About";
            return (
              <button
                key={name}
                type="button"
                className="ks-rail__item"
                data-on={section === name || undefined}
                aria-current={section === name ? "page" : undefined}
                disabled={!live}
                onClick={() => live && onSection(name as Section)}
              >
                <KIcon name={icon} size={16} />
                {name}
              </button>
            );
          })}
        </nav>
        <div className="ks-main">
          <div className="ks-head">
            <h3>{section}</h3>
            <button type="button" className="k-ibtn" aria-label="Close settings" title="Close  Esc" onClick={onClose}>
              <KIcon name="close" size={14} />
            </button>
          </div>
          <div className="ks-body">{section === "Appearance" ? <Appearance /> : <About autoCheck={autoCheck} setAutoCheck={setAutoCheck} />}</div>
        </div>
      </div>
    </div>
  );
}

export type MenuAction = "contents" | "goto" | "settings";

/** The ⋯ menu: a theme row of swatches, then the everyday rows. */
export function MoreMenu({ onAction, onClose }: { onAction: (a: MenuAction) => void; onClose: () => void }) {
  const current = useCurrentTheme();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const down = (e: PointerEvent) => ref.current && !ref.current.contains(e.target as Node) && onClose();
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    // Next tick, so the click that opened it doesn't close it.
    const t = window.setTimeout(() => document.addEventListener("pointerdown", down), 0);
    document.addEventListener("keydown", key);
    return () => {
      window.clearTimeout(t);
      document.removeEventListener("pointerdown", down);
      document.removeEventListener("keydown", key);
    };
  }, [onClose]);
  const row = (a: MenuAction | null, icon: KIconName, label: string, key: string) => (
    <button type="button" role="menuitem" className="km-row" onClick={() => (a ? onAction(a) : onClose())}>
      <KIcon name={icon} size={16} />
      <span>{label}</span>
      <small>{key}</small>
    </button>
  );
  return (
    <div ref={ref} className="km" role="menu" aria-label="More">
      <p className="km-head">Theme</p>
      <div className="km-swatches">
        {KTHEMES.map((t) => (
          <button
            key={t.id}
            type="button"
            className="km-swatch"
            title={t.name}
            aria-label={`${t.name} theme`}
            aria-pressed={current === t.id}
            data-on={current === t.id || undefined}
            style={{ background: t.c.bg, borderColor: current === t.id ? undefined : t.c.line }}
            onClick={() => setMock({ theme: t.id })}
          >
            <i style={{ background: t.c.accent }} />
          </button>
        ))}
      </div>
      <hr />
      {row("contents", "sidebar", "Contents", "F12")}
      {row("goto", "hash", "Go to page…", "Ctrl+G")}
      <hr />
      {row(null, "print", "Print…", "Ctrl+P")}
      {row("settings", "settings", "Settings…", "Ctrl+,")}
    </div>
  );
}
