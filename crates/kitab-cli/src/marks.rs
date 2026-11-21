// SPDX-License-Identifier: AGPL-3.0-or-later

//! Annotations and signatures: annotate, annotations list/remove, sign.
//! Coordinates are page points from the page's top-left corner, y down, as
//! in kitab-core's markup (the same space MuPDF's text boxes use).

use crate::cx::{self, Cx, shown};
use crate::out::{CliError, Result, count};
use kitab_core::{
    AnnotEdit, FindOptions, Mark, MarkShape, PointPt, RectPt, Rgb8, TextAlign, TextStyle,
};
use serde_json::{Value, json};

const YELLOW: Rgb8 = Rgb8 {
    r: 255,
    g: 230,
    b: 0,
};
const RED: Rgb8 = Rgb8 {
    r: 220,
    g: 30,
    b: 30,
};
const BLACK: Rgb8 = Rgb8 { r: 0, g: 0, b: 0 };

const KINDS: &[&str] = &["highlight", "underline", "strike", "note", "textbox"];

/// "#RRGGBB" (the # optional).
fn colour(text: &str) -> Result<Rgb8> {
    let hex = text.trim().trim_start_matches('#');
    let bad = || CliError::usage(format!("--color: \"{text}\" isn't a colour like #FF8800"));
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    Ok(Rgb8 {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
    })
}

fn rect_json(r: &RectPt) -> Value {
    let k = |v: f32| (f64::from(v) * 10.0).round() / 10.0;
    json!([k(r.x), k(r.y), k(r.w), k(r.h)])
}

pub fn annotate(cx: &mut Cx) -> Result<()> {
    // The kind may come before or after the file.
    let pos = cx.args.positional.clone();
    let kind_at = pos
        .iter()
        .position(|p| KINDS.contains(&p.as_str()))
        .ok_or_else(|| {
            CliError::usage(
                "annotate needs a kind: highlight, underline, strike, note or textbox \
                 (see: kitab help annotate)",
            )
        })?;
    let kind = pos[kind_at].clone();
    let rest: Vec<&String> = pos
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != kind_at)
        .map(|(_, p)| p)
        .collect();
    let file = match rest.as_slice() {
        [f] => (*f).clone(),
        [] => return Err(CliError::usage("missing FILE (see: kitab help annotate)")),
        [_, extra, ..] => {
            return Err(CliError::usage(format!(
                "unexpected argument '{extra}' (see: kitab help annotate)"
            )));
        }
    };
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    let total = cx.open_pdf(&path)?;
    let page_text = cx
        .args
        .value("page")
        .ok_or_else(|| CliError::usage("annotate needs --page N"))?
        .to_string();
    let page = cx::page_number(&page_text, total, "--page")?;
    let text = cx
        .args
        .value("text")
        .ok_or_else(|| CliError::usage("annotate needs --text"))?
        .to_string();
    let color = cx
        .args
        .value("color")
        .or(cx.args.value("colour"))
        .map(colour)
        .transpose()?;
    let opacity: f32 = match cx.args.value("opacity") {
        Some(o) => o
            .parse()
            .ok()
            .filter(|v| kitab_core::OPACITY_RANGE.contains(v))
            .ok_or_else(|| CliError::usage(format!("--opacity: \"{o}\" isn't 0.05 to 1")))?,
        None => 1.0,
    };
    cx.check_out(&out, &[&path])?;

    let mark = |shape: MarkShape, default: Rgb8| Mark {
        page,
        shape,
        color: color.unwrap_or(default),
        width: 1.0,
        opacity,
    };
    let marks: Vec<Mark> = match kind.as_str() {
        "highlight" | "underline" | "strike" => {
            let rects = find_phrase(cx, &path, page, &text)?;
            let shape = match kind.as_str() {
                "highlight" => MarkShape::Highlight { rects },
                "underline" => MarkShape::Underline { rects },
                _ => MarkShape::Strike { rects },
            };
            let default = if kind == "highlight" { YELLOW } else { RED };
            vec![mark(shape, default)]
        }
        "note" => {
            let at = cx
                .args
                .value("at")
                .ok_or_else(|| CliError::usage("annotate note needs --at X,Y"))?;
            let v = cx::numbers(at, 2, "--at", "X,Y in points")?;
            vec![mark(
                MarkShape::Note {
                    at: PointPt { x: v[0], y: v[1] },
                    text,
                },
                YELLOW,
            )]
        }
        _ => {
            let r = cx
                .args
                .value("rect")
                .ok_or_else(|| CliError::usage("annotate textbox needs --rect X,Y,W,H"))?;
            let v = cx::numbers(r, 4, "--rect", "X,Y,W,H in points")?;
            if v[2] <= 0.0 || v[3] <= 0.0 {
                return Err(CliError::usage(
                    "--rect: the width and height must be above 0",
                ));
            }
            let size: f32 = match cx.args.value("size") {
                Some(s) => s
                    .parse()
                    .ok()
                    .filter(|v| (4.0..=144.0).contains(v))
                    .ok_or_else(|| CliError::usage(format!("--size: \"{s}\" isn't 4 to 144")))?,
                None => 12.0,
            };
            let align = match cx.args.value("align").unwrap_or("left") {
                "left" => TextAlign::Left,
                "centre" | "center" => TextAlign::Centre,
                "right" => TextAlign::Right,
                a => {
                    return Err(CliError::usage(format!(
                        "--align: \"{a}\" isn't left, centre or right"
                    )));
                }
            };
            let style = TextStyle {
                bold: cx.args.has("bold"),
                italic: cx.args.has("italic"),
                underline: cx.args.has("underline"),
                strike: cx.args.has("strike"),
                align,
                ..TextStyle::default()
            };
            vec![mark(
                MarkShape::Text {
                    rect: RectPt {
                        x: v[0],
                        y: v[1],
                        w: v[2],
                        h: v[3],
                    },
                    text,
                    size,
                    style,
                },
                BLACK,
            )]
        }
    };
    if marks.iter().any(|m| !m.is_valid()) {
        return Err(CliError::usage(
            "the annotation is out of range (coordinates, size or text length)",
        ));
    }
    let rects: Vec<Value> = marks
        .iter()
        .flat_map(|m| match &m.shape {
            MarkShape::Highlight { rects }
            | MarkShape::Underline { rects }
            | MarkShape::Strike { rects } => rects.iter().map(rect_json).collect(),
            other => vec![rect_json(
                &Mark {
                    shape: other.clone(),
                    ..m.clone()
                }
                .bounds(),
            )],
        })
        .collect();
    let pw = cx.password()?;
    let added = kitab_mupdf::marks::write_marks(&path, pw.as_deref(), &marks, &[], &out)
        .map_err(CliError::engine)?;
    cx.done(
        json!({"kind": kind, "page": page + 1, "output": shown(&out), "added": added,
               "rects": rects}),
        &format!("Added a {kind} on page {} in {}", page + 1, shown(&out)),
    );
    Ok(())
}

