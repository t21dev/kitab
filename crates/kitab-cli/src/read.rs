// SPDX-License-Identifier: AGPL-3.0-or-later

//! The commands that read a document: info, text, markdown, search, render,
//! annotations list and verify. None of them changes the input.

use crate::cx::{self, Cx, shown};
use crate::out::{CliError, Result, count};
use kitab_core::{FindOptions, Format, RenderRequest, SigCoverage, SigModification, SigStatus};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Form fields are counted page by page; past this many pages it isn't
/// cheap enough for `info`, and the count is left out.
const FORM_COUNT_PAGES: u32 = 300;

pub fn info(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let path = cx.input_path(&file)?;
    let (id, info) = match cx.open(&path) {
        Ok(opened) => opened,
        Err(e) if e.code == "needs_password" => {
            // What can be said without the password, and how to give it.
            let format = Format::from_extension(
                &path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
            .map_or("unknown", cx::format_name);
            cx.done(
                json!({
                    "file": shown(&path), "format": format, "encrypted": true,
                    "needs_password": true,
                }),
                &format!(
                    "{} is encrypted and needs a password: pass it with --password-env VAR or \
                     --password-stdin to see more.",
                    shown(&path)
                ),
            );
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let engine = cx.engine();
    let annotations = engine.annotations(id).map(|a| a.len()).ok();
    let is_pdf = info.format == Format::Pdf;
    let form_fields = (is_pdf && info.page_count <= FORM_COUNT_PAGES).then(|| {
        (0..info.page_count)
            .map(|p| engine.form_fields(id, p).map_or(0, |f| f.len()))
            .sum::<usize>()
    });
    engine.close(id);
    let password = cx.password()?;
    let signatures = if is_pdf {
        kitab_mupdf::sigs::signed_fields(&path, password.as_deref())
            .map(|s| s.len())
            .ok()
    } else {
        None
    };

    let size = |s: &kitab_core::SizePt| json!({"width": round1(s.w), "height": round1(s.h)});
    let first = info.page_sizes.first().copied().unwrap_or_default();
    let differ = info
        .page_sizes
        .iter()
        .any(|s| (s.w - first.w).abs() > 0.5 || (s.h - first.h).abs() > 0.5);
    let outline: Vec<Value> = info
        .outline
        .iter()
        .map(|o| json!({"title": o.title, "page": o.page.map(|p| p + 1)}))
        .collect();
    let perms = info.permissions;
    let mut fields = json!({
        "file": shown(&path),
        "format": cx::format_name(info.format),
        "title": info.title,
        "author": info.author,
        "pages": info.page_count,
        "page_size": {"width": round1(first.w), "height": round1(first.h), "unit": "pt"},
        "page_sizes_differ": differ,
        "encrypted": info.encryption.is_some(),
        "encryption": info.encryption,
        "needs_password": false,
        "permissions": {
            "print": perms.print, "copy": perms.copy, "annotate": perms.annotate,
            "fill_forms": perms.fill_forms, "modify": perms.modify, "assemble": perms.assemble,
        },
        "page_labels": info.page_labels.is_some(),
        "reflowable": info.reflowable,
        "repaired": info.repaired,
        "outline": outline,
        "annotations": annotations,
        "signatures": signatures,
        "form_fields": form_fields,
    });
    if differ && let Value::Object(m) = &mut fields {
        m.insert(
            "page_sizes".into(),
            info.page_sizes.iter().map(size).collect(),
        );
    }

    // The human version: a few short lines.
    let mut h = String::new();
    let _ = writeln!(
        h,
        "{}: {}, {}",
        shown(&path),
        cx::format_name(info.format).to_ascii_uppercase(),
        count(u64::from(info.page_count), "page", "pages")
    );
    if let Some(t) = &info.title {
        let _ = writeln!(h, "Title: {t}");
    }
    if let Some(a) = &info.author {
        let _ = writeln!(h, "Author: {a}");
    }
    let _ = writeln!(
        h,
        "Page size: {} x {} pt ({}){}",
        round1(first.w),
        round1(first.h),
        paper_name(first.w, first.h),
        if differ { "; other pages differ" } else { "" }
    );
    if info.reflowable {
        let _ = writeln!(h, "A book: pages are laid out at 400 x 600 pt.");
    }
    match &info.encryption {
        Some(e) => {
            let _ = writeln!(h, "Encrypted: {e}");
        }
        None => {
            let _ = writeln!(h, "Not encrypted");
        }
    }
    let denied: Vec<&str> = [
        ("print", perms.print),
        ("copy", perms.copy),
        ("annotate", perms.annotate),
        ("fill forms", perms.fill_forms),
        ("modify", perms.modify),
        ("assemble", perms.assemble),
    ]
    .iter()
    .filter(|(_, ok)| !ok)
    .map(|(n, _)| *n)
    .collect();
    if !denied.is_empty() {
        let _ = writeln!(h, "The owner doesn't allow: {}", denied.join(", "));
    }
    if info.page_labels.is_some() {
        let _ = writeln!(h, "Has page labels (i, ii, 1...)");
    }
    if info.repaired {
        let _ = writeln!(h, "The file was damaged and repaired while opening.");
    }
    if is_pdf {
        let mut counts = Vec::new();
        if let Some(n) = annotations {
            counts.push(count(n as u64, "annotation", "annotations"));
        }
        if let Some(n) = signatures {
            counts.push(count(n as u64, "signature", "signatures"));
        }
        if let Some(n) = form_fields {
            counts.push(count(n as u64, "form field", "form fields"));
        }
        if !counts.is_empty() {
            let _ = writeln!(h, "{}", counts.join(", "));
        }
    }
    if info.outline.is_empty() {
        let _ = writeln!(h, "No outline");
    } else {
        let _ = writeln!(h, "Outline:");
        for o in &info.outline {
            match o.page {
                Some(p) => {
                    let _ = writeln!(h, "  {} (page {})", o.title, p + 1);
                }
                None => {
                    let _ = writeln!(h, "  {}", o.title);
                }
            }
        }
    }
    cx.done(fields, &h);
    Ok(())
}

fn round1(v: f32) -> f64 {
    (f64::from(v) * 10.0).round() / 10.0
}

/// A familiar name for a page size, either way round.
fn paper_name(w: f32, h: f32) -> &'static str {
    let (a, b) = (w.min(h), w.max(h));
    let near = |x: f32, y: f32| (a - x).abs() < 3.0 && (b - y).abs() < 3.0;
    let shape = if w > h { " landscape" } else { "" };
    let name = if near(595.0, 842.0) {
        "A4"
    } else if near(612.0, 792.0) {
        "Letter"
    } else if near(420.0, 595.0) {
        "A5"
    } else if near(842.0, 1191.0) {
        "A3"
    } else if near(612.0, 1008.0) {
        "Legal"
    } else {
        return "custom";
    };
    match (name, shape) {
        ("A4", "") => "A4",
        ("A4", _) => "A4 landscape",
        ("Letter", "") => "Letter",
        ("Letter", _) => "Letter landscape",
        ("A5", "") => "A5",
        ("A5", _) => "A5 landscape",
        ("A3", "") => "A3",
        ("A3", _) => "A3 landscape",
        ("Legal", "") => "Legal",
        _ => "Legal landscape",
    }
}

/// The page ranges of `--pages`, checked against the document.
fn pages_flag(cx: &mut Cx, path: &Path) -> Result<Option<Vec<u32>>> {
    let Some(text) = cx.args.value("pages").map(str::to_string) else {
        // Still open it once for the right error (password, unreadable).
        let (id, _) = cx.open(path)?;
        cx.engine().close(id);
        return Ok(None);
    };
    let (id, info) = cx.open(path)?;
    cx.engine().close(id);
    cx::ranges(&text, info.page_count).map(Some)
}

pub fn text(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let path = cx.input_path(&file)?;
    let out = cx.args.value("out").map(PathBuf::from);
    if let Some(o) = &out {
        cx.check_out(o, &[&path])?;
    }
    let pages = pages_flag(cx, &path)?;
    let password = cx.password()?;
    let done = kitab_mupdf::export::to_text(&path, password.as_deref(), pages.as_deref())
        .map_err(CliError::engine)?;
    let n = count(u64::from(done.pages), "page", "pages");
    match out {
        Some(o) => {
            cx::write_atomic(&o, done.text.as_bytes())?;
            cx.done(
                json!({"pages": done.pages, "output": shown(&o), "bytes": done.text.len()}),
                &format!("Saved the text of {n} to {}", shown(&o)),
            );
        }
        None if cx.out.json => cx.done(json!({"pages": done.pages, "text": done.text}), ""),
        None => {
            cx.out.content(&done.text);
            if done.text.trim().is_empty() {
                cx.out.note(
                    "no text found: the pages may be scans (pictures); use `kitab render` to look",
                );
            }
        }
    }
    Ok(())
}

pub fn markdown(cx: &mut Cx) -> Result<()> {
    use kitab_mupdf::export::{ImageOut, MarkdownOptions, to_markdown};
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let path = cx.input_path(&file)?;
    if cx.args.has("images") && cx.args.has("no-images") {
        return Err(CliError::usage("use --images DIR or --no-images, not both"));
    }
    let out = cx.args.value("out").map(PathBuf::from);
    if let Some(o) = &out {
        cx.check_out(o, &[&path])?;
    }
    // Where pictures go: --images DIR; beside the output by default; none
    // on stdout unless asked.
    let images_dir: Option<PathBuf> = match (cx.args.value("images"), &out) {
        _ if cx.args.has("no-images") => None,
        (Some(dir), _) => Some(PathBuf::from(dir)),
        (None, Some(o)) => {
            let stem = o.file_stem().unwrap_or_default().to_string_lossy();
            Some(o.with_file_name(format!("{stem}_images")))
        }
        (None, None) => None,
    };
    let images = match &images_dir {
        Some(dir) => {
            cx.check_out_dir(dir)?;
            Some(ImageOut {
                dir: dir.clone(),
                link_prefix: link_prefix(dir, out.as_deref()),
            })
        }
        None => None,
    };
    let pages = pages_flag(cx, &path)?;
    let password = cx.password()?;
    let opts = MarkdownOptions {
        pages,
        images,
        page_markers: cx.args.has("page-markers"),
    };
    let done = match to_markdown(&path, password.as_deref(), &opts) {
        Ok(done) => done,
        Err(e) => {
            // Don't leave the empty picture folder made for it.
            if let Some(dir) = &images_dir {
                let _ = std::fs::remove_dir(dir);
            }
            return Err(CliError::engine(e));
        }
    };
    if let Some(dir) = &images_dir
        && done.images == 0
    {
        let _ = std::fs::remove_dir(dir);
    }
    let pics = count(u64::from(done.images), "picture", "pictures");
    let n = count(u64::from(done.pages), "page", "pages");
    let dir_json = images_dir
        .as_ref()
        .filter(|_| done.images > 0)
        .map(|d| shown(d));
    match out {
        Some(o) => {
            cx::write_atomic(&o, done.text.as_bytes())?;
            let mut human = format!("Saved {n} as Markdown to {}", shown(&o));
            if let Some(d) = &dir_json {
                let _ = write!(human, " ({pics} in {d})");
            }
            cx.done(
                json!({"pages": done.pages, "images": done.images, "output": shown(&o),
                       "images_dir": dir_json}),
                &human,
            );
        }
        None if cx.out.json => cx.done(
            json!({"pages": done.pages, "images": done.images, "images_dir": dir_json,
                   "markdown": done.text}),
            "",
        ),
        None => {
            cx.out.content(&done.text);
            if done.text.trim().is_empty() {
                cx.out.note(
                    "no text found: the pages may be scans (pictures); use `kitab render` to look",
                );
            }
        }
    }
    Ok(())
}

/// How the Markdown links a picture folder: relative to the Markdown file
/// when the folder is beside or under it, else the folder as given.
fn link_prefix(dir: &Path, out: Option<&Path>) -> String {
    let base = out.and_then(Path::parent).unwrap_or(Path::new(""));
    let rel = if base.as_os_str().is_empty() {
        Some(dir.to_path_buf())
    } else {
        dir.strip_prefix(base).ok().map(Path::to_path_buf)
    };
    let link = rel.unwrap_or_else(|| dir.to_path_buf());
    let mut s = shown(&link);
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

pub fn search(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    let query = cx.arg(1, "QUERY")?;
    cx.no_more_args(2)?;
    if query.trim().is_empty() {
        return Err(CliError::usage("the search text is empty"));
    }
    let limit: usize = match cx.args.value("limit") {
        Some(v) => v
            .parse()
            .map_err(|_| CliError::usage(format!("--limit: \"{v}\" isn't a number")))?,
        None => 200,
    };
    let path = cx.input_path(&file)?;
    let (id, info) = cx.open(&path)?;
    let options = FindOptions {
        match_case: cx.args.has("case"),
        whole_word: cx.args.has("whole-word"),
        ignore_diacritics: true,
    };
    let mut hits: Vec<Value> = Vec::new();
    let mut lines = String::new();
    let mut truncated = false;
    let engine = cx.engine();
    engine
        .find(id, &query, options, |hit| {
            if limit > 0 && hits.len() >= limit {
                truncated = true;
                return false;
            }
            let text = hit.context.trim().to_string();
            let _ = writeln!(lines, "page {}: {text}", hit.page + 1);
            let rects: Vec<Value> = hit
                .quads
                .iter()
                .map(|r| json!([round1(r.x), round1(r.y), round1(r.w), round1(r.h)]))
                .collect();
            hits.push(json!({"page": hit.page + 1, "text": text, "rects": rects}));
            true
        })
        .map_err(CliError::failed)?;
    let mut fields = json!({
        "query": query, "count": hits.len(), "truncated": truncated, "hits": hits,
    });
    let mut human = if hits.is_empty() {
        format!("No matches for \"{query}\".")
    } else {
        let mut s = lines;
        let _ = write!(
            s,
            "{}{}",
            count(hits.len() as u64, "hit", "hits"),
            if truncated {
                format!(" (stopped at --limit {limit})")
            } else {
                String::new()
            }
        );
        s
    };
    if hits.is_empty() {
        // Scans have no text to search: say so, so the agent renders them.
        let bare = (0..info.page_count)
            .filter(|&p| engine.page_has_text(id, p).is_ok_and(|t| !t))
            .count();
        if let Value::Object(m) = &mut fields {
            m.insert("pages_without_text".into(), json!(bare));
        }
        if bare > 0 {
            let _ = write!(
                human,
                "\n{} no text (scanned pictures); use `kitab render` to look at them.",
                if bare == 1 {
                    "1 page has".to_string()
                } else {
                    format!("{bare} pages have")
                }
            );
        }
    }
    engine.close(id);
    cx.done(fields, &human);
    Ok(())
}

/// The widest picture `render` makes.
const MAX_RENDER_PX: u32 = 10_000;

pub fn render(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    if cx.args.has("width") && cx.args.has("dpi") {
        return Err(CliError::usage("use --width or --dpi, not both"));
    }
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    cx.check_out(&out, &[&path])?;
    let (id, info) = cx.open(&path)?;
    let page = match cx.args.value("page") {
        Some(p) => cx::page_number(p, info.page_count, "--page")?,
        None => 0,
    };
    let size = info
        .page_sizes
        .get(page as usize)
        .copied()
        .unwrap_or_default();
    if !(size.w >= 1.0 && size.h >= 1.0) {
        return Err(CliError::failed("the page has no size"));
    }
    let scale = if let Some(d) = cx.args.value("dpi") {
        let dpi: f32 = d
            .parse()
            .ok()
            .filter(|v: &f32| v.is_finite() && *v > 0.0)
            .ok_or_else(|| CliError::usage(format!("--dpi: \"{d}\" isn't a resolution")))?;
        dpi / 72.0
    } else {
        let width: u32 = match cx.args.value("width") {
            Some(w) => w
                .parse()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| CliError::usage(format!("--width: \"{w}\" isn't a width")))?,
            None => 1200,
        };
        width as f32 / size.w
    };
    let (pw, ph) = (size.w * scale, size.h * scale);
    if pw > MAX_RENDER_PX as f32 || ph > MAX_RENDER_PX as f32 {
        return Err(CliError::failed(format!(
            "that would be {} x {} pixels; the most is {MAX_RENDER_PX} on a side",
            pw.round(),
            ph.round()
        )));
    }
    let req = RenderRequest {
        page,
        scale,
        rotation: 0,
        tile: None,
        generation: 0,
    };
    let bitmap = cx
        .engine()
        .render(id, req)
        .map_err(CliError::failed)?
        .ok_or_else(|| CliError::failed("the page could not be drawn"))?;
    cx.engine().close(id);
    kitab_mupdf::cli_ops::write_png(&bitmap, &out).map_err(CliError::failed)?;
    cx.done(
        json!({"page": page + 1, "output": shown(&out), "width": bitmap.width,
               "height": bitmap.height}),
        &format!(
            "Rendered page {} ({} x {} px) to {}",
            page + 1,
            bitmap.width,
            bitmap.height,
            shown(&out)
        ),
    );
    Ok(())
}

/// "#RRGGBB" for a colour.
pub fn hex(c: kitab_core::Rgb8) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

pub fn annotations_list(cx: &mut Cx, file: &str) -> Result<()> {
    let path = cx.input_path(file)?;
    let (id, info) = cx.open(&path)?;
    if info.format != Format::Pdf {
        cx.engine().close(id);
        return Err(CliError::failed(
            "annotations are read from PDF files only".to_string(),
        ));
    }
    let list = cx.engine().annotations(id).map_err(CliError::failed)?;
    cx.engine().close(id);
    let mut human = String::new();
    let items: Vec<Value> = list
        .iter()
        .map(|a| {
            let rect = [
                round1(a.rect.x),
                round1(a.rect.y),
                round1(a.rect.w),
                round1(a.rect.h),
            ];
            let _ = writeln!(
                human,
                "page {} #{}: {}{} at [{}, {}, {}, {}]",
                a.page + 1,
                a.index + 1,
                a.kind,
                a.text
                    .as_ref()
                    .map(|t| format!(" \"{}\"", one_line(t)))
                    .unwrap_or_default(),
                rect[0],
                rect[1],
                rect[2],
                rect[3]
            );
            json!({
                "page": a.page + 1, "index": a.index + 1, "kind": a.kind, "text": a.text,
                "rect": rect, "color": a.color.map(hex),
            })
        })
        .collect();
    let _ = write!(
        human,
        "{}",
        count(list.len() as u64, "annotation", "annotations")
    );
    cx.done(json!({"count": items.len(), "annotations": items}), &human);
    Ok(())
}

fn one_line(t: &str) -> String {
    let flat: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 80 {
        format!("{}...", flat.chars().take(77).collect::<String>())
    } else {
        flat
    }
}

/// Where Kitab keeps the certificates trusted in Settings › Signing.
fn trust_dir() -> Option<PathBuf> {
    kitab_os::data_dir().map(|d| d.join("trusted-certs"))
}

fn status_word(s: SigStatus) -> &'static str {
    match s {
        SigStatus::Valid => "valid",
        SigStatus::Unknown => "signer-unknown",
        SigStatus::Changed => "changed-after-signing",
        SigStatus::Broken => "invalid",
    }
}

pub fn verify(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let path = cx.input_path(&file)?;
    cx.open_pdf(&path)?;
    let password = cx.password()?;
    let fields =
        kitab_mupdf::sigs::signed_fields(&path, password.as_deref()).map_err(CliError::engine)?;
    if fields.is_empty() {
        cx.done(
            json!({"count": 0, "status": Value::Null, "signatures": []}),
            &format!("{} has no digital signatures.", shown(&path)),
        );
        return Ok(());
    }
    let mut trust = trust_dir()
        .map(|d| kitab_sign::store::load(&d))
        .unwrap_or_default();
    for cert in cx.args.values("trust") {
        let bytes =
            std::fs::read(cert).map_err(|e| CliError::not_found(format!("--trust {cert}: {e}")))?;
        trust
            .add(&bytes)
            .map_err(|e| CliError::failed(format!("--trust {cert}: {e}")))?;
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| CliError::unreadable(format!("{}: {e}", path.display())))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let checks: Vec<kitab_core::SignatureCheck> = fields
        .iter()
        .map(|f| kitab_sign::verify(&bytes, f, &trust, now))
        .collect();
    let mut human = String::new();
    let sigs: Vec<Value> = checks
        .iter()
        .map(|c| {
            let status = status_word(c.status());
            let whole = c.coverage == SigCoverage::EntireFile;
            let changed = match c.modification {
                SigModification::None => "none",
                SigModification::FormFilling => "form-filling",
                SigModification::Annotations => "annotations",
                SigModification::Other => "other",
            };
            let who = c
                .signer_name
                .clone()
                .or_else(|| c.signer.clone())
                .unwrap_or_else(|| "an unknown signer".into());
            let _ = writeln!(
                human,
                "{}: {status}, signed by {who}{}{}{}",
                c.field,
                c.when
                    .as_ref()
                    .map(|w| format!(" on {w}"))
                    .unwrap_or_default(),
                if whole {
                    ", covers the whole document"
                } else {
                    ", doesn't cover the whole document"
                },
                c.problem
                    .as_ref()
                    .map(|p| format!(" ({p})"))
                    .unwrap_or_default()
            );
            json!({
                "field": c.field,
                "page": c.page.map(|p| p + 1),
                "signer": c.signer,
                "signer_name": c.signer_name,
                "status": status,
                "intact": c.intact,
                "signature_valid": c.valid,
                "trusted": c.trusted,
                "expired": c.expired,
                "covers_whole_document": whole,
                "changed_after": changed,
                "time": c.when,
                "reason": c.reason,
                "location": c.location,
                "problem": c.problem,
            })
        })
        .collect();
    let overall = kitab_core::overall_status(&checks).map(status_word);
    let _ = write!(
        human,
        "{}; overall: {}",
        count(sigs.len() as u64, "signature", "signatures"),
        overall.unwrap_or("none")
    );
    cx.done(
        json!({"count": sigs.len(), "status": overall, "signatures": sigs}),
        &human,
    );
    Ok(())
}
