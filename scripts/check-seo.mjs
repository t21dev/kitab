// SPDX-License-Identifier: AGPL-3.0-or-later
// `npm run check`, after `npm run build`: checks the prerendered site in dist/ the way a
// crawler would see it. Fails (exit 1) on anything that would hurt search, sharing or
// accessibility: a missing title, description, canonical or Open Graph tag, not exactly one
// h1, a skipped heading level, an image without alt, width or height, a duplicate id, a
// link to a page or anchor that doesn't exist, a page missing from the sitemap or llms.txt,
// or a missing Markdown mirror. The docs page is also held to the copy rules.
import { existsSync, readFileSync, readdirSync } from "node:fs";

const dist = new URL("../dist/", import.meta.url);
const read = (p) => readFileSync(new URL(p, dist), "utf8");
const SITE = "https://kitab.t21.dev";

if (!existsSync(new URL("index.html", dist))) {
  console.error("dist/ is empty: run `npm run build` first.");
  process.exit(1);
}

const errors = [];
const warnings = [];
const fail = (page, msg) => errors.push(`${page}: ${msg}`);
const warn = (page, msg) => warnings.push(`${page}: ${msg}`);

const sitemap = read("sitemap.xml");
const llms = read("llms.txt");
const robots = read("robots.txt");
const routes = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1].replace(SITE, "") || "/");
if (routes.length < 5) fail("sitemap.xml", `lists only ${routes.length} pages`);
if (!/^Sitemap: https:\/\/kitab\.t21\.dev\/sitemap\.xml$/m.test(robots)) fail("robots.txt", "no Sitemap line");

const fileOf = (path) => (path === "/" ? "index.html" : `${path.slice(1)}.html`);
const pages = Object.fromEntries(routes.map((r) => [r, read(fileOf(r))]));
pages["/404"] = read("404.html");

/** Every id on a page. */
const idsOf = (html) => [...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);
const attr = (tag, name) => tag.match(new RegExp(`\\s${name}="([^"]*)"`))?.[1];
const meta = (html, key) => {
  for (const m of html.matchAll(/<meta\s[^>]*>/g)) {
    const t = m[0];
    if (attr(t, "name") === key || attr(t, "property") === key) return attr(t, "content");
  }
  return undefined;
};
/** The HTML with every mock app window (<div class="kbox" …>…</div>) cut out. */
function withoutMockWindows(html) {
  let out = "";
  let from = 0;
  for (;;) {
    const start = html.indexOf('<div class="kbox"', from);
    if (start < 0) return out + html.slice(from);
    out += html.slice(from, start);
    // Walk to the matching </div>.
    const tag = /<div[\s>]|<\/div>/g;
    tag.lastIndex = start;
    let depth = 0;
    let m;
    while ((m = tag.exec(html))) {
      depth += m[0] === "</div>" ? -1 : 1;
      if (depth === 0) break;
    }
    from = m ? tag.lastIndex : html.length;
  }
}

