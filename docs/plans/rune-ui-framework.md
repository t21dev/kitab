# Rune as Kitab's UI framework: future plan

Status: **not adopted. Watching.** Written 9 November 2026. Next review: **February 2027**.

[Rune](https://github.com/anomalyco/rune) (anomalyco/rune, MIT) is a GPU-rendered Rust UI
framework from Anomaly, the OpenCode team. This page records why Kitab stays on GPUI for now,
what we borrow from Rune today, and the exact conditions and measurements that would make us
switch.

## 1. Where things stand (9 November 2026)

| | GPUI 0.2.2 (today, patched in `vendor/`) | Rune (`dev`) |
|---|---|---|
| Age, releases | Ships Zed; versioned on crates.io | Repo created 22 Oct 2026; no releases, pin a `dev` rev |
| Contributions | We patch locally and can upstream | No pull requests accepted (issues only) |
| Windows renderer | Direct3D 11 + DirectWrite | wgpu (DX12 / Vulkan), DirectComposition |
| Text | DirectWrite | Parley 0.12 + swash, own atlas, `FontSmoothing::DirectWrite` |
| Layout | Taffy | Taffy 0.15 |
| Windows IME | Proven in Zed | winit preedit; TSF planned, called the "top risk" in its PLAN.md |
| Native menus on Windows | Ours | Report `Unsupported` |
| Accessibility | None | None yet (AccessKit planned) |
| Headless render / scripted input | No | Yes: PNG, JSON element tree, scripted input on a virtual clock |
| Components | Ours (`kitab-ui`, ~32k lines, 49 files use `gpui::`) | `rune-kit`: virtual Table/Tree, Menu, Combobox, Dialog, Toast, TextInput/TextArea, Markdown, DiffView |
| Motion | Basic easing | Springs, shared-element flights, Reduce Motion, layer caching |
| Long lists | Ours | `.virtualize()`, but view construction still visits every row |

Open Rune issues on Windows at the time of writing: resize panic (#306), other windows
turning white (#317), overlays never taking the keyboard (#325), a ~32 MB DX12 descriptor heap
(#308).

## 2. Decision

Stay on GPUI. Kitab is Windows-first, its goal is to start and run lighter than SumatraPDF
(`docs/perf/baseline.md`), and forms with typed text and passwords are in scope. Today Rune is
weakest exactly there: Windows maturity, IME, and unmeasured start-up and memory cost. Moving
would be a rewrite of the UI layer, not a port, onto an API with no stability promise.

## 3. Borrow now (no framework change)

These are the parts of Rune worth having in Kitab regardless of the decision.

1. **Headless harness.** A `kitab --headless` (or `kitab-bench` sub-command) mode that renders
   a window to PNG, dumps the element tree, and replays a small input script (click, type, key,
   scroll). Use it for UI reviews and screenshots instead of capturing the live window.
2. **Spring motion with Reduce Motion.** Replace ad-hoc easing in the dock, panels and page
   transitions with a small spring helper. When Windows "Show animations" is off, spatial
   movement snaps and only opacity fades (~150 ms), as Rune does.
3. **Image mip chain for thumbnails.** Downscale page bitmaps by halves once and pick the
   nearest size, instead of scaling the full page each time. Check against the
   "perf never costs UX" rule: no flicker, no blurry pages at rest.
4. **Element tree as the future accessibility tree.** Keep roles and labels on interactive
   elements now (buttons, switches, form fields), so an AccessKit bridge later is wiring, not
   a rewrite.

## 4. Conditions to look again

Re-run the evaluation when **all** of these are true in Rune:

- [ ] A tagged release (or a written API stability policy).
- [ ] Windows IME through TSF, tested with at least one CJK input method and dead keys.
- [ ] Native menus or a supported equivalent on Windows.
- [ ] No open Windows crash or blank-window issues like #306 and #317.
- [ ] AccessKit landed on Windows (not required to switch, but required to call it ahead of GPUI).

If they are not met by the review date, push the date back three months and update §1.

## 5. The spike (when §4 is met, or earlier if we want numbers)

One to two days, in `spikes/spike-rune/`, never in the main crates.

1. A Rune window that opens a PDF with `kitab-mupdf` and shows page 1 as an RGBA image,
   scrolls a 40-page document, and zooms.
2. A text field over the page (the form-filling case) typed into with the Windows Japanese IME.
3. Measure with `kitab-bench s2 --runs 15 --warmup 2` on `tests/fixtures/pdf/01-text-outline.pdf`,
   same machine, release build, static C runtime, against current Kitab and SumatraPDF.

### Gates

The spike passes only if Rune is **no worse than current Kitab** on every line:

| Measure | Kitab today (baseline.md) | Rune must be |
|---|---|---|
| Window appears (no splash) | ~201 ms | ≤ 201 ms |
| First page painted | 326–385 ms bracket | ≤ Kitab |
| Working set, 40 pages | ~107 MB | ≤ 107 MB |
| Idle CPU after 5 s | ~0 % | ~0 % |
| IME composition in a form field | works | works, caret placed correctly |
| Text sharpness vs DirectWrite | reference | no visible difference at 100 % and 150 % scale |

The GDI splash (page 1 at ~21 ms) does not depend on the framework and is kept either way.

## 6. If the spike passes: migration path

Do not big-bang it. Order by risk, smallest first, each step shippable:

1. **Settings and About windows** in Rune, running alongside GPUI windows, to prove two
   frameworks can share one process (event loop ownership is the first question to answer).
   If they cannot, stop here and plan a single cut-over branch instead.
2. **Library view** (Table/Tree from `rune-kit`, virtualised).
3. **Markdown reader** (compare `rune-kit` Markdown against `kitab-md` for selection,
   typography panel and bookmarks; keep whichever loses no features).
4. **PDF reader and forms** last, since they carry the start-up and IME risk.
5. Remove GPUI and `vendor/gpui-0.2.2`; drop `tools/patch_gpui.py`.

Each step must keep the Sumatra benchmark table from getting worse. Pin one Rune `dev` rev per
release and record it in the changelog.

## 7. Sources

- README: https://github.com/anomalyco/rune
- Core docs: https://github.com/anomalyco/rune/blob/dev/crates/rune/README.md
- Components: https://github.com/anomalyco/rune/blob/dev/crates/rune-kit/README.md
- Plan: https://github.com/anomalyco/rune/blob/dev/PLAN.md
- Changelog: https://github.com/anomalyco/rune/blob/dev/CHANGELOG.md
- Kitab baselines: `docs/perf/baseline.md`, `docs/adr/0001-spike-a-gpui-mupdf.md`,
  `docs/adr/0002-keep-gpui-fix-startup.md`
