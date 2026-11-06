<div align="center">

# Kitab Reader

**Read beautifully. Work efficiently.**

A fast, native reader for PDF, EPUB, Markdown and comics, with tabs, a library that remembers where you were, find-in-document, highlights, digital signatures and local PDF tools.
Rust and GPUI on the GPU, with MuPDF underneath. No browser engine inside.

By **[t21 dev](https://github.com/t21dev)** and **[TriptoAfsin](https://github.com/TriptoAfsin)**

[![Licence: AGPL-3.0](https://img.shields.io/badge/licence-AGPL--3.0-blue?style=flat-square)](LICENSE)
![Status: Phase 0](https://img.shields.io/badge/status-phase%200-lightgrey?style=flat-square)

</div>

---

> **Status: Phase 0 (foundations).** There is no app to download yet. This repository holds the plan, the design system, the test corpus and the first workspace crates. Work lands phase by phase. See the [roadmap](docs/PLAN.md#12-roadmap).

## What it will do

- **Open fast.** The goal is to start and show the first page as fast as SumatraPDF on the same machine, or faster. It is measured in CI, not claimed ([how](docs/PLAN.md#6-performance)).
- **Read anything:** PDF, EPUB, MOBI/AZW3 (without DRM), FB2, XPS, CBZ, images, TXT, and **Markdown with Mermaid diagrams, code highlighting and math**.
- **Tabs and memory:** many documents in one window, restored after restart with the same page, zoom and layout. Choose what happens on start-up: restore the session, open Home, open the last file, or an empty window.
- **Home and library:** continue reading, recent files as cards or a list, collections, tags, and full-text search across your books.
- **Find (Ctrl+F)** with match case, whole word and accent-insensitive matching, plus a results list.
- **True page sizes:** mixed portrait, landscape and odd-sized PDF pages each keep their own shape.
- **Highlights and notes**, exported to Markdown, CSV or text.
- **Digital signatures:** see whether a PDF's signatures are valid and whether it changed after signing; sign with a certificate file or your system's certificate store. Keys never leave your computer.
- **PDF tools, locally:** merge, split, extract, rotate, compress, extract text and images, watermark, page numbers, passwords.
- **Native printing** on Windows, macOS and Linux, with per-page orientation.
- **Keyboard first:** Sumatra-compatible shortcuts, a command palette (Ctrl+K), and a keymap you can rebind.
- **Themes:** Dark, Light, Paper, Nord, Noctua and Vampire, with any accent colour you like.
- **Ultra performance mode** for old machines and huge files, and a hardware acceleration switch.
- **Private:** no telemetry, and no network unless you turn something on. See [PRIVACY.md](PRIVACY.md).

## Platforms

Windows first, then macOS and Linux from the same code. Releases will be on [GitHub Releases](https://github.com/t21dev/kitab/releases): MSI, installer and portable zip for Windows; DMG for macOS; AppImage, deb, rpm and Flatpak for Linux. Homebrew, Scoop and winget will follow 1.0.

## Repository

| Path | What |
|---|---|
| [`docs/PLAN.md`](docs/PLAN.md) | Decisions, architecture, performance budgets, tests, roadmap |
| [`DESIGN.md`](DESIGN.md) | Colour tokens for the six themes, depth, type, density, icons, components, motion |
| [`AGENTS.md`](AGENTS.md) | Rules for working on the code (guardrails, commits, versions, branches) |
| [`tests/fixtures/`](tests/fixtures/MANIFEST.md) | The test corpus: PDF, EPUB and Markdown files and their expected results |
| [`CHANGELOG.md`](CHANGELOG.md) | Changes, [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) + [SemVer](https://semver.org/) |
| branch [`landing`](https://github.com/t21dev/kitab/tree/landing) | The website (React + Tailwind on Cloudflare Workers) |

## Building

Phase 0: the Rust workspace has the theme tokens and a stub binary. You need Rust stable (see `rust-toolchain.toml`).

```powershell
cargo run -p kitab
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python tools/check_source.py     # SPDX headers, no colour literals outside kitab-theme
cargo deny check                 # cargo install cargo-deny
```

The GPUI + MuPDF spike lives in `spikes/spike-a` and needs libclang; see [ADR 0001](docs/adr/0001-spike-a-gpui-mupdf.md).

The test corpus:

```powershell
python -m venv tests\fixtures\tools\.venv
tests\fixtures\tools\.venv\Scripts\pip install -r tests\fixtures\tools\requirements.txt
tests\fixtures\tools\.venv\Scripts\python tests\fixtures\tools\generate_fixtures.py --large
tests\fixtures\tools\.venv\Scripts\python tests\fixtures\tools\verify_fixtures.py
```

## Contributing

Read [AGENTS.md](AGENTS.md) first. Commits follow [Conventional Commits](https://www.conventionalcommits.org/), and every user-visible change gets a CHANGELOG line.

## Licence

[AGPL-3.0-or-later](LICENSE). Kitab uses [MuPDF](https://mupdf.com/), which is AGPL-licensed by Artifex. Nord theme colours are from [Nord](https://www.nordtheme.com/) (MIT).
