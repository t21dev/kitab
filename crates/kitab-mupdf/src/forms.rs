// SPDX-License-Identifier: AGPL-3.0-or-later

//! Filling in a PDF's form (AcroForm): read each page's fields, and set a
//! field's value the way MuPDF's own viewer does, which also draws its new
//! appearance (the text in the field's font, the check mark in the PDF's own
//! style), so every other reader shows what Kitab shows.
//!
//! Field scripts (format, validate, calculate) are not run: Kitab runs no
//! JavaScript (AGENTS.md rule 11), so values go in as typed.

use kitab_core::{FieldEdit, FieldKind, FieldValue, FormField, RectPt};
use mupdf::pdf::{FieldFlags, PdfDocument, PdfObject, PdfPage, PdfWidget, WidgetType};

/// The most widgets read from one page (a hostile file could have millions).
const MAX_WIDGETS: usize = 2_000;
/// Pages searched for other widgets of an edited field. Past this, only the
/// page it was edited on is redrawn at once (the rest catch up on save).
const MAX_LINKED_PAGES: u32 = 400;

fn page(pdf: &PdfDocument, index: u32) -> Result<PdfPage, String> {
    let i = i32::try_from(index).map_err(|e| e.to_string())?;
    pdf.load_pdf_page(i).map_err(|e| e.to_string())
}

fn rect_of(w: &PdfWidget) -> RectPt {
    w.annotation()
        .bounds()
        .map(|r| RectPt {
            x: r.x0,
            y: r.y0,
            w: r.x1 - r.x0,
            h: r.y1 - r.y0,
        })
        .unwrap_or_default()
}

fn inheritable(obj: &PdfObject, key: &str) -> Option<PdfObject> {
    obj.get_dict_inheritable(key).ok().flatten()
}

fn name_of(obj: &PdfObject) -> Option<String> {
    obj.as_name()
        .ok()
        .map(|n| String::from_utf8_lossy(&n).into_owned())
}

/// The name of a checkbox's or radio button's "on" appearance ("Yes", "On",
/// "1", a radio's own choice…): what its value is set to when it's ticked.
fn on_state(w: &PdfWidget) -> Option<String> {
    let ap = w.annotation().object().get_dict("AP").ok()??;
    for key in ["N", "D"] {
        let Some(states) = ap.get_dict(key).ok().flatten() else {
            continue;
        };
        let n = states.dict_len().unwrap_or(0).min(16);
        for i in 0..n {
            let Ok(Some(k)) = states.get_dict_key(i as i32) else {
                continue;
            };
            if let Some(name) = name_of(&k).filter(|s| s != "Off") {
                return Some(name);
            }
        }
    }
    None
}

/// Whether a checkbox or radio button shows its "on" appearance.
fn is_on(w: &PdfWidget) -> bool {
    w.annotation()
        .object()
        .get_dict("AS")
        .ok()
        .flatten()
        .and_then(|a| name_of(&a))
        .is_some_and(|s| s != "Off")
}

/// The text size in the field's default appearance ("/Helv 9 Tf 0 g").
fn font_size(obj: &PdfObject) -> f32 {
    let Some(da) = inheritable(obj, "DA").and_then(|d| d.as_string().ok()) else {
        return 0.0;
    };
    let words: Vec<&str> = da.split_whitespace().collect();
    words
        .windows(2)
        .find(|w| w[1] == "Tf")
        .and_then(|w| w[0].parse::<f32>().ok())
        .filter(|s| s.is_finite() && (0.0..=200.0).contains(s))
        .unwrap_or(0.0)
}

/// A list's options: the value each one sets (an [export, shown] pair sets its export value).
fn options(obj: &PdfObject) -> Vec<String> {
    let Some(opt) = inheritable(obj, "Opt") else {
        return Vec::new();
    };
    let n = opt.len().unwrap_or(0).min(500);
    (0..n)
        .filter_map(|i| {
            let item = opt.get_array(i as i32).ok()??;
            let item = if item.is_array().unwrap_or(false) {
                item.get_array(0).ok()??
            } else {
                item
            };
            item.as_string().ok()
        })
        .collect()
}

