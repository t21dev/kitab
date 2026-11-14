// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mermaid diagrams in Markdown (PLAN.md D5, §3.4): the fence's source is laid
//! out to SVG by `mermaid-svg` (pure Rust, no JS, no web view) in colours from
//! the theme's tokens, then drawn by `resvg` (the copy GPUI already carries)
//! into an image, all off the UI thread. Results are kept per source, theme
//! and zoom; a diagram that can't be read shows its source and the error.

use super::*;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Diagrams kept per document before the oldest set is dropped.
const MAX_KEPT: usize = 48;
/// Device pixels per diagram pixel at 100 % zoom (sharp on HiDPI screens).
const DENSITY: f32 = 2.0;
/// No side of a drawn diagram is longer than this, in device pixels.
const MAX_SIDE: f32 = 4096.0;
/// Sources longer than this are shown as source.
const MAX_SOURCE: usize = 64 * 1024;

pub(super) enum Diagram {
    Pending,
    Ready {
        image: Arc<RenderImage>,
        /// Logical size at 100 % zoom.
        w: f32,
        h: f32,
    },
    Failed(String),
}

#[derive(Default)]
pub(super) struct Diagrams {
    pub map: HashMap<u64, Diagram>,
}

/// The colours a diagram is drawn in, from the theme's tokens.
fn theme_for(st: &Style) -> mermaid_svg::Theme {
    let c = &st.theme.colors;
    let hex = |v: kitab_theme::Rgba| std::borrow::Cow::Owned(v.to_string());
    let dark = st.theme.appearance != kitab_theme::Appearance::Light;
    let base = if dark {
        mermaid_svg::Theme::dark()
    } else {
        mermaid_svg::Theme::neutral()
    };
    mermaid_svg::Theme {
        fg: hex(c.text),
        fg_muted: hex(c.muted),
        bg: hex(c.bg),
        actor_fill: hex(c.raised),
        actor_stroke: hex(c.accent),
        lifeline: hex(c.muted),
        arrow_stroke: hex(c.muted),
        note_fill: hex(c.highlight),
        note_stroke: hex(c.warn),
        activation_fill: hex(c.hover),
        activation_stroke: hex(c.accent),
        frame_label_fill: hex(c.hover),
        flow_node_fill: hex(c.raised),
        flow_node_stroke: hex(c.accent),
        flow_edge_stroke: hex(c.muted),
        flow_label_bg: hex(c.bg),
        flow_cluster_fill: hex(c.panel),
        flow_cluster_stroke: hex(c.line),
        font_family: "Segoe UI, Helvetica, Arial, sans-serif".into(),
        responsive: false,
        ..base
    }
}

/// The key a diagram is kept under: its source, the colours and the zoom.
pub(super) fn key(src: &str, st: &Style, scale: f32) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    st.theme.id.hash(&mut h);
    st.theme.colors.accent.to_string().hash(&mut h);
    ((scale * 100.0).round() as i64).hash(&mut h);
    h.finish()
}

/// Every system font, loaded once, the first time a diagram is drawn (off
/// the UI thread), so diagram text uses the same faces as the app.
fn fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: std::sync::LazyLock<Arc<resvg::usvg::fontdb::Database>> =
        std::sync::LazyLock::new(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        });
    DB.clone()
}

/// Lay out and draw `src`: BGRA pixels, their size, and the logical size at
/// 100 % zoom.
pub fn draw(
    src: &str,
    theme: &mermaid_svg::Theme,
    scale: f32,
) -> Result<(Vec<u8>, u32, u32, f32, f32), String> {
    if src.len() > MAX_SOURCE {
        return Err("this diagram is too long to draw".into());
    }
    let svg = mermaid_svg::render_with(src, theme).map_err(|e| format!("{e}"))?;
    let options = resvg::usvg::Options {
        fontdb: fonts(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (lw, lh) = (size.width(), size.height());
    let mut s = scale * DENSITY;
    let longest = lw.max(lh) * s;
    if longest > MAX_SIDE {
        s *= MAX_SIDE / longest;
    }
    let (w, h) = (
        (lw * s).ceil().max(1.0) as u32,
        (lh * s).ceil().max(1.0) as u32,
    );
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(w, h).ok_or("the diagram has no size".to_string())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(s, s),
        &mut pixmap.as_mut(),
    );
    // tiny-skia is premultiplied RGBA; GPUI images are BGRA.
    let mut px = pixmap.take();
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    Ok((px, w, h, lw, lh))
}

impl Reader {
    /// The diagram for `src` at the current look and zoom, asked for if new.
    pub(super) fn diagram(&mut self, src: &str, st: &Style, cx: &mut Context<Self>) {
        let scale = self.md_scale();
        let k = key(src, st, scale);
        let Some(md) = self.md.as_mut() else { return };
        if md.diagrams.map.contains_key(&k) {
            return;
        }
        if md.diagrams.map.len() >= MAX_KEPT {
            md.diagrams.map.clear();
        }
        md.diagrams.map.insert(k, Diagram::Pending);
        let theme = theme_for(st);
        let src = src.to_string();
        cx.spawn(async move |this, cx| {
            let drawn = cx
                .background_spawn(async move { draw(&src, &theme, scale) })
                .await;
            let _ = this.update(cx, |r, cx| {
                let Some(md) = r.md.as_mut() else { return };
                let entry = match drawn {
                    Ok((px, w, h, lw, lh)) => match RgbaImage::from_raw(w, h, px) {
                        Some(buffer) => Diagram::Ready {
                            image: Arc::new(RenderImage::new([Frame::new(buffer)])),
                            w: lw,
                            h: lh,
                        },
                        None => Diagram::Failed("the diagram couldn't be drawn".into()),
                    },
                    Err(e) => Diagram::Failed(e),
                };
                md.diagrams.map.insert(k, entry);
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_blocks() -> Vec<String> {
        let md = include_str!("../../../../tests/fixtures/md/02-mermaid.md");
        let doc = kitab_md::parse(md);
        doc.blocks
            .iter()
            .filter_map(|b| match b {
                kitab_md::Block::Mermaid(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_fixture_diagram_draws_and_the_broken_one_says_why() {
        let theme = mermaid_svg::Theme::neutral();
        let blocks = fixture_blocks();
        assert_eq!(blocks.len(), 11, "MANIFEST: ten diagrams and one broken");
        let (good, bad) = blocks.split_at(10);
        for src in good {
            let (px, w, h, lw, lh) =
                draw(src, &theme, 1.0).unwrap_or_else(|e| panic!("{e}\n{src}"));
            assert_eq!(px.len(), (w * h * 4) as usize);
            assert!(lw > 10.0 && lh > 10.0);
            // Something was drawn: not every pixel is transparent.
            assert!(px.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        }
        assert!(draw(&bad[0], &theme, 1.0).is_err());
    }

    #[test]
    fn huge_diagrams_are_capped() {
        let theme = mermaid_svg::Theme::neutral();
        let src = fixture_blocks().remove(0);
        let (_, w, h, ..) = draw(&src, &theme, 40.0).expect("draws");
        assert!(w as f32 <= MAX_SIDE + 1.0 && h as f32 <= MAX_SIDE + 1.0);
        assert!(draw(&"graph LR\n".repeat(10_000), &theme, 1.0).is_err());
    }
}
