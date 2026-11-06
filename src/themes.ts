// SPDX-License-Identifier: AGPL-3.0-or-later
// The site uses the app's Light and Dark themes (DESIGN.md § Colour on main).
// It follows the system setting; the theme button overrides it per visitor.

export type SiteTheme = "light" | "dark";

type Tokens = Record<
  | "bg" | "panel" | "desk" | "raised" | "hover" | "line" | "text" | "read" | "muted" | "faint" | "ring"
  | "accent" | "highlight" | "ok" | "warn" | "bad" | "on-accent"
  | "elev" | "well" | "float" | "sheet",
  string
>;

export const TOKENS: Record<SiteTheme, Tokens> = {
  light: {
    bg: "#FFFFFF", panel: "#FAFAFB", desk: "#F2F2F4", raised: "#FFFFFF", hover: "#F3F3F5", line: "#E8E8EC",
    text: "#18181B", read: "#232326", muted: "#55555D", faint: "#6B6B73", ring: "#B5B5BC",
    // DESIGN.md: Light's accent is 3.64:1 on white, fine for marks; text uses this darker step (4.5:1+).
    accent: "#B7791F", highlight: "#FBE2A6", ok: "#1F9D63", warn: "#B7791F", bad: "#D33B3B", "on-accent": "#FFFFFF",
    elev: "0 0 0 1px rgba(20,20,40,.08), 0 1px 2px rgba(20,20,40,.08), 0 2px 6px rgba(20,20,40,.05)",
    well: "inset 0 1px 2px rgba(20,20,40,.06), inset 0 0 0 1px rgba(20,20,40,.08)",
    float: "0 0 0 1px rgba(20,20,40,.08), 0 4px 12px rgba(20,20,40,.08), 0 20px 40px rgba(20,20,40,.10)",
    sheet: "0 0 0 1px rgba(20,20,40,.08), 0 2px 6px rgba(20,20,40,.08), 0 12px 28px rgba(20,20,40,.10)",
  },
  dark: {
    bg: "#141415", panel: "#111112", desk: "#0F0F10", raised: "#1E1E21", hover: "#18181A", line: "#27272B",
    text: "#ECECEE", read: "#D9D9DD", muted: "#A1A1A7", faint: "#8C8C92", ring: "#5C5C63",
    accent: "#E6B450", highlight: "rgba(230,180,80,.30)", ok: "#4CC38A", warn: "#E6B450", bad: "#F06A6A", "on-accent": "#141415",
    elev: "inset 0 1px 0 rgba(255,255,255,.07), 0 0 0 1px rgba(0,0,0,.5), 0 2px 6px rgba(0,0,0,.35)",
    well: "inset 0 1px 2px rgba(0,0,0,.45), inset 0 0 0 1px rgba(255,255,255,.05)",
    float: "inset 0 1px 0 rgba(255,255,255,.08), 0 0 0 1px rgba(0,0,0,.6), 0 8px 20px rgba(0,0,0,.4), 0 24px 48px rgba(0,0,0,.35)",
    sheet: "inset 0 0 0 1px rgba(255,255,255,.06), 0 0 0 1px rgba(0,0,0,.6), 0 4px 10px rgba(0,0,0,.35), 0 16px 32px rgba(0,0,0,.3)",
  },
};

const LIGHT_ACCENT_TEXT = "#94600F"; // accent used as text on Light (≥ 4.5:1 on white)

function block(t: SiteTheme): string {
  const vars = Object.entries(TOKENS[t]).map(([k, v]) => `--${k}:${v};`).join("");
  const accentText = t === "light" ? LIGHT_ACCENT_TEXT : TOKENS.dark.accent;
  return `${vars}--accent-text:${accentText};color-scheme:${t};`;
}

/** Light on :root, Dark when the system asks for it, and the visitor's choice (data-theme) beats both. */
export function themeCss(): string {
  return [
    `:root{${block("light")}}`,
    `@media (prefers-color-scheme: dark){:root:not([data-theme="light"]){${block("dark")}}}`,
    `:root[data-theme="dark"]{${block("dark")}}`,
    `:root[data-theme="light"]{${block("light")}}`,
  ].join("\n");
}
