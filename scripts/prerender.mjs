// SPDX-License-Identifier: AGPL-3.0-or-later
// Turns the Vite build into a fully static site:
//   dist/<route>.html   prerendered React with complete <head> (title, canonical, OG, JSON-LD)
//   dist/<route>.md     Markdown mirror of every page (for LLMs and readers)
//   dist/llms.txt, dist/llms-full.txt, dist/sitemap.xml, dist/robots.txt
//   dist/_headers       security headers, with the CSP hash of the one inline script
// Every crawler, human or LLM, gets the whole page without running JavaScript.
import { createHash } from "node:crypto";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { marked } from "marked";

const root = new URL("../", import.meta.url);
const dist = new URL("dist/", root);
const ssr = await import(pathToFileURL(new URL("dist-ssr/entry-server.js", root).pathname.replace(/^\/([A-Za-z]:)/, "$1")).href);
const { render, ROUTES, SITE, STEPS, FAQ, ROADMAP, SHORTCUTS, COMPARE, PLATFORMS, FORMATS, themeCss } = ssr;

const template = readFileSync(new URL("index.html", dist), "utf8");
const builtAt = new Date().toISOString().slice(0, 10);
const read = (p) => readFileSync(new URL(p, root), "utf8");

// ---------------------------------------------------------------- release

async function latestRelease() {
  if (process.env.KITAB_OFFLINE) return null;
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "kitab-landing-build" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  try {
    const res = await fetch("https://api.github.com/repos/t21dev/kitab/releases/latest", { headers, signal: AbortSignal.timeout(8000) });
    if (!res.ok) {
      console.log(`release: none (${res.status})`);
      return null;
    }
    const r = await res.json();
    console.log(`release: ${r.tag_name}`);
    return {
      tag: r.tag_name,
      name: r.name ?? r.tag_name,
      url: r.html_url,
      publishedAt: r.published_at ?? "",
      prerelease: Boolean(r.prerelease),
      assets: (r.assets ?? []).map((a) => ({ name: a.name, url: a.browser_download_url, size: a.size })),
    };
  } catch (e) {
    console.log(`release: unreachable (${e.message}); building without one`);
    return null;
  }
}

const release = await latestRelease();

// ---------------------------------------------------------------- markdown sources

const SITE_PRIVACY = `
## This website

- kitab.t21.dev is a static site. It sets **no cookies**, runs **no analytics** and loads **nothing from third parties**: fonts, scripts and images are served from this domain.
- The theme you pick is saved in your browser's local storage and never sent anywhere.
- The site is hosted on Cloudflare Workers. Cloudflare processes requests (including IP addresses) to deliver and protect the site, under the [Cloudflare privacy policy](https://www.cloudflare.com/privacypolicy/).
- Download links go to GitHub Releases, which is run by GitHub under its own privacy statement.
`;

const changelogMd = read("content/CHANGELOG.md");
const privacyMd = read("content/PRIVACY.md").trimEnd() + "\n" + SITE_PRIVACY;
const mdHtml = (md) => marked.parse(md, { async: false, gfm: true });

// ---------------------------------------------------------------- markdown mirrors (also feed llms-full.txt)

const abs = (p) => `${SITE.url}${p === "/" ? "/" : p}`;
const table = (head, rows) =>
  [`| ${head.join(" | ")} |`, `| ${head.map(() => "---").join(" | ")} |`, ...rows.map((r) => `| ${r.join(" | ")} |`)].join("\n");