/// The boxes of `phrase` on `page`: its first occurrence, or all with --all.
fn find_phrase(
    cx: &mut Cx,
    path: &std::path::Path,
    page: u32,
    phrase: &str,
) -> Result<Vec<RectPt>> {
    if phrase.trim().is_empty() {
        return Err(CliError::usage("--text is empty"));
    }
    let all = cx.args.has("all");
    let (id, _) = cx.open(path)?;
    let options = FindOptions {
        match_case: false,
        whole_word: false,
        ignore_diacritics: true,
    };
    let hits = cx.engine().find_page(id, page, phrase, options);
    cx.engine().close(id);
    let hits = hits.map_err(CliError::failed)?;
    let rects: Vec<RectPt> = if all {
        hits.into_iter().flat_map(|h| h.quads).collect()
    } else {
        hits.into_iter().next().map(|h| h.quads).unwrap_or_default()
    };
    if rects.is_empty() {
        return Err(CliError::new(
            crate::out::EXIT_FAILED,
            "not_found_on_page",
            format!(
                "\"{phrase}\" isn't on page {} (try `kitab search` to find its page)",
                page + 1
            ),
        ));
    }
    Ok(rects)
}

pub fn annotations(cx: &mut Cx) -> Result<()> {
    let sub = cx.arg(0, "what to do: list or remove")?;
    cx.command = format!("annotations {sub}");
    let file = cx.arg(1, "FILE")?;
    cx.no_more_args(2)?;
    match sub.as_str() {
        "list" => crate::read::annotations_list(cx, &file),
        "remove" => remove(cx, &file),
        other => Err(CliError::usage(format!(
            "unknown annotations command '{other}': use list or remove"
        ))),
    }
}

fn remove(cx: &mut Cx, file: &str) -> Result<()> {
    let path = cx.input_path(file)?;
    let out = cx.out_path()?;
    let all = cx.args.has("all");
    let page = cx.args.value("page").map(str::to_string);
    let index = cx.args.value("index").map(str::to_string);
    match (all, &page, &index) {
        (true, None, None) | (false, Some(_), _) => {}
        (true, _, _) => return Err(CliError::usage("use --all or --page, not both")),
        (false, None, Some(_)) => return Err(CliError::usage("--index needs --page N")),
        (false, None, None) => {
            return Err(CliError::usage(
                "annotations remove needs --all or --page N [--index I]",
            ));
        }
    }
    let total = cx.open_pdf(&path)?;
    let page = page
        .map(|p| cx::page_number(&p, total, "--page"))
        .transpose()?;
    let index: Option<u32> = index
        .map(|i| {
            i.parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| CliError::usage(format!("--index: \"{i}\" isn't an index from 1")))
        })
        .transpose()?;
    cx.check_out(&out, &[&path])?;
    let (id, _) = cx.open(&path)?;
    let listed = cx.engine().annotations(id);
    cx.engine().close(id);
    let listed = listed.map_err(CliError::failed)?;
    let edits: Vec<AnnotEdit> = listed
        .iter()
        .filter(|a| page.is_none_or(|p| a.page == p))
        .filter(|a| index.is_none_or(|i| a.index + 1 == i))
        .map(|a| AnnotEdit::Delete {
            page: a.page,
            index: a.index,
        })
        .collect();
    if edits.is_empty() {
        let where_ = match (page, index) {
            (Some(p), Some(i)) => format!("no annotation #{i} on page {}", p + 1),
            (Some(p), None) => format!("no annotations on page {}", p + 1),
            _ => "no annotations".to_string(),
        };
        return Err(CliError::new(
            crate::out::EXIT_FAILED,
            "nothing_to_remove",
            format!(
                "{} has {where_} (see: kitab annotations list)",
                shown(&path)
            ),
        ));
    }
    let pw = cx.password()?;
    let removed = kitab_mupdf::marks::write_marks(&path, pw.as_deref(), &[], &edits, &out)
        .map_err(CliError::engine)?;
    cx.done(
        json!({"removed": removed, "output": shown(&out)}),
        &format!(
            "Removed {} in {}",
            count(removed as u64, "annotation", "annotations"),
            shown(&out)
        ),
    );
    Ok(())
}

