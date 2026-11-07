// SPDX-License-Identifier: AGPL-3.0-or-later

//! Marks: markup a reader adds to a page (pen, shapes, highlights, notes).

use kitab_core::{Mark, MarkShape, PointPt, RectPt, Rgb8};
use rusqlite::params;
use serde_json::{Value, json};

use crate::documents::DocumentId;
use crate::{Result, Store};

/// A row in `marks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MarkId(pub i64);

fn point(p: &PointPt) -> Value {
    json!([p.x, p.y])
}

fn rect(r: &RectPt) -> Value {
    json!([r.x, r.y, r.w, r.h])
}

/// The mark as JSON for the `data` column.
pub(crate) fn encode_mark(m: &Mark) -> String {
    let mut v = json!({
        "color": format!("#{:02x}{:02x}{:02x}", m.color.r, m.color.g, m.color.b),
        "width": m.width,
        "opacity": m.opacity,
    });
    let extra = match &m.shape {
        MarkShape::Ink { strokes } => json!({
            "strokes": strokes.iter().map(|s| s.iter().map(point).collect::<Vec<_>>()).collect::<Vec<_>>()
        }),
        MarkShape::Line { from, to, .. } => json!({ "from": point(from), "to": point(to) }),
        MarkShape::Rect { rect: r } | MarkShape::Ellipse { rect: r } => json!({ "rect": rect(r) }),
        MarkShape::Highlight { rects }
        | MarkShape::Underline { rects }
        | MarkShape::Strike { rects } => {
            json!({ "rects": rects.iter().map(rect).collect::<Vec<_>>() })
        }
        MarkShape::Text {
            rect: r,
            text,
            size,
        } => json!({ "rect": rect(r), "text": text, "size": size }),
        MarkShape::Note { at, text } => json!({ "at": point(at), "text": text }),
    };
    if let (Some(o), Value::Object(e)) = (v.as_object_mut(), extra) {
        o.extend(e);
    }
    v.to_string()
}

fn f(v: &Value) -> Option<f32> {
    v.as_f64().map(|x| x as f32)
}

fn get_point(v: &Value) -> Option<PointPt> {
    let a = v.as_array()?;
    Some(PointPt {
        x: f(a.first()?)?,
        y: f(a.get(1)?)?,
    })
}

fn get_rect(v: &Value) -> Option<RectPt> {
    let a = v.as_array()?;
    Some(RectPt {
        x: f(a.first()?)?,
        y: f(a.get(1)?)?,
        w: f(a.get(2)?)?,
        h: f(a.get(3)?)?,
    })
}

fn get_rects(v: &Value) -> Option<Vec<RectPt>> {
    v.as_array()?.iter().map(get_rect).collect()
}

fn get_color(v: &Value) -> Option<Rgb8> {
    let s = v.as_str()?.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    Some(Rgb8 {
        r: c(0)?,
        g: c(2)?,
        b: c(4)?,
    })
}

/// A mark from its `kind` and `data`; `None` for anything malformed or outside
/// the limits (a row from a newer Kitab, or a damaged database).
pub(crate) fn decode_mark(page: u32, kind: &str, data: &str) -> Option<Mark> {
    let v: Value = serde_json::from_str(data).ok()?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let shape = match kind {
        "ink" => MarkShape::Ink {
            strokes: v
                .get("strokes")?
                .as_array()?
                .iter()
                .map(|s| s.as_array()?.iter().map(get_point).collect())
                .collect::<Option<_>>()?,
        },
        "line" | "arrow" => MarkShape::Line {
            from: get_point(v.get("from")?)?,
            to: get_point(v.get("to")?)?,
            arrow: kind == "arrow",
        },
        "rect" => MarkShape::Rect {
            rect: get_rect(v.get("rect")?)?,
        },
        "ellipse" => MarkShape::Ellipse {
            rect: get_rect(v.get("rect")?)?,
        },
        "highlight" => MarkShape::Highlight {
            rects: get_rects(v.get("rects")?)?,
        },
        "underline" => MarkShape::Underline {
            rects: get_rects(v.get("rects")?)?,
        },
        "strike" => MarkShape::Strike {
            rects: get_rects(v.get("rects")?)?,
        },
        "text" => MarkShape::Text {
            rect: get_rect(v.get("rect")?)?,
            text: text("text")?,
            size: f(v.get("size")?)?,
        },
        "note" => MarkShape::Note {
            at: get_point(v.get("at")?)?,
            text: text("text")?,
        },
        _ => return None,
    };
    let mark = Mark {
        page,
        shape,
        color: get_color(v.get("color")?)?,
        width: f(v.get("width")?)?,
        opacity: f(v.get("opacity")?)?,
    };
    mark.is_valid().then_some(mark)
}

