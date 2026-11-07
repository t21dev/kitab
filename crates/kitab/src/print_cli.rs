// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kitab --print <file> [--printer NAME] [--to FILE]` (PLAN.md §3.8): print
//! a document without opening a window, to the default printer or a named
//! one. Explorer's Print verb runs this. `--to` sends the printer driver's
//! output to a file (Microsoft Print to PDF, in tests). On macOS and Linux
//! the pages go to CUPS as a PDF, and `--to` keeps that PDF instead.

use kitab_print::{Job, PageSize, Printer, Scale, print_pages};
use std::path::PathBuf;

/// Run the command; the process's exit code.
pub fn run(args: &[String]) -> i32 {
    match print(args) {
        Ok(n) => {
            eprintln!("kitab: printed {n} pages");
            0
        }
        Err(e) => {
            eprintln!("kitab: couldn't print: {e}");
            1
        }
    }
}

fn print(args: &[String]) -> Result<usize, String> {
    let mut file = None;
    let mut printer_name = None;
    let mut to = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--printer" => printer_name = it.next().cloned(),
            "--to" => to = it.next().map(PathBuf::from),
            other if file.is_none() && !other.starts_with("--") => {
                file = Some(PathBuf::from(other))
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let file = file.ok_or("which file?")?;
    let engine = kitab_mupdf::Engine::new(kitab_mupdf::Engine::default_threads().min(2));
    let (doc, info) = engine.open(&file, None).map_err(|e| format!("{e:?}"))?;
    if !info.permissions.print {
        engine.close(doc);
        return Err("this PDF's owner doesn't allow printing it".into());
    }
    let pages: Vec<(u32, PageSize)> = info
        .page_sizes
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u32, PageSize { w: s.w, h: s.h }))
        .collect();
    let title = info.title.clone().unwrap_or_else(|| {
        file.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    if !kitab_print::SYSTEM_DIALOG {
        // macOS and Linux: the pages as a PDF, to CUPS (or to `--to`).
        let list: Vec<u32> = pages.iter().map(|(p, _)| *p).collect();
        let tmp = to.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("kitab-print-{}.pdf", std::process::id()))
        });
        let written = engine.write_pdf(doc, list, tmp.clone());
        engine.close(doc);
        let n = written? as usize;
        if to.is_some() {
            return Ok(n);
        }
        let printer = match printer_name {
            Some(name) => name,
            None => kitab_print::printers()?
                .into_iter()
                .find(|p| p.is_default)
                .map(|p| p.name)
                .ok_or("there is no default printer")?,
        };
        let sent = kitab_print::print_pdf(&printer, &tmp, &title, 1, Scale::Fit, true);
        let _ = std::fs::remove_file(&tmp);
        return sent.map(|_| n);
    }
    let mut printer = match printer_name {
        Some(name) => Printer::named(&name)?,
        None => Printer::default_printer()?,
    };
    let job = Job {
        title: &title,
        to_file: to.as_deref(),
        pages: &pages,
        scale: Scale::Fit,
        auto_rotate: true,
    };
    let result = print_pages(
        &mut printer,
        &job,
        |page, scale, rotation| {
            let req = kitab_core::RenderRequest {
                page,
                scale,
                rotation,
                tile: None,
                generation: 0,
            };
            match engine.render(doc, req)? {
                Some(b) => Ok((b.bgra, b.width, b.height)),
                None => Err("it didn't draw".to_string()),
            }
        },
        |_| {},
    );
    engine.close(doc);
    result
}
