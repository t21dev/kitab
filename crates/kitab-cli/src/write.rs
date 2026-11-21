// SPDX-License-Identifier: AGPL-3.0-or-later

//! The commands that write a new PDF: pages, merge, split, compress,
//! password, images-to-pdf, images, watermark and number. Each checks its
//! input and output first (exit codes 3, 4, 5), then calls the engine tool,
//! which writes beside the target and renames.

use crate::cx::{self, Cx, shown};
use crate::out::{CliError, Result, count};
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn pages(cx: &mut Cx) -> Result<()> {
    let sub = cx.arg(0, "what to do: keep, delete, rotate, move or insert")?;
    cx.command = format!("pages {sub}");
    let file = cx.arg(1, "FILE")?;
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    let total = cx.open_pdf(&path)?;
    let pw = cx.password()?;
    let pw = pw.as_deref();
    let engine = |e: String| CliError::engine(e);
    let n = |c: usize| count(c as u64, "page", "pages");
    let (pages, changed, human) = match sub.as_str() {
        "keep" | "extract" => {
            let list = cx::ranges(&cx.arg(2, "RANGES")?, total)?;
            cx.no_more_args(3)?;
            cx.check_out(&out, &[&path])?;
            kitab_mupdf::cli_ops::arrange_pages(&path, pw, &list, &out).map_err(engine)?;
            let k = list.len();
            (k, k, format!("Kept {} of {total} in {}", n(k), shown(&out)))
        }
        "delete" => {
            let list = cx::ranges(&cx.arg(2, "RANGES")?, total)?;
            cx.no_more_args(3)?;
            cx.check_out(&out, &[&path])?;
            let left =
                kitab_mupdf::pages::delete_pages(&path, pw, &list, &out).map_err(engine)? as usize;
            let msg = format!(
                "Deleted {}; {} left in {}",
                n(list.len()),
                n(left),
                shown(&out)
            );
            (left, list.len(), msg)
        }
        "rotate" => {
            let list = cx::ranges(&cx.arg(2, "RANGES")?, total)?;
            cx.no_more_args(3)?;
            let by = cx
                .args
                .value("by")
                .ok_or_else(|| CliError::usage("pages rotate needs --by 90, 180 or 270"))?;
            let deg: i32 = match by {
                "90" => 90,
                "180" => 180,
                "270" | "-90" => 270,
                _ => {
                    return Err(CliError::usage(format!(
                        "--by: \"{by}\" isn't 90, 180 or 270"
                    )));
                }
            };
            cx.check_out(&out, &[&path])?;
            kitab_mupdf::pages::rotate_pages(&path, pw, &list, deg, &out).map_err(engine)?;
            let msg = format!(
                "Turned {} by {deg} degrees in {}",
                n(list.len()),
                shown(&out)
            );
            (total as usize, list.len(), msg)
        }
        "move" => {
            let list = cx::ranges(&cx.arg(2, "RANGES")?, total)?;
            cx.no_more_args(3)?;
            let before = cx
                .args
                .value("before")
                .ok_or_else(|| CliError::usage("pages move needs --before N (or --before end)"))?;
            let before = cx::before_page(before, total)?;
            cx.check_out(&out, &[&path])?;
            let order = kitab_mupdf::pages::moved_order(total, &list, before);
            kitab_mupdf::cli_ops::arrange_pages(&path, pw, &order, &out).map_err(engine)?;
            let place = if before >= total {
                "the end".to_string()
            } else {
                format!("before page {}", before + 1)
            };
            let msg = format!("Moved {} to {place} in {}", n(list.len()), shown(&out));
            (total as usize, list.len(), msg)
        }
        "insert" => {
            cx.no_more_args(2)?;
            let from = cx
                .args
                .value("from")
                .ok_or_else(|| CliError::usage("pages insert needs --from OTHER.pdf"))?
                .to_string();
            let before = cx
                .args
                .value("before")
                .ok_or_else(|| CliError::usage("pages insert needs --before N (or --before end)"))?
                .to_string();
            let before = cx::before_page(&before, total)?;
            let from_path = cx.input_path(&from)?;
            let from_total = cx.open_pdf(&from_path)?;
            let picked = match cx.args.value("from-pages") {
                Some(r) => Some(cx::ranges(r, from_total)?),
                None => None,
            };
            cx.check_out(&out, &[&path, &from_path])?;
            let (added, now) = kitab_mupdf::cli_ops::insert_pages(
                &path,
                pw,
                &from_path,
                pw,
                picked.as_deref(),
                before,
                &out,
            )
            .map_err(engine)?;
            let msg = format!(
                "Inserted {} from {}; {} in {}",
                n(added as usize),
                shown(&from_path),
                n(now as usize),
                shown(&out)
            );
            (now as usize, added as usize, msg)
        }
        other => {
            return Err(CliError::usage(format!(
                "unknown pages command '{other}': use keep, delete, rotate, move or insert"
            )));
        }
    };
    cx.done(
        json!({"output": shown(&out), "pages": pages, "changed": changed}),
        &human,
    );
    Ok(())
}