/** The body without scripts and styles, as text. */
const textOf = (html) =>
  html
    .slice(html.indexOf("<body"))
    .replace(/<script[\s\S]*?<\/script>/g, " ")
    .replace(/<style[\s\S]*?<\/style>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ");

for (const [path, html] of Object.entries(pages)) {
  const isPage = path !== "/404";
  const title = html.match(/<title>([^<]*)<\/title>/)?.[1];
  if (!title) fail(path, "no <title>");
  else if (title.length > 70) warn(path, `title is ${title.length} characters`);
  const desc = meta(html, "description");
  if (!desc) fail(path, "no meta description");
  else if (desc.length < 50 || desc.length > 200) warn(path, `description is ${desc.length} characters`);
  if (isPage) {
    const canonical = html.match(/<link rel="canonical" href="([^"]+)"/)?.[1];
    const want = `${SITE}${path === "/" ? "/" : path}`;
    if (canonical !== want) fail(path, `canonical is ${canonical}, want ${want}`);
    for (const k of ["og:title", "og:description", "og:url", "og:image", "og:type", "twitter:card"]) if (!meta(html, k)) fail(path, `no ${k}`);
    if (meta(html, "og:url") !== want) fail(path, "og:url doesn't match the canonical");
    if (!html.includes('type="application/ld+json"')) fail(path, "no JSON-LD");
    const md = html.match(/<link rel="alternate" type="text\/markdown" href="\/([^"]+)"/)?.[1];
    if (!md || !existsSync(new URL(md, dist))) fail(path, "no Markdown mirror");
    else if (!llms.includes(`${SITE}/${md}`)) fail(path, `llms.txt doesn't list ${md}`);
  }
  if (!/<html lang="[a-z]{2}/.test(html)) fail(path, "no lang on <html>");
  if (!html.includes('<main id="main"')) fail(path, "no <main id=\"main\">");

  // Headings: one h1, and no level skipped on the way down. Kitab's window rebuilt in HTML
  // (src/mock, class "kbox") shows a document with its own headings; it is counted apart.
  const outside = withoutMockWindows(html);
  const inMock = [...html.matchAll(/<h[1-6][\s>]/g)].length - [...outside.matchAll(/<h[1-6][\s>]/g)].length;
  if (inMock) warn(path, `${inMock} headings inside the mock app window(s)`);
  const levels = [...outside.matchAll(/<h([1-6])[\s>]/g)].map((m) => Number(m[1]));
  const h1s = levels.filter((l) => l === 1).length;
  if (h1s !== 1) fail(path, `${h1s} <h1> elements`);
  levels.forEach((l, i) => i > 0 && l > levels[i - 1] + 1 && fail(path, `heading jumps from h${levels[i - 1]} to h${l}`));

  // Images: alt, width and height, and the file exists.
  for (const m of html.matchAll(/<img\s[^>]*>/g)) {
    const t = m[0];
    const src = attr(t, "src");
    if (attr(t, "alt") === undefined) fail(path, `image without alt: ${src}`);
    if (!attr(t, "width") || !attr(t, "height")) fail(path, `image without width and height: ${src}`);
    if (src?.startsWith("/") && !existsSync(new URL(src.slice(1), dist))) fail(path, `image not found: ${src}`);
  }

  // Ids are unique.
  const ids = idsOf(html);
  const seen = new Set();
  for (const id of ids) {
    if (seen.has(id)) fail(path, `duplicate id "${id}"`);
    seen.add(id);
  }

  // Internal links lead somewhere.
  for (const m of html.matchAll(/\shref="([^"]+)"/g)) {
    const href = m[1].replace(/&amp;/g, "&");
    if (/^(https?:|mailto:)/.test(href)) continue;
    const [p, hash] = href.split("#");
    const target = p === "" ? path : p.split("?")[0];
    if (p && !(target === "/" || pages[target] || existsSync(new URL(target.slice(1), dist)))) {
      fail(path, `link to a missing page: ${href}`);
      continue;
    }
    if (hash) {
      const other = pages[target];
      if (other && !idsOf(other).includes(hash)) fail(path, `link to a missing anchor: ${href}`);
    }
  }
}

// The docs page: the copy rules (no em dashes, no competitor names).
const docsText = textOf(withoutMockWindows(pages["/docs"] ?? "")) + read("docs.md");
if (!pages["/docs"]) fail("/docs", "missing");
if (docsText.includes("—")) fail("/docs", "an em dash in the copy");
for (const word of ["Sumatra", "seamless", "blazing", "unlock", "revolutionary", "Windows first"])
  if (docsText.toLowerCase().includes(word.toLowerCase())) fail("/docs", `copy says "${word}"`);

// Every public image is used somewhere, so nothing stale ships.
const docsImgs = readdirSync(new URL("docs/", dist)).filter((f) => f.endsWith(".webp"));
for (const f of docsImgs) if (!pages["/docs"]?.includes(`/docs/${f}`)) warn("/docs", `public/docs/${f} isn't used`);

for (const w of warnings) console.log(`warn  ${w}`);
for (const e of errors) console.log(`FAIL  ${e}`);
console.log(`checked ${Object.keys(pages).length} pages: ${errors.length} problems, ${warnings.length} warnings`);
process.exit(errors.length ? 1 : 0);
