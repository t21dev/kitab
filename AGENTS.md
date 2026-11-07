# Working on Kitab

Rules for anyone changing this repository, human or agent. Read [docs/PLAN.md](docs/PLAN.md) for the why and [DESIGN.md](DESIGN.md) for the look. If a change needs something these files do not cover, update the file in the same change, so docs and code never disagree.

## Before you start

- Find the phase in PLAN.md §12 the work belongs to. Don't build ahead of the current phase without saying so in the PR.
- One change per PR: a feature, a fix, or a refactor. Not a mix.
- If the change touches the start-up path, rendering, scrolling, tabs or memory, run the perf scenarios it affects (PLAN.md §6) before and after, and put both numbers in the PR.

## Guardrails

These are hard rules. CI enforces the ones marked **(CI)**. Reviewers enforce the rest.

### Performance

1. **The start-up path is sacred.** Nothing new runs between process start and the first sharp page unless it is required to draw that page. That includes network, indexing, update checks, thumbnail generation, opening non-active tabs, loading reading fonts, and scanning watched folders. Defer with the idle scheduler, and justify in the PR anything that can't be deferred.
2. **Never block the main thread.** No file I/O, no SQLite, no MuPDF, no hashing, no image decode, no `sleep`, no `block_on` on the GPUI thread. Use the render pool, the io thread or `cx.background_spawn`.
3. **Every background job is cancellable** and carries a generation. Results for a stale generation are dropped, not applied.
4. **Nothing proportional to library size or tab count at start-up or idle.** Inactive tabs are metadata until focused.
5. **Caches are bounded** (bytes, not items) and evict under memory pressure. No unbounded `Vec`/`HashMap` growth keyed by user content.
6. **Perf regressions fail CI** (CI): > 10 % on the median of S1, S2, S6(50), S9 or S11 against `main`. Fix it or explain in the PR, and get an explicit approval.
7. **Installer ≤ 15 MB** (CI) for Windows x64. A dependency, font or asset that grows it needs a line in the PR with the before/after size.

### Correctness and safety

8. **Every document is hostile.** Parsing and rendering happen in the pool with limits (page count in flight, image dimensions, decompression ratio). A failure is an error state, never a panic or a hang on the UI thread.
9. **No `unwrap`/`expect`** in code that handles file data, user input or FFI results. They're allowed in tests and for programmer invariants with a message saying why they hold.
10. **`unsafe` only in `kitab-mupdf`, `kitab-os`, `kitab-shell`, `kitab-print` and `kitab-sign`**, each block with a `// SAFETY:` comment. No panics cross FFI boundaries (`catch_unwind` at every callback).
11. **No code runs from documents.** No PDF JavaScript, no EPUB scripts, Markdown HTML through the allow-list only (PLAN.md §3.4). `tests/fixtures/md/09-hostile.md` must stay inert. **(CI)**
12. **No network without a user action or an opt-in setting.** Remote images, revocation checks and sync are off by default. The one exception, the owner's call: the update check (one request to GitHub for the latest release number, 10 s after start-up and every six hours, never on the start-up path) is on by default and turned off in Settings › About. A new network call needs a setting, a line in PRIVACY.md and a mention in the PR. No telemetry, ever.
13. **Keys stay local.** Private keys, certificate passwords and documents are never sent anywhere and never written to logs. Logs never contain document text or file paths at info level or above.
14. **The automation channel exists only in `--features e2e` builds.** **(CI)** checks that the release binary has no automation symbols and rejects the flag.
15. **Data migrations are forward-only and automatic**, with a test that migrates a fixture database from every released schema version.

### Design

16. UI follows DESIGN.md. **Colours, shadows, radii and sizes come only from `kitab-theme` tokens**, never literals in views. **(CI: a lint rejects colour literals outside `kitab-theme`.)**
17. Check new UI in all six themes, a custom accent, the three densities, Ultra mode and animations off. Update the golden screenshots in the same commit. **(CI: goldens)**
18. Every interactive element: all states, a keyboard path, a tooltip with its shortcut, an AccessKit role and name.
19. Every new action is registered as a GPUI action, bound in the default keymap if it deserves a key, shown in the command palette, and listed in PLAN.md §3.10 and the cheat sheet.

### Testing

20. **A bug fix comes with a test that fails without it.**
21. Behaviour that depends on a document uses a fixture from `tests/fixtures/`, and asserts against `expected.json`, not numbers retyped in the test. A new fixture goes through `tools/generate_fixtures.py` (or `fetch_real_world.py` with a pinned hash and a licence line), plus a MANIFEST.md row.
22. A new user-facing flow gets an e2e flow ID in PLAN.md §7.3 and a test.
23. Platform code (`cfg(target_os)`) compiles and is linted on all three OSes **(CI)**.

### Dependencies and licence

24. Kitab is **AGPL-3.0-or-later**. Every source file starts with `// SPDX-License-Identifier: AGPL-3.0-or-later` **(CI)**.
25. New crates need a reason in the PR (what, why not std or an existing crate, size cost). `cargo deny` checks licences (AGPL-compatible only), advisories, duplicates and sources **(CI)**. Fonts must be OFL or similar. Assets must have a licence line in NOTICE.
26. GPUI is pinned to a git revision. Upgrading it is its own PR, with the full e2e, goldens and perf run.

## Commands

```powershell
cargo run -p kitab -- tests\fixtures\pdf\01-text-outline.pdf
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo deny check
cargo run -p kitab-bench -- --scenario S2 --compare sumatra
cargo test -p kitab-e2e --features e2e

# fixtures
python -m venv tests\fixtures\tools\.venv
tests\fixtures\tools\.venv\Scripts\pip install -r tests\fixtures\tools\requirements.txt
tests\fixtures\tools\.venv\Scripts\python tests\fixtures\tools\generate_fixtures.py --large
tests\fixtures\tools\.venv\Scripts\python tests\fixtures\tools\verify_fixtures.py
tests\fixtures\tools\.venv\Scripts\python tests\fixtures\tools\fetch_real_world.py
```

(Commands for crates that don't exist yet start working as their phase lands.)

## Commits, versions and branches

- **Conventional Commits**, always: `feat:`, `fix:`, `perf:`, `refactor:`, `test:`, `docs:`, `build:`, `ci:`, `chore:`, with an optional scope (`feat(find): …`) and `!` for breaking changes. The subject is imperative, lower case and under 72 characters.
- **Semantic Versioning 2.0.0**, one version in the workspace `Cargo.toml`. Before 1.0 each phase is a minor release. See PLAN.md §11.
- Every user-visible change adds a line under `## [Unreleased]` in CHANGELOG.md in the same commit. Releases are `chore(release): X.Y.Z` plus tag `vX.Y.Z`. Tags trigger `release.yml`.
- `main` is the app and is always releasable. Feature branches are `feat/…`, `fix/…` and so on, merged by PR.
- `landing` is an **orphan branch** holding only the website (React + Tailwind on Cloudflare Workers). Never merge `landing` into `main` or the other way round. Content from `main` reaches it only through the sync workflow.
- Never commit secrets, signing keys or real certificates. The `.p12` files under `tests/fixtures/pdf/certs/` are throwaway test keys and say so.