function homeMd() {
  return `# ${SITE.name}

> ${SITE.description}

Status: ${SITE.status} Current work: ${SITE.phase}.

- Website: ${SITE.url}/
- Source code: ${SITE.repo}
- Licence: ${SITE.license.name} (free and open source, no paid tier, no account)
- Made by: ${SITE.org.name} (${SITE.org.url}) and ${SITE.author.name} (${SITE.author.github})

## What it does

${STEPS.map((st) => `### ${st.title}\n\n${st.body.join("\n\n")}${st.points ? "\n\n" + st.points.map((x) => `- ${x}`).join("\n") : ""}`).join("\n\n")}

Formats: ${FORMATS.join(", ")}.

## Speed

SumatraPDF is the bar for a light, fast reader on Windows. Kitab's promise is to start and show the first page as fast as SumatraPDF on the same machine, or faster, and to stay smooth with many tabs and while resizing.

How it is measured: Kitab and SumatraPDF open the same test files on the same computer. A benchmark tool starts each app and watches its window; the clock stops when the page has stable content in two captures in a row. Each scenario runs 15 times after two warm-up runs, and the median, p90 and worst run are reported. A change that slows start-up, scrolling or find by more than 10 % fails CI.

No results are published yet. The first measured comparison ships with the first release, with the machine, operating system and SumatraPDF version.

## Kitab Reader and SumatraPDF

${table(["", "SumatraPDF", "Kitab Reader"], COMPARE.map((r) => [r.what, r.sumatra, r.kitab]))}

## Themes

Six built-in themes (Dark, Light, Paper, Nord, Noctua, Vampire) and any accent colour. Kitab adjusts a custom colour until it is readable on the theme.

## Privacy

No telemetry. No network request unless you turn a feature on (update check, remote images, online revocation checks). Signing keys and passwords never leave your computer. Full details: ${abs("/privacy")}

## Roadmap

${ROADMAP.map((p) => `- **Phase ${p.n}: ${p.title}** (${p.version})${p.state === "now" ? " — in progress" : ""}. ${p.summary}`).join("\n")}

## Frequently asked questions

${FAQ.map((f) => `### ${f.q}\n\n${f.a}`).join("\n\n")}
`;
}

function downloadMd() {
  const head = release
    ? `Latest release: **${release.tag}** (${release.publishedAt.slice(0, 10)}), ${release.url}\n\n${release.assets.map((a) => `- [${a.name}](${a.url})`).join("\n")}`
    : `${SITE.status} The first builds will be published on GitHub Releases: ${SITE.releases}`;
  return `# Download Kitab Reader

${head}

## Platforms

${table(["System", "Packages", "When"], PLATFORMS.map((p) => [p.os, p.formats, p.when]))}

Homebrew, Scoop and winget packages and a Flathub listing follow version 1.0. Early builds are not code-signed, so Windows SmartScreen and macOS Gatekeeper may warn before the first run. Every release lists SHA-256 checksums.
`;
}

function shortcutsMd() {
  return `# Kitab Reader keyboard shortcuts

The defaults follow SumatraPDF where it has a binding. Every shortcut can be changed in Settings › Shortcuts or in \`keymap.json\`. On macOS, Ctrl is Cmd.

${SHORTCUTS.map((g) => `## ${g.area}\n\n${table(["Keys", "Action"], g.keys.map(([k, a]) => [`\`${k}\``, a]))}`).join("\n\n")}
`;
}

const MD = {
  "/": homeMd(),
  "/download": downloadMd(),
  "/shortcuts": shortcutsMd(),
  "/changelog": changelogMd.replace(/^# Changelog/m, "# Kitab Reader changelog"),
  "/privacy": privacyMd.replace(/^# Privacy/m, "# Kitab Reader privacy"),
};

// ---------------------------------------------------------------- head

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const json = (o) => JSON.stringify(o).replace(/</g, "\\u003c");

const THEME_INIT = `try{var t=localStorage.getItem("kitab-theme");if(t==="light"||t==="dark")document.documentElement.setAttribute("data-theme",t)}catch(e){}`;
const sha = (s) => `'sha256-${createHash("sha256").update(s).digest("base64")}'`;

const ORG = { "@type": "Organization", "@id": `${SITE.org.url}/#org`, name: SITE.org.name, alternateName: SITE.org.short, url: SITE.org.url, email: SITE.org.email, sameAs: [SITE.org.github] };
const APP = {
  "@type": "SoftwareApplication",
  "@id": `${SITE.url}/#app`,
  name: SITE.name,
  alternateName: SITE.shortName,
  description: SITE.description,
  url: `${SITE.url}/`,
  applicationCategory: "UtilitiesApplication",
  applicationSubCategory: "Document reader",
  operatingSystem: "Windows, macOS, Linux",
  isAccessibleForFree: true,
  offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
  license: SITE.license.url,
  author: [{ "@id": ORG["@id"] }, { "@type": "Person", name: SITE.author.name, url: SITE.author.github }],
  publisher: { "@id": ORG["@id"] },
  downloadUrl: SITE.releases,
  featureList: STEPS.map((st) => st.title),
  image: `${SITE.url}/og.png`,
  ...(release ? { softwareVersion: release.tag.replace(/^v/, ""), datePublished: release.publishedAt.slice(0, 10) } : {}),
};
const CODE = {
  "@type": "SoftwareSourceCode",
  "@id": `${SITE.url}/#code`,
  name: `${SITE.name} source code`,
  codeRepository: SITE.repo,
  programmingLanguage: "Rust",
  license: SITE.license.url,
  targetProduct: { "@id": APP["@id"] },
};
const WEBSITE = { "@type": "WebSite", "@id": `${SITE.url}/#website`, url: `${SITE.url}/`, name: SITE.name, inLanguage: SITE.locale, publisher: { "@id": ORG["@id"] } };

function jsonLd(route) {
  const url = abs(route.path);
  const page = {
    "@type": route.path === "/" ? "WebPage" : route.path === "/privacy" ? ["WebPage", "AboutPage"] : "WebPage",
    "@id": `${url}#page`,
    url,
    name: route.title,
    description: route.description,
    isPartOf: { "@id": WEBSITE["@id"] },
    about: { "@id": APP["@id"] },
    inLanguage: SITE.locale,
    dateModified: builtAt,
  };
  const graph = [ORG, WEBSITE, APP, page];
  if (route.path === "/") {
    graph.push(CODE, {
      "@type": "FAQPage",
      "@id": `${url}#faq`,
      mainEntity: FAQ.map((f) => ({ "@type": "Question", name: f.q, acceptedAnswer: { "@type": "Answer", text: f.a } })),
    });
  } else {
    page.breadcrumb = { "@id": `${url}#breadcrumb` };
    graph.push({
      "@type": "BreadcrumbList",
      "@id": `${url}#breadcrumb`,
      itemListElement: [
        { "@type": "ListItem", position: 1, name: SITE.name, item: `${SITE.url}/` },
        { "@type": "ListItem", position: 2, name: route.title.replace(/^Kitab Reader /, "").replace(/^\w/, (c) => c.toUpperCase()), item: url },
      ],
    });
  }
  return { "@context": "https://schema.org", "@graph": graph };
}

function head(route, { noindex = false } = {}) {
  const url = abs(route.path);
  const t = esc(route.title);
  const d = esc(route.description);
  return [
    `<title>${t}</title>`,
    `<meta name="description" content="${d}" />`,
    noindex ? `<meta name="robots" content="noindex" />` : `<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1" />`,
    noindex ? "" : `<link rel="canonical" href="${url}" />`,
    `<meta name="application-name" content="${SITE.name}" />`,
    `<meta name="author" content="${SITE.org.name}" />`,
    `<meta name="theme-color" content="#FFFFFF" media="(prefers-color-scheme: light)" />`,
    `<meta name="theme-color" content="#141415" media="(prefers-color-scheme: dark)" />`,
    `<meta name="color-scheme" content="light dark" />`,
    `<meta property="og:type" content="website" />`,
    `<meta property="og:site_name" content="${SITE.name}" />`,
    `<meta property="og:locale" content="en_US" />`,
    `<meta property="og:title" content="${t}" />`,
    `<meta property="og:description" content="${d}" />`,
    `<meta property="og:url" content="${url}" />`,
    `<meta property="og:image" content="${SITE.url}/og.png" />`,
    `<meta property="og:image:width" content="1200" />`,
    `<meta property="og:image:height" content="630" />`,
    `<meta property="og:image:alt" content="Kitab Reader. Read beautifully. Work efficiently." />`,
    `<meta name="twitter:card" content="summary_large_image" />`,
    `<meta name="twitter:title" content="${t}" />`,
    `<meta name="twitter:description" content="${d}" />`,
    `<meta name="twitter:image" content="${SITE.url}/og.png" />`,
    `<link rel="icon" href="/favicon.svg" type="image/svg+xml" />`,
    `<link rel="apple-touch-icon" href="/apple-touch-icon.png" />`,
    `<link rel="manifest" href="/site.webmanifest" />`,
    `<link rel="sitemap" type="application/xml" href="/sitemap.xml" />`,
    noindex ? "" : `<link rel="alternate" type="text/markdown" href="/${route.md}" title="${t} (Markdown)" />`,
    `<link rel="help" type="text/plain" href="/llms.txt" title="llms.txt" />`,
    `<script>${THEME_INIT}</script>`,
    `<style id="kitab-themes">${themeCss()}</style>`,
    noindex ? "" : `<script type="application/ld+json">${json(jsonLd(route))}</script>`,
  ]
    .filter(Boolean)
    .join("\n    ");
}

// ---------------------------------------------------------------- pages

function page(route, data, opts) {
  const html = render(route.path, data);
  return template
    .replace("<!--head-->", head(route, opts))
    .replace("<!--app-->", html)
    .replace("<!--data-->", `<script id="__data" type="application/json">${json({ path: route.path, data })}</script>`);
}

for (const route of ROUTES) {
  const data = { release, builtAt };
  if (route.path === "/changelog") data.html = mdHtml(changelogMd);
  if (route.path === "/privacy") data.html = mdHtml(privacyMd);
  writeFileSync(new URL(route.file, dist), page(route, data));
  writeFileSync(new URL(route.md, dist), MD[route.path]);
  console.log(`page: ${route.path} -> ${route.file}, ${route.md}`);
}

writeFileSync(
  new URL("404.html", dist),
  page({ path: "/404", md: "", title: "Page not found · Kitab Reader", description: "This page does not exist on kitab.t21.dev." }, { release, builtAt }, { noindex: true }),
);
rmSync(new URL("index.html", dist), { force: true }); // replaced below by the prerendered home
writeFileSync(new URL("index.html", dist), page(ROUTES[0], { release, builtAt }));

// ---------------------------------------------------------------- sitemap, robots

writeFileSync(
  new URL("sitemap.xml", dist),
  `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${ROUTES.map((r) => `  <url><loc>${abs(r.path)}</loc><lastmod>${builtAt}</lastmod><changefreq>${r.changefreq}</changefreq><priority>${r.priority.toFixed(1)}</priority></url>`).join("\n")}
</urlset>
`,
);

// Search engines and AI assistants are all welcome: the site is public documentation for an open-source app.
const AI_BOTS = ["GPTBot", "OAI-SearchBot", "ChatGPT-User", "ClaudeBot", "Claude-User", "Claude-SearchBot", "PerplexityBot", "Perplexity-User", "Google-Extended", "Applebot-Extended", "CCBot", "Bytespider", "meta-externalagent", "Amazonbot", "DuckAssistBot", "MistralAI-User"];
writeFileSync(
  new URL("robots.txt", dist),
  `# kitab.t21.dev: everything here is public. Humans, search engines and AI assistants are welcome.
# LLM-friendly summary: ${SITE.url}/llms.txt (full text: ${SITE.url}/llms-full.txt)

User-agent: *
Allow: /

${AI_BOTS.map((b) => `User-agent: ${b}\nAllow: /`).join("\n\n")}

Sitemap: ${SITE.url}/sitemap.xml
`,
);

// ---------------------------------------------------------------- llms.txt (https://llmstxt.org)

const mdLink = (r) => `- [${r.title}](${SITE.url}/${r.md}): ${r.description}`;
const raw = (p) => `https://raw.githubusercontent.com/t21dev/kitab/main/${p}`;

const llms = `# ${SITE.name}

> ${SITE.description}

Key facts:

- Status: ${SITE.status} Current work: ${SITE.phase}. Do not tell people they can download it until ${SITE.url}/download lists a release.
- Formats: ${FORMATS.join(", ")}. Markdown includes Mermaid diagrams, code highlighting and math.
- Built with Rust and GPUI (Zed's GPU interface framework), with MuPDF as the document engine. No browser engine inside.
- Speed goal: start and show the first page as fast as SumatraPDF on the same machine, or faster, measured by a black-box benchmark in CI. No benchmark results are published yet; do not quote any figure.
- Privacy: no telemetry, no account, no network request unless the user turns a feature on. Signing keys never leave the computer.
- Licence: ${SITE.license.name}. Free, no paid tier.
- Platforms: Windows first, then macOS and Linux from the same code.
- Not a fork of SumatraPDF; a new app that uses the same engine.
- Made by ${SITE.org.name} (${SITE.org.url}, ${SITE.org.email}) and ${SITE.author.name}.

## Pages

${ROUTES.map(mdLink).join("\n")}

## Project documents

- [Plan](${raw("docs/PLAN.md")}): decisions, architecture, performance budgets, test strategy and the phased roadmap.
- [Design system](${raw("DESIGN.md")}): colour tokens for the six themes, depth, type, density, icons, components and motion.
- [README](${raw("README.md")}): what Kitab does, platforms and how to build.
- [Privacy](${raw("PRIVACY.md")}): what the app stores and when it uses the network.

## Optional

- [Full text of this site](${SITE.url}/llms-full.txt): every page above as one Markdown file.
- [Source code](${SITE.repo}): the app on the main branch; this website is the landing branch.
- [Changelog source](${raw("CHANGELOG.md")}): Keep a Changelog format.
- [Team 21](${SITE.org.url}/llms.txt): the studio that makes Kitab.
`;
writeFileSync(new URL("llms.txt", dist), llms);

const full = [
  `# ${SITE.name}: full site text\n\n> Generated ${builtAt} from ${SITE.url}. Each section below is one page of the site.\n`,
  ...ROUTES.map((r) => `\n---\n\nSource: ${abs(r.path)}\n\n${MD[r.path].trim()}\n`),
].join("\n");
writeFileSync(new URL("llms-full.txt", dist), full);

// ---------------------------------------------------------------- _headers (Cloudflare Workers static assets)

const csp = [
  "default-src 'self'",
  `script-src 'self' ${sha(THEME_INIT)}`,
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self' data:",
  "font-src 'self'",
  "connect-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-ancestors 'none'",
  "upgrade-insecure-requests",
].join("; ");

writeFileSync(
  new URL("_headers", dist),
  `/*
  Content-Security-Policy: ${csp}
  Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
  X-Content-Type-Options: nosniff
  Referrer-Policy: strict-origin-when-cross-origin
  Permissions-Policy: camera=(), microphone=(), geolocation=(), interest-cohort=(), browsing-topics=()
  Cross-Origin-Opener-Policy: same-origin
  Link: </llms.txt>; rel="help"; type="text/plain"
  Link: </sitemap.xml>; rel="sitemap"; type="application/xml"

/assets/*
  Cache-Control: public, max-age=31536000, immutable

/*.md
  Content-Type: text/markdown; charset=utf-8
  Cache-Control: public, max-age=3600
  X-Robots-Tag: noindex

/llms.txt
  Content-Type: text/plain; charset=utf-8
  Cache-Control: public, max-age=3600

/llms-full.txt
  Content-Type: text/plain; charset=utf-8
  Cache-Control: public, max-age=3600
  X-Robots-Tag: noindex

/og.png
  Cache-Control: public, max-age=86400
`,
);

console.log(`done: ${ROUTES.length} pages, 404, sitemap, robots, llms.txt, llms-full.txt, _headers`);