fn describe(page_ix: u32, index: u32, w: &PdfWidget) -> Option<FormField> {
    let kind = w.r#type().ok()?;
    let flags = w.field_flags().unwrap_or(FieldFlags::empty());
    let obj = w.annotation().object();
    let kind = match kind {
        WidgetType::Text => FieldKind::Text {
            multiline: flags.contains(FieldFlags::MULTILINE),
            max_len: inheritable(&obj, "MaxLen")
                .and_then(|m| m.as_int().ok())
                .and_then(|m| u32::try_from(m).ok())
                .filter(|&m| m > 0),
            password: flags.contains(FieldFlags::PASSWORD),
            font_size: font_size(&obj),
        },
        WidgetType::Checkbox => FieldKind::Check,
        WidgetType::RadioButton => FieldKind::Radio,
        WidgetType::Combobox | WidgetType::Listbox => FieldKind::Choice {
            options: options(&obj),
            edit: flags.contains(FieldFlags::EDIT),
        },
        WidgetType::Signature => FieldKind::Signature,
        WidgetType::Button => FieldKind::Button,
        WidgetType::Unknown => return None,
    };
    let on = matches!(kind, FieldKind::Check | FieldKind::Radio) && is_on(w);
    Some(FormField {
        page: page_ix,
        index,
        name: w.name().ok().flatten().unwrap_or_default(),
        kind,
        rect: rect_of(w),
        value: w.value().ok().flatten().unwrap_or_default(),
        on,
        read_only: w.is_readonly().unwrap_or(false) || flags.contains(FieldFlags::READ_ONLY),
    })
}

/// The form fields on one page (0-based), in file order. Runs on a worker.
pub(crate) fn fields(pdf: &PdfDocument, page_ix: u32) -> Result<Vec<FormField>, String> {
    let p = page(pdf, page_ix)?;
    Ok(p.widgets()
        .take(MAX_WIDGETS)
        .enumerate()
        .filter_map(|(i, w)| describe(page_ix, u32::try_from(i).ok()?, &w))
        .collect())
}

