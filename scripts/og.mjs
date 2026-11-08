// SPDX-License-Identifier: AGPL-3.0-or-later
// Writes the share image and icons into public/. Run with `npm run og` after a
// new capture lands; the outputs are committed, so builds don't need fonts.
//   public/og.png (1200 × 630), favicon.svg, apple-touch-icon.png, icon-192.png, icon-512.png, site.webmanifest
// The OG image uses a real capture of the app (scripts/og/capture-dark-pages.png; it is not published on the site itself).
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Resvg } from "@resvg/resvg-js";
import wawoff2 from "wawoff2";

const root = new URL("../", import.meta.url);
const pub = new URL("public/", root);
const read = (p) => readFileSync(new URL(p, root));

// resvg needs TTF/OTF; fontsource ships WOFF2. Decompress into a temp dir.
const tmp = mkdtempSync(join(tmpdir(), "kitab-og-"));
const fonts = [];
for (const [name, file] of [
  ["inter.ttf", "node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2"],
  ["literata.ttf", "node_modules/@fontsource-variable/literata/files/literata-latin-standard-normal.woff2"],
]) {
  const ttf = await wawoff2.decompress(read(file));
  const out = join(tmp, name);
  writeFileSync(out, ttf);
  fonts.push(out);
}

const C = { bg: "#141415", raised: "#1E1E21", line: "#27272B", text: "#ECECEE", muted: "#A1A1A7", faint: "#8C8C92", accent: "#E6B450", desk: "#0F0F10" };
const MARK = "M12 6.6C10.2 5.3 7.7 4.6 4.5 4.6v13c3.2 0 5.7.7 7.5 2 1.8-1.3 4.3-2 7.5-2v-13c-3.2 0-5.7.7-7.5 2zM12 6.6v13";

function render(svg, width) {
  return new Resvg(svg, {
    fitTo: { mode: "width", value: width },
    font: { fontFiles: fonts, loadSystemFonts: false, defaultFontFamily: "Inter" },
  }).render().asPng();
}

// ── OG image ─────────────────────────────────────────────────────────────
const shot = read("scripts/og/capture-dark-pages.png").toString("base64");
const og = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="${C.bg}"/>
  <g transform="translate(72 74)">
    <rect width="52" height="52" rx="14" fill="${C.raised}" stroke="${C.line}"/>
    <g transform="translate(12 12) scale(1.2)"><path d="${MARK}" fill="none" stroke="${C.accent}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></g>
    <text x="70" y="36" font-family="Inter" font-weight="600" font-size="28" fill="${C.text}">Kitab Reader</text>
  </g>
  <text x="72" y="268" font-family="Literata" font-weight="600" font-size="66" letter-spacing="-1.4" fill="${C.text}">The most beautiful</text>
  <text x="72" y="348" font-family="Literata" font-weight="600" font-size="66" letter-spacing="-1.4" fill="${C.faint}">PDF reader.</text>
  <text x="72" y="430" font-family="Inter" font-size="24" fill="${C.muted}">Modern, native and fast by design.</text>
  <text x="72" y="464" font-family="Inter" font-size="24" fill="${C.muted}">PDF, EPUB, Markdown, comics. Free and open source.</text>
  <text x="72" y="560" font-family="Inter" font-weight="500" font-size="20" fill="${C.accent}">kitab.t21.dev</text>
  <clipPath id="frame"><rect x="760" y="70" width="380" height="490" rx="18"/></clipPath>
  <rect x="760" y="70" width="380" height="490" rx="18" fill="${C.desk}" stroke="${C.line}"/>
  <image clip-path="url(#frame)" x="680" y="70" width="540" height="539" preserveAspectRatio="xMidYMin slice" xlink:href="data:image/png;base64,${shot}"/>
</svg>`;
writeFileSync(new URL("og.png", pub), render(og, 1200));

// ── Icons ────────────────────────────────────────────────────────────────
const icon = (pad) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <rect width="64" height="64" rx="${pad ? 14 : 16}" fill="${C.bg}"/>
  <g transform="translate(${pad ? 14 : 12} ${pad ? 14 : 12}) scale(${pad ? 1.5 : 1.67})"><path d="${MARK}" fill="none" stroke="${C.accent}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></g>
</svg>`;
writeFileSync(new URL("favicon.svg", pub), icon(false));
writeFileSync(new URL("apple-touch-icon.png", pub), render(icon(true), 180));
writeFileSync(new URL("icon-192.png", pub), render(icon(true), 192));
writeFileSync(new URL("icon-512.png", pub), render(icon(true), 512));
writeFileSync(
  new URL("site.webmanifest", pub),
  JSON.stringify(
    {
      name: "Kitab Reader",
      short_name: "Kitab",
      description: "The most beautiful, modern PDF reader, built for performance.",
      start_url: "/",
      display: "browser",
      background_color: C.bg,
      theme_color: C.bg,
      icons: [
        { src: "/icon-192.png", sizes: "192x192", type: "image/png" },
        { src: "/icon-512.png", sizes: "512x512", type: "image/png" },
      ],
    },
    null,
    2,
  ) + "\n",
);
console.log("wrote og.png, favicon.svg, apple-touch-icon.png, icon-192.png, icon-512.png, site.webmanifest");
