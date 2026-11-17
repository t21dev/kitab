// SPDX-License-Identifier: AGPL-3.0-or-later

//! A document's unsaved work for crash recovery (`crate::recovery`): what
//! it is now, and putting it back after a crash.

use super::Reader;
use super::editor::Editor;
use crate::recovery::{Field, Unsaved};
use gpui::Context;
use kitab_core::FieldEdit;

impl Reader {
    /// What would be lost if Kitab stopped now: form fields filled in, the
    /// PDF's annotations taken out, Markdown being edited. `None` if nothing.
    pub fn unsaved_work(&self) -> Option<Unsaved> {
        let markdown = self
            .md
            .as_ref()
            .and_then(|m| m.edit.as_ref())
            .filter(|e| e.buf.dirty)
            .map(|e| e.buf.text().to_string());
        let fields = kitab_core::latest_edits(&self.form.edits)
            .iter()
            .map(Field::from)
            .collect();
        let work = Unsaved {
            path: self.path.clone(),
            markdown,
            fields,
            deleted: self.markup.deleted.clone(),
        };
        (!work.is_empty()).then_some(work)
    }

    /// Put `work` back once this document is open (now, if it is).
    pub fn restore_work(&mut self, work: Unsaved, cx: &mut Context<Self>) {
        self.pending_recovery = Some(work);
        if self.status == super::Status::Open {
            self.take_recovery(cx);
        }
    }

    /// Apply the work waiting for this document to open.
    pub(super) fn take_recovery(&mut self, cx: &mut Context<Self>) {
        let Some(work) = self.pending_recovery.take() else {
            return;
        };
        for f in &work.fields {
            let e = FieldEdit::from(f);
            self.write_field(e.page, e.index, e.value, cx);
        }
        for d in work.deleted {
            if !self.markup.deleted.contains(&d) {
                self.markup.deleted.push(d);
            }
        }
        if let Some(text) = work.markdown
            && self.md.is_some()
        {
            let mut editor = Editor::new(text, cx);
            editor.buf.dirty = true;
            if let Some(md) = self.md.as_mut() {
                md.edit = Some(editor);
            }
            self.edited(cx);
        }
        cx.notify();
    }
}