pub fn sign(cx: &mut Cx) -> Result<()> {
    let file = cx.arg(0, "FILE")?;
    cx.no_more_args(1)?;
    if cx.args.has("key-password") {
        return Err(CliError::usage(
            "the key's password is never taken as an argument: use --key-password-env VAR or \
             --key-password-stdin",
        ));
    }
    let path = cx.input_path(&file)?;
    let out = cx.out_path()?;
    let key_file = cx
        .args
        .value("key")
        .ok_or_else(|| CliError::usage("sign needs --key KEY.pfx"))?
        .to_string();
    let key_path = cx.input_path(&key_file)?;
    let total = cx.open_pdf(&path)?;
    let page = cx::page_number(
        cx.args
            .value("page")
            .ok_or_else(|| CliError::usage("sign needs --page N"))?,
        total,
        "--page",
    )?;
    let r = cx
        .args
        .value("rect")
        .ok_or_else(|| CliError::usage("sign needs --rect X0,Y0,X1,Y1"))?;
    let v = cx::numbers(r, 4, "--rect", "X0,Y0,X1,Y1 in points")?;
    let rect = [
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ];
    if rect[2] - rect[0] < 1.0 || rect[3] - rect[1] < 1.0 {
        return Err(CliError::usage("--rect: the box has no size"));
    }
    let reason = cx.args.value("reason").map(str::to_string);
    let location = cx.args.value("location").map(str::to_string);
    cx.check_out(&out, &[&path, &key_path])?;
    let key_password = cx
        .secret("key-password", "the key's password")?
        .ok_or_else(|| {
            CliError::usage("sign needs --key-password-env VAR or --key-password-stdin")
        })?;
    let bytes = std::fs::read(&key_path)
        .map_err(|e| CliError::unreadable(format!("{}: {e}", key_path.display())))?;
    let key = kitab_sign::sign::load_pfx(&bytes, &key_password).map_err(|e| {
        let lower = e.to_lowercase();
        if lower.contains("password") || lower.contains("mac") {
            CliError::new(
                crate::out::EXIT_PASSWORD,
                "wrong_password",
                format!("could not open {}: {e}", key_path.display()),
            )
        } else {
            CliError::failed(format!("could not open {}: {e}", key_path.display()))
        }
    })?;

    // As the app signs (adapters.rs `sign_pdf`): a visible box with who and
    // when, the next free field name, then the CMS filled in place.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let stamp = kitab_sign::format_unix(now);
    let digits: String = stamp.chars().filter(char::is_ascii_digit).collect();
    let taken = kitab_mupdf::sigs::signed_fields(&path, None)
        .map(|f| f.len())
        .unwrap_or(0);
    let mut lines = vec![format!("Digitally signed by {}", key.name()), stamp];
    if let Some(r) = &reason {
        lines.push(format!("Reason: {r}"));
    }
    let field = format!("Signature{}", taken + 1);
    let new = kitab_mupdf::sigs::NewSignature {
        field: field.clone(),
        page,
        rect,
        lines,
        name: Some(key.name()),
        reason,
        location,
        when: format!("D:{digits}00Z"),
    };
    let mut pdf = kitab_mupdf::sigs::prepare_signature(&path, &new).map_err(CliError::failed)?;
    kitab_sign::sign::fill(
        &mut pdf,
        kitab_mupdf::sigs::RANGE_MARK,
        kitab_mupdf::sigs::SIG_ROOM,
        &key,
        now,
    )
    .map_err(CliError::failed)?;
    cx::write_atomic(&out, &pdf)?;
    cx.done(
        json!({"output": shown(&out), "field": field, "signer": key.name(), "page": page + 1}),
        &format!(
            "Signed {} as {} on page {} ({})",
            shown(&out),
            key.name(),
            page + 1,
            field
        ),
    );
    Ok(())
}