/// Set a field's value in the open document and draw it anew, on its page
/// and on every other page showing the same field. Returns the pages whose
/// look changed. Nothing changes if the widget isn't there or is read-only.
pub(crate) fn apply(
    pdf: &mut PdfDocument,
    page_count: u32,
    edit: &FieldEdit,
) -> Result<Vec<u32>, String> {
    if !edit.is_valid() {
        return Err("that value is too long for a form field".into());
    }
    let mut p = page(pdf, edit.page)?;
    let mut w = p
        .widgets()
        .nth(edit.index as usize)
        .ok_or("that form field isn't in the PDF any more")?;
    if w.is_readonly().unwrap_or(false) {
        return Err("this field can't be changed".into());
    }
    let kind = w.r#type().map_err(|e| e.to_string())?;
    let button = matches!(kind, WidgetType::Checkbox | WidgetType::RadioButton);
    let value = match (&edit.value, kind) {
        (FieldValue::Checked(on), WidgetType::Checkbox | WidgetType::RadioButton) => {
            if *on {
                on_state(&w).ok_or("this box has no ticked look to show")?
            } else {
                "Off".to_string()
            }
        }
        (FieldValue::Text(t), WidgetType::Text) => t.clone(),
        (FieldValue::Choice(t), WidgetType::Combobox | WidgetType::Listbox) => t.clone(),
        _ => return Err("that value doesn't fit this kind of field".into()),
    };
    let name = w.name().ok().flatten().filter(|n| !n.is_empty());
    // Other widgets of the same field (a name repeated in every page's
    // header, a radio group) take the value too. MuPDF redraws them only on
    // pages open when it updates, so open those pages first.
    let mut linked: Vec<(u32, PdfPage)> = Vec::new();
    if let Some(name) = name.as_deref()
        && page_count <= MAX_LINKED_PAGES
    {
        for other in (0..page_count).filter(|&i| i != edit.page) {
            let Ok(op) = page(pdf, other) else {
                continue;
            };
            let shares = op
                .widgets()
                .take(MAX_WIDGETS)
                .any(|ow| ow.name().ok().flatten().as_deref() == Some(name));
            if shares {
                linked.push((other, op));
            }
        }
    }
    // No scripts run (ignore trigger events): the value goes in as given.
    w.set_value(pdf, &value, true)
        .map_err(|e| format!("could not fill in the field: {e}"))?;
    // A ticked box or radio shows the state it was set to; every other
    // button of the group shows Off (MuPDF sets the field's value, but only
    // the clicked widget's look).
    if button && let Some(name) = name.as_deref() {
        for pg in std::iter::once(&p).chain(linked.iter().map(|(_, lp)| lp)) {
            for ow in pg.widgets().take(MAX_WIDGETS) {
                if ow.name().ok().flatten().as_deref() != Some(name) {
                    continue;
                }
                let state = if value != "Off" && on_state(&ow).as_deref() == Some(value.as_str()) {
                    value.as_str()
                } else {
                    "Off"
                };
                let mut obj = ow.annotation().object();
                let name = PdfObject::new_name(state).map_err(|e| e.to_string())?;
                obj.dict_put("AS", name).map_err(|e| e.to_string())?;
            }
        }
    }
    w.update().map_err(|e| e.to_string())?;
    p.update().map_err(|e| e.to_string())?;
    let mut changed = vec![edit.page];
    for (n, mut lp) in linked {
        let _ = lp.update();
        changed.push(n);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn open(name: &str) -> PdfDocument {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pdf")
            .join(name);
        PdfDocument::open(path.to_str().expect("utf-8")).expect("opens the form fixture")
    }

    fn find(pdf: &PdfDocument, page: u32, name: &str) -> FormField {
        fields(pdf, page)
            .expect("fields")
            .into_iter()
            .find(|f| f.name == name)
            .expect(name)
    }

    fn edit(f: &FormField, value: FieldValue) -> FieldEdit {
        FieldEdit {
            page: f.page,
            index: f.index,
            value,
        }
    }

    #[test]
    fn reads_every_kind_of_field() {
        let pdf = open("03-forms.pdf");
        let f = fields(&pdf, 0).expect("fields");
        let kind = |name: &str| f.iter().find(|f| f.name == name).expect(name).clone();
        assert!(matches!(
            kind("full_name").kind,
            FieldKind::Text {
                multiline: false,
                ..
            }
        ));
        assert_eq!(kind("city").value, "Dhaka");
        assert_eq!(kind("newsletter").kind, FieldKind::Check);
        assert!(kind("newsletter").on);
        let radios: Vec<_> = f.iter().filter(|f| f.name == "card_size").collect();
        assert_eq!(radios.len(), 2);
        assert!(radios.iter().all(|r| r.kind == FieldKind::Radio));
        assert_eq!(
            radios.iter().map(|r| r.on).collect::<Vec<_>>(),
            [true, false]
        );
        assert!(matches!(&kind("branch").kind,
            FieldKind::Choice { options, .. } if options == &["Central", "North", "South"]));
        assert!(
            matches!(&kind("interests").kind, FieldKind::Choice { options, .. } if options.len() == 4)
        );
        assert!(
            f.iter()
                .all(|f| f.rect.w > 0.0 && f.rect.h > 0.0 && f.fillable())
        );

        let two = open("11-form-two-pages.pdf");
        assert!(
            matches!(find(&two, 0, "notes").kind, FieldKind::Text { multiline: true, font_size, .. } if font_size == 10.0)
        );
        let locked = find(&two, 0, "locked");
        assert!(locked.read_only && !locked.fillable());
        assert_eq!(locked.value, "Fixed");
    }

    #[test]
    fn text_checkboxes_radios_and_lists_take_values() {
        let mut pdf = open("03-forms.pdf");
        let name = find(&pdf, 0, "full_name");
        apply(
            &mut pdf,
            1,
            &edit(&name, FieldValue::Text("Ada Lovelace".into())),
        )
        .expect("text");
        assert_eq!(find(&pdf, 0, "full_name").value, "Ada Lovelace");

        let box_ = find(&pdf, 0, "newsletter");
        apply(&mut pdf, 1, &edit(&box_, FieldValue::Checked(false))).expect("untick");
        assert!(!find(&pdf, 0, "newsletter").on);
        apply(&mut pdf, 1, &edit(&box_, FieldValue::Checked(true))).expect("tick");
        assert!(find(&pdf, 0, "newsletter").on);

        // Radios: choosing one turns the other in its group off.
        let radios = |pdf: &PdfDocument| -> Vec<FormField> {
            fields(pdf, 0)
                .expect("fields")
                .into_iter()
                .filter(|f| f.name == "card_size")
                .collect()
        };
        let large = radios(&pdf)[1].clone();
        apply(&mut pdf, 1, &edit(&large, FieldValue::Checked(true))).expect("radio");
        assert_eq!(
            radios(&pdf).iter().map(|r| r.on).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(radios(&pdf)[0].value, "large");

        let branch = find(&pdf, 0, "branch");
        apply(
            &mut pdf,
            1,
            &edit(&branch, FieldValue::Choice("North".into())),
        )
        .expect("choice");
        assert_eq!(find(&pdf, 0, "branch").value, "North");
    }

    /// Page `n` as drawn, annotations and fields included.
    fn pixels(pdf: &PdfDocument, n: i32) -> Vec<u8> {
        pdf.load_page(n)
            .expect("page")
            .to_pixmap(
                &mupdf::Matrix::IDENTITY,
                &mupdf::Colorspace::device_rgb(),
                false,
                true,
            )
            .expect("draws")
            .samples()
            .to_vec()
    }

    #[test]
    fn a_field_shown_on_two_pages_changes_on_both() {
        let mut pdf = open("11-form-two-pages.pdf");
        let before = pixels(&pdf, 1);
        let a = find(&pdf, 0, "address");
        let changed =
            apply(&mut pdf, 2, &edit(&a, FieldValue::Text("1 Main St".into()))).expect("fills");
        assert_eq!(changed, vec![0, 1]);
        assert_eq!(find(&pdf, 1, "address").value, "1 Main St");
        // The second page draws the new value, not only stores it.
        assert_ne!(pixels(&pdf, 1), before, "page two shows the address");
    }

    #[test]
    fn signature_fields_are_listed_but_not_filled_in() {
        let mut pdf = open("08a-signed-valid.pdf");
        let f = fields(&pdf, 0).expect("fields");
        assert_eq!(f.len(), 2, "Landlord and Tenant");
        assert!(
            f.iter()
                .all(|f| f.kind == FieldKind::Signature && !f.fillable())
        );
        for v in [FieldValue::Text("forged".into()), FieldValue::Checked(true)] {
            assert!(apply(&mut pdf, 1, &edit(&f[0], v)).is_err());
        }
    }

    #[test]
    fn wrong_values_and_locked_fields_change_nothing() {
        let mut pdf = open("11-form-two-pages.pdf");
        let a = find(&pdf, 0, "address");
        let locked = find(&pdf, 0, "locked");
        let bad = [
            edit(&a, FieldValue::Checked(true)),
            edit(&locked, FieldValue::Text("x".into())),
            FieldEdit {
                page: 0,
                index: 9_999,
                value: FieldValue::Text("x".into()),
            },
            edit(&a, FieldValue::Text("a\0b".into())),
        ];
        for e in &bad {
            assert!(apply(&mut pdf, 2, e).is_err(), "{e:?}");
        }
        assert_eq!(find(&pdf, 0, "address").value, "");
        assert_eq!(find(&pdf, 0, "locked").value, "Fixed");
    }
}