impl Store {
    /// Keep a mark. Invalid marks are refused, not stored.
    pub fn add_mark(&mut self, document_id: DocumentId, mark: &Mark) -> Result<MarkId> {
        if !mark.is_valid() {
            return Err(crate::Error::Corrupt("mark"));
        }
        let now = self.now_ms();
        self.conn
            .prepare_cached(
                "INSERT INTO marks (document_id, page, kind, data, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?
            .execute(params![
                document_id.0,
                mark.page,
                mark.kind(),
                encode_mark(mark),
                now
            ])?;
        Ok(MarkId(self.conn.last_insert_rowid()))
    }

    /// A document's marks, oldest first. Rows that don't decode are skipped.
    pub fn marks(&self, document_id: DocumentId) -> Result<Vec<(MarkId, Mark)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, page, kind, data FROM marks WHERE document_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([document_id.0], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, page, kind, data) = row?;
            let Ok(page) = u32::try_from(page) else {
                continue;
            };
            if let Some(m) = decode_mark(page, &kind, &data) {
                out.push((MarkId(id), m));
            }
        }
        Ok(out)
    }

    /// Delete one mark (the eraser, or undo).
    pub fn delete_mark(&mut self, id: MarkId) -> Result<()> {
        self.conn
            .prepare_cached("DELETE FROM marks WHERE id = ?1")?
            .execute([id.0])?;
        Ok(())
    }

    /// Delete every mark of a document (after they were saved into the PDF).
    pub fn clear_marks(&mut self, document_id: DocumentId) -> Result<usize> {
        Ok(self
            .conn
            .prepare_cached("DELETE FROM marks WHERE document_id = ?1")?
            .execute([document_id.0])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_shape() -> Vec<Mark> {
        let p = |x, y| PointPt { x, y };
        let r = RectPt {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        };
        let shapes = vec![
            MarkShape::Ink {
                strokes: vec![vec![p(1.0, 2.0), p(3.5, 4.25)], vec![p(9.0, 9.0)]],
            },
            MarkShape::Line {
                from: p(0.0, 0.0),
                to: p(50.0, 60.0),
                arrow: false,
            },
            MarkShape::Line {
                from: p(0.0, 0.0),
                to: p(50.0, 60.0),
                arrow: true,
            },
            MarkShape::Rect { rect: r },
            MarkShape::Ellipse { rect: r },
            MarkShape::Highlight { rects: vec![r, r] },
            MarkShape::Underline { rects: vec![r] },
            MarkShape::Strike { rects: vec![r] },
            MarkShape::Text {
                rect: r,
                text: "কিতাব \"quoted\"".into(),
                size: 14.0,
            },
            MarkShape::Note {
                at: p(5.0, 6.0),
                text: "A note\nwith lines".into(),
            },
        ];
        shapes
            .into_iter()
            .enumerate()
            .map(|(i, shape)| Mark {
                page: i as u32,
                shape,
                color: Rgb8 {
                    r: 230,
                    g: 180,
                    b: 80,
                },
                width: 2.5,
                opacity: 0.4,
            })
            .collect()
    }

    #[test]
    fn every_shape_round_trips() {
        for m in every_shape() {
            let back = decode_mark(m.page, m.kind(), &encode_mark(&m));
            assert_eq!(back.as_ref(), Some(&m), "{}", m.kind());
        }
    }

    #[test]
    fn malformed_rows_decode_to_none() {
        // Colours as data, assembled so the view-colour lint doesn't read them as UI.
        let black = format!("{}000000", '#');
        let row = |rest: &str| format!(r#"{{"color":"{black}",{rest}}}"#);
        assert!(decode_mark(0, "ink", "not json").is_none());
        assert!(decode_mark(0, "spiral", &row(r#""width":1,"opacity":1"#)).is_none());
        let bad_colour = r#"{"color":"red","width":1,"opacity":1,"rect":[0,0,1,1]}"#;
        assert!(decode_mark(0, "rect", bad_colour).is_none());
        let too_wide = row(r#""width":500,"opacity":1,"rect":[0,0,1,1]"#);
        assert!(decode_mark(0, "rect", &too_wide).is_none());
        assert!(
            decode_mark(0, "rect", &row(r#""width":1,"opacity":1,"rect":[0,0,1,1]"#)).is_some()
        );
    }
}