pub fn merge(cx: &mut Cx) -> Result<()> {
    let files = cx.args.positional.clone();
    if files.len() < 2 {
        return Err(CliError::usage(
            "merge needs at least two PDFs: kitab merge A.pdf B.pdf -o OUT.pdf",
        ));
    }
    let out = cx.out_path()?;
    let mut paths = Vec::new();
    for f in &files {
        let p = cx.input_path(f)?;
        cx.open_pdf(&p)?;
        paths.push(p);
    }
    let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    cx.check_out(&out, &refs)?;
    let pw = cx.password()?;
    let inputs: Vec<(PathBuf, Option<String>)> =
        paths.iter().map(|p| (p.clone(), pw.clone())).collect();
    let pages = kitab_mupdf::tools::merge(&inputs, &out).map_err(CliError::engine)?;
    cx.done(
        json!({"output": shown(&out), "inputs": paths.len(), "pages": pages}),
        &format!(
            "Merged {} PDFs ({}) into {}",
            paths.len(),
            count(u64::from(pages), "page", "pages"),
            shown(&out)
        ),
    );
    Ok(())
}

pub fn split(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let path = cx.input_path(&file)?;
    let dir = cx.out_path()?;
    let total = cx.open_pdf(&path)?;
    let groups: Vec<Vec<u32>> = match (cx.args.value("every"), cx.args.value("ranges")) {
        (Some(_), Some(_)) => return Err(CliError::usage("use --every N or --ranges, not both")),
        (Some(n), None) => {
            let n: u32 =
                n.parse().ok().filter(|n| *n > 0).ok_or_else(|| {
                    CliError::usage(format!("--every: \"{n}\" isn't a page count"))
                })?;
            (0..total)
                .collect::<Vec<_>>()
                .chunks(n as usize)
                .map(<[u32]>::to_vec)
                .collect()
        }
        (None, Some(r)) => r
            .split([',', ';'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|part| cx::ranges(part, total))
            .collect::<Result<_>>()?,
        (None, None) => {
            return Err(CliError::usage(
                "split needs --every N or --ranges \"1-3,4-9\"",
            ));
        }
    };
    if groups.is_empty() {
        return Err(CliError::usage("--ranges names no pages"));
    }
    if dir.is_file() {
        return Err(CliError::usage(format!(
            "{} is a file; give a folder with -o",
            dir.display()
        )));
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "part".into());
    let outs: Vec<PathBuf> = (1..=groups.len())
        .map(|i| dir.join(format!("{stem}-{i}.pdf")))
        .collect();
    for o in &outs {
        cx.check_out(o, &[&path])?;
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| CliError::failed(format!("could not create {}: {e}", dir.display())))?;
    let pw = cx.password()?;
    let mut files = Vec::new();
    for (pages, o) in groups.iter().zip(&outs) {
        kitab_mupdf::cli_ops::arrange_pages(&path, pw.as_deref(), pages, o)
            .map_err(CliError::engine)?;
        files.push(json!({"output": shown(o), "pages": pages.len()}));
    }
    cx.done(
        json!({"dir": shown(&dir), "files": files}),
        &format!(
            "Split {} into {} files in {} ({}-1.pdf...)",
            shown(&path),
            outs.len(),
            shown(&dir),
            stem
        ),
    );
    Ok(())
}

pub fn compress(cx: &mut Cx) -> Result<()> {
    use kitab_mupdf::compress::{Strength, compress_pdf};
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let strength = match cx.args.value("quality").unwrap_or("balanced") {
        "balanced" => Strength::Balanced,
        "smallest" => Strength::Smallest,
        "same" | "lossless" => Strength::Lossless,
        q => {
            return Err(CliError::usage(format!(
                "--quality: \"{q}\" isn't balanced, smallest or same"
            )));
        }
    };
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    cx.open_pdf(&path)?;
    cx.check_out(&out, &[&path])?;
    let pw = cx.password()?;
    let done = compress_pdf(&path, pw.as_deref(), strength, &out).map_err(CliError::engine)?;
    let smaller = done.after < done.before;
    let pct = (done.after * 100)
        .checked_div(done.before)
        .map_or(0, |share| 100 - share.min(100));
    let pictures = match done.pictures {
        0 => String::new(),
        n => format!(
            ", {} made smaller",
            count(u64::from(n), "picture", "pictures")
        ),
    };
    let human = if smaller {
        format!(
            "Compressed {} to {}: {} -> {} ({pct}% smaller{pictures})",
            shown(&path),
            shown(&out),
            crate::out::size(done.before),
            crate::out::size(done.after)
        )
    } else {
        format!(
            "Wrote {} but it isn't smaller ({} -> {}): this PDF is already compact",
            shown(&out),
            crate::out::size(done.before),
            crate::out::size(done.after)
        )
    };
    cx.done(
        json!({"output": shown(&out), "bytes_before": done.before, "bytes_after": done.after,
               "smaller": smaller, "pictures_shrunk": done.pictures}),
        &human,
    );
    Ok(())
}

pub fn password(cx: &mut Cx) -> Result<()> {
    let sub = cx.arg(0, "what to do: set or remove")?;
    cx.command = format!("password {sub}");
    let file = cx.arg(1, "FILE")?;
    cx.no_more_args(2)?;
    if cx.args.has("new-password") {
        return Err(CliError::usage(
            "the new password is never taken as an argument: use --new-password-env VAR or \
             --new-password-stdin",
        ));
    }
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    cx.open_pdf(&path)?;
    cx.check_out(&out, &[&path])?;
    let pw = cx.password()?;
    let (new, human) = match sub.as_str() {
        "set" => {
            let new = cx
                .secret("new-password", "the new password")?
                .ok_or_else(|| {
                    CliError::usage(
                        "password set needs --new-password-env VAR or --new-password-stdin",
                    )
                })?;
            (Some(new), format!("Locked {} with a password", shown(&out)))
        }
        "remove" => (None, format!("Saved {} without a password", shown(&out))),
        other => {
            return Err(CliError::usage(format!(
                "unknown password command '{other}': use set or remove"
            )));
        }
    };
    kitab_mupdf::tools::set_password(&path, pw.as_deref(), new.as_deref(), &out)
        .map_err(CliError::engine)?;
    cx.done(
        json!({"output": shown(&out), "encrypted": new.is_some()}),
        &human,
    );
    Ok(())
}

pub fn images_to_pdf(cx: &mut Cx) -> Result<()> {
    let files = cx.args.positional.clone();
    if files.is_empty() {
        return Err(CliError::usage(
            "images-to-pdf needs pictures: kitab images-to-pdf A.jpg B.png -o OUT.pdf",
        ));
    }
    let out = cx.out_path()?;
    let paths: Vec<PathBuf> = files
        .iter()
        .map(|f| cx.input_path(f))
        .collect::<Result<_>>()?;
    let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    cx.check_out(&out, &refs)?;
    let pages = kitab_mupdf::tools::images_to_pdf(&paths, &out).map_err(CliError::failed)?;
    cx.done(
        json!({"output": shown(&out), "pages": pages}),
        &format!(
            "Made {} from {}",
            shown(&out),
            count(u64::from(pages), "picture", "pictures")
        ),
    );
    Ok(())
}

pub fn images(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let path = cx.input_path(&file)?;
    let dir = cx.out_path()?;
    cx.open_pdf(&path)?;
    cx.check_out_dir(&dir)?;
    let pw = cx.password()?;
    let before: Vec<PathBuf> = listing(&dir);
    let n =
        kitab_mupdf::pages::extract_images(&path, pw.as_deref(), &dir).map_err(CliError::engine)?;
    let mut files: Vec<String> = listing(&dir)
        .into_iter()
        .filter(|p| !before.contains(p) || cx.force())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .map(|p| shown(&p))
        .collect();
    files.sort();
    let human = if n == 0 {
        format!("{} has no pictures", shown(&path))
    } else {
        format!(
            "Saved {} to {}",
            count(u64::from(n), "picture", "pictures"),
            shown(&dir)
        )
    };
    cx.done(
        json!({"dir": shown(&dir), "images": n, "files": files}),
        &human,
    );
    Ok(())
}

fn listing(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

pub fn watermark(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    let text = cx.arg(1, "TEXT (the watermark)")?;
    cx.no_more_args(2)?;
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    cx.open_pdf(&path)?;
    cx.check_out(&out, &[&path])?;
    let pw = cx.password()?;
    let pages = kitab_mupdf::pages::watermark(&path, pw.as_deref(), &text, &out)
        .map_err(CliError::engine)?;
    cx.done(
        json!({"output": shown(&out), "pages": pages}),
        &format!(
            "Watermarked {} in {}",
            count(u64::from(pages), "page", "pages"),
            shown(&out)
        ),
    );
    Ok(())
}

pub fn number(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    let first: u32 = match cx.args.value("start") {
        Some(s) => s
            .parse()
            .map_err(|_| CliError::usage(format!("--start: \"{s}\" isn't a number")))?,
        None => 1,
    };
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    cx.open_pdf(&path)?;
    cx.check_out(&out, &[&path])?;
    let pw = cx.password()?;
    let pages = kitab_mupdf::pages::number_pages(&path, pw.as_deref(), first, &out)
        .map_err(CliError::engine)?;
    cx.done(
        json!({"output": shown(&out), "pages": pages, "first": first}),
        &format!(
            "Numbered {} from {first} in {}",
            count(u64::from(pages), "page", "pages"),
            shown(&out)
        ),
    );
    Ok(())
}
