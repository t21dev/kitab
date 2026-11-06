// SPDX-License-Identifier: AGPL-3.0-or-later
// Hairline icons (DESIGN.md § Icons): 24 grid, 1.4 px stroke, round caps, currentColor.

const PATHS = {
  logo: "M12 6.6C10.2 5.3 7.7 4.6 4.5 4.6v13c3.2 0 5.7.7 7.5 2 1.8-1.3 4.3-2 7.5-2v-13c-3.2 0-5.7.7-7.5 2zM12 6.6v13",
  home: "M4 10.5 12 4l8 6.5V19a1 1 0 0 1-1 1h-4.5v-5.5h-5V20H5a1 1 0 0 1-1-1z",
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.2 15.2 20 20",
  more: "M6 12h.01M12 12h.01M18 12h.01",
  github:
    "M9 19c-4 1.3-4-2-5.6-2.5M14.6 21v-3.4c0-1 .1-1.4-.5-2 2.8-.3 5.4-1.4 5.4-6a4.7 4.7 0 0 0-1.3-3.2 4.3 4.3 0 0 0-.1-3.2s-1-.3-3.4 1.3a11.7 11.7 0 0 0-6.1 0C6.2 2.9 5.2 3.2 5.2 3.2a4.3 4.3 0 0 0-.1 3.2A4.7 4.7 0 0 0 3.8 9.6c0 4.6 2.6 5.7 5.4 6-.6.6-.6 1.2-.5 2V21",
  download: "M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19.5h14",
  arrow: "M5 12h14M13 6l6 6-6 6",
  check: "M5 12.5 10 17l9-10",
  sun: "M12 16a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4",
  moon: "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z",
  palette:
    "M12 20a8 8 0 1 1 8-8c0 2.2-1.8 3-3.5 3H15a1.6 1.6 0 0 0-1.2 2.7c.5.6.2 2.3-1.8 2.3zM7.5 12.5h.01M9.5 8h.01M14.5 8h.01",
  bolt: "M13 3 5 13.5h6L10.5 21 19 10.5h-6z",
  file: "M14 3.5H7a1.5 1.5 0 0 0-1.5 1.5v14A1.5 1.5 0 0 0 7 20.5h10a1.5 1.5 0 0 0 1.5-1.5V8zM14 3.5V8h4.5",
  markdown: "M4 6.5h16v11H4zM7 14.5v-5l2 2.5 2-2.5v5M15.5 9.5v5M13.5 12.5l2 2 2-2",
  tabs: "M3.5 8.5h17V19a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1zM3.5 8.5V5.5a1 1 0 0 1 1-1h5l1.5 4M11 8.5l-1.5-4",
  library: "M5 4.5v15M9 4.5v15M13 5.5l3.8 14M5 4.5h4M5 19.5h4",
  find: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.2 15.2 20 20M8 10.5h5",
  pages: "M4 6h7v12H4zM14 8h6v8h-6z",
  highlight: "M4 20h7M14.5 4.5l5 5L11 18H6v-5z",
  shield: "M12 3.5 5 6v5.5c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6zM9 12l2.2 2.2L15.5 10",
  tools: "M14.5 6.5a4 4 0 0 0-5.3 5.3L4 17l3 3 5.2-5.2a4 4 0 0 0 5.3-5.3l-2.5 2.5-2.5-.5-.5-2.5z",
  print: "M7 9V4h10v5M7 17H5a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1h-2M7 14h10v6H7z",
  keyboard: "M3.5 6.5h17v11h-17zM7 10h.01M10.3 10h.01M13.7 10h.01M17 10h.01M8 14h8",
  lock: "M6.5 10.5h11v9h-11zM8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5",
  outline: "M5 6h14M8 12h11M8 18h11M5 12h.01M5 18h.01",
  close: "M6 6l12 12M18 6 6 18",
  monitor: "M3.5 5.5h17v11h-17zM9 20h6M12 16.5V20",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16, className = "", title }: { name: IconName; size?: number; className?: string; title?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden={title ? undefined : true}
      role={title ? "img" : undefined}
    >
      {title ? <title>{title}</title> : null}
      <path d={PATHS[name]} />
    </svg>
  );
}

export const FEATURE_ICONS: Record<string, IconName> = {
  speed: "bolt",
  forms: "shield",
  fast: "bolt",
  formats: "file",
  markdown: "markdown",
  tabs: "tabs",
  home: "library",
  find: "find",
  pages: "pages",
  notes: "highlight",
  sign: "shield",
  tools: "tools",
  print: "print",
  keyboard: "keyboard",
  themes: "palette",
  ultra: "bolt",
  private: "lock",
};
