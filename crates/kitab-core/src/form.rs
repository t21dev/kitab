// SPDX-License-Identifier: AGPL-3.0-or-later

//! A PDF's form fields (AcroForm), as the reader fills them in: text boxes,
//! checkboxes, radio buttons and lists. The engine reads them page by page;
//! the reader sends back [`FieldEdit`]s, which change the open document at
//! once (so the page draws the new value) and are written into the file on
//! Save.

use crate::RectPt;

/// The longest value Kitab writes into one field.
pub const MAX_FIELD_CHARS: usize = 20_000;

/// What kind of field a widget is.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Text {
        multiline: bool,
        /// Characters the field takes, if it says.
        max_len: Option<u32>,
        /// Shown as dots.
        password: bool,
        /// The field's own text size in points; 0 means "fit the box".
        font_size: f32,
    },
    Check,
    Radio,
    /// A drop-down (combo) or list box; `edit` takes typed values too.
    Choice {
        options: Vec<String>,
        edit: bool,
    },
    /// Signed with Sign…, not typed into.
    Signature,
    /// A push button: it runs scripts, which Kitab doesn't.
    Button,
}

/// One widget of a field, on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub page: u32,
    /// Its place among the page's widgets, in file order: how an edit finds
    /// it again in the open document and in the file when saving.
    pub index: u32,
    /// The fully qualified field name ("Buyer.Name"). Widgets sharing a
    /// name are one field: filling one fills them all.
    pub name: String,
    pub kind: FieldKind,
    /// In page points from the top-left, like links and annotations.
    pub rect: RectPt,
    /// The text, or a list's choice. Empty when unset.
    pub value: String,
    /// A checkbox or radio button that is on.
    pub on: bool,
    pub read_only: bool,
}

impl FormField {
    /// Whether a click or typing can change it here.
    pub fn fillable(&self) -> bool {
        !self.read_only
            && matches!(
                self.kind,
                FieldKind::Text { .. }
                    | FieldKind::Check
                    | FieldKind::Radio
                    | FieldKind::Choice { .. }
            )
    }
}

/// A new value for a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    /// A checkbox or radio button on or off.
    Checked(bool),
    /// One of a list's options (or typed, if it allows that).
    Choice(String),
}

/// A field filled in, by its widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEdit {
    pub page: u32,
    pub index: u32,
    pub value: FieldValue,
}

impl FieldEdit {
    /// Values Kitab will write: not too long, no NULs (MuPDF takes C strings).
    pub fn is_valid(&self) -> bool {
        match &self.value {
            FieldValue::Text(t) | FieldValue::Choice(t) => {
                t.chars().count() <= MAX_FIELD_CHARS && !t.contains('\0')
            }
            FieldValue::Checked(_) => true,
        }
    }
}

/// The edits to write, the newest per widget, in the order they were made.
pub fn latest_edits(edits: &[FieldEdit]) -> Vec<FieldEdit> {
    let mut out: Vec<FieldEdit> = Vec::new();
    for e in edits {
        out.retain(|o| (o.page, o.index) != (e.page, e.index));
        out.push(e.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(page: u32, index: u32, v: &str) -> FieldEdit {
        FieldEdit {
            page,
            index,
            value: FieldValue::Text(v.into()),
        }
    }

    #[test]
    fn only_the_last_value_of_a_widget_is_kept() {
        let edits = [edit(0, 1, "a"), edit(0, 2, "b"), edit(0, 1, "c")];
        assert_eq!(latest_edits(&edits), vec![edit(0, 2, "b"), edit(0, 1, "c")]);
    }

    #[test]
    fn values_are_checked() {
        assert!(edit(0, 0, "Main St").is_valid());
        assert!(!edit(0, 0, "a\0b").is_valid());
        assert!(!edit(0, 0, &"x".repeat(MAX_FIELD_CHARS + 1)).is_valid());
    }
}
