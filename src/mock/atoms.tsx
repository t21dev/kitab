// SPDX-License-Identifier: AGPL-3.0-or-later
// Small parts the mock app is built from, sized like the app at Compact density
// (DESIGN.md § Density): 28 px icon buttons, 30 px controls, 13 px UI text.
import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from "react";
import { ICONS, type KIconName } from "./icons";

export function KIcon({ name, size = 16, className = "" }: { name: KIconName; size?: number; className?: string }) {
  const raw: string = ICONS[name];
  const filled = raw.startsWith("F");
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`ki ${className}`}
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: filled ? raw.slice(1) : raw }}
    />
  );
}

type Btn = ButtonHTMLAttributes<HTMLButtonElement> & { icon: KIconName; label: string; on?: boolean; size?: number };

/** A title-bar or header icon button: muted glyph, hover fill, raised and accent when on. */
export function IconBtn({ icon, label, on, size = 15, className = "", ...rest }: Btn) {
  return (
    <button type="button" className={`k-ibtn ${className}`} data-on={on || undefined} aria-label={label} aria-pressed={on} title={label} {...rest}>
      <KIcon name={icon} size={size} />
    </button>
  );
}

/** A dock (or markup bar) button: 28 px, raised fill on hover, raised + accent when on. */
export function DockBtn({ icon, label, on, className = "", children, ...rest }: Btn & { children?: ReactNode }) {
  return (
    <button type="button" className={`k-dbtn ${className}`} data-on={on || undefined} aria-label={label} aria-pressed={on} title={label} {...rest}>
      <KIcon name={icon} size={16} />
      {children}
    </button>
  );
}

export function Switch({ on, label, onChange }: { on: boolean; label: string; onChange: () => void }) {
  return (
    <button type="button" role="switch" aria-checked={on} aria-label={label} className="k-switch" data-on={on || undefined} onClick={onChange}>
      <i />
    </button>
  );
}

export function Progress({ value }: { value: number }) {
  return (
    <span className="k-prog">
      <i style={{ width: `${Math.round(Math.max(0, Math.min(1, value)) * 100)}%` }} />
    </span>
  );
}

/** FNV-1a over the title's bytes, as the app hashes a title into a cover hue. */
function hue(title: string): number {
  let h = 2166136261;
  for (const b of new TextEncoder().encode(title)) h = Math.imul(h ^ b, 16777619) >>> 0;
  return h % 360;
}

/** A generated cover (DESIGN.md § Covers): muted hue from the title, its initial, a spine. */
export function Cover({ title, width }: { title: string; width: number }) {
  const initial = [...title].find((c) => /[\p{L}\p{N}]/u.test(c))?.toUpperCase() ?? "";
  return (
    <span className="k-cover" style={{ "--h": hue(title), width, height: width * 1.5, fontSize: width * 0.42 } as CSSProperties}>
      <i style={{ width: Math.max(3, width * 0.035) }} />
      <b>{initial}</b>
    </span>
  );
}
