// SPDX-License-Identifier: AGPL-3.0-or-later

//! The block tree as a self-contained HTML page, for printing and Save as
//! PDF: MuPDF lays it out on paper like an EPUB chapter. Every piece of text
//! is escaped. Links are left out (paper can't follow them). Pictures and
//! Mermaid diagrams come from [`PrintAssets`], as files beside the page: the
//! app draws the diagrams and copies the local pictures there; without one
//! (or for a remote picture, which stays blocked) the alt text stands in.
//! Math is written out as a line of text (`math::to_line`).

use crate::model::{Align, Block, Inline, Inlines, MdDoc};

/// Options for the printed page.
#[derive(Debug, Clone)]
pub struct PrintStyle<'a> {
    /// CSS font-family list for the body text.
    pub body_font: &'a str,
    /// CSS font-family list for code.
    pub mono_font: &'a str,
}

/// A picture for the page: its file name, beside the HTML, and the width
/// to show it at in points (`None`: its own size, 96 pixels an inch; MuPDF
/// shrinks anything wider than the page to fit).
pub type PrintPicture = (String, Option<f32>);

/// What the page shows pictures and diagrams with.
pub trait PrintAssets {
    /// A Mermaid diagram drawn as a picture, or `None` to print its source.
    fn diagram(&self, _src: &str) -> Option<PrintPicture> {
        None
    }
    /// The picture at `src` (as written in the Markdown), `width` pixels wide
    /// when the Markdown says so; `None` prints its alt text.
    fn image(&self, _src: &str, _width: Option<f32>) -> Option<PrintPicture> {
        None
    }
}

/// No pictures: alt text and diagram sources.
pub struct NoAssets;
impl PrintAssets for NoAssets {}

/// The document as an HTML page sized for A4 or Letter (MuPDF's default page).
pub fn to_print_html(doc: &MdDoc, style: &PrintStyle) -> String {
    to_print_html_with(doc, style, &NoAssets)
}

/// [`to_print_html`] with pictures and diagrams from `assets`.
pub fn to_print_html_with(doc: &MdDoc, style: &PrintStyle, assets: &dyn PrintAssets) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("<!DOCTYPE html><html><head><meta charset=\"utf-8\">");
    if let Some(t) = &doc.title {
        out.push_str("<title>");
        esc(t, &mut out);
        out.push_str("</title>");
    }
    out.push_str("<style>");
    out.push_str(&format!(
        "@page{{margin:18mm 18mm}}\
         body{{font-family:{body};font-size:10.5pt;line-height:1.5;color:#111}}\
         h1,h2,h3,h4,h5,h6{{line-height:1.25;margin:1.1em 0 .45em;page-break-after:avoid}}\
         h1{{font-size:20pt;border-bottom:1px solid #ccc;padding-bottom:.2em}}\
         h2{{font-size:15pt;border-bottom:1px solid #ddd;padding-bottom:.15em}}\
         h3{{font-size:12.5pt}}h4,h5,h6{{font-size:11pt}}\
         p{{margin:.5em 0}}li p{{margin:0}}\
         ul{{list-style-type:none}}\
         ul,ol{{margin:.4em 0;padding-left:1.4em}}\
         del{{text-decoration:line-through}}\
         code,kbd,pre{{font-family:{mono};font-size:9pt}}\
         code,kbd{{background:#f2f2f2;padding:0 .2em}}\
         pre{{background:#f6f6f6;border:1px solid #ddd;padding:.6em .8em;white-space:pre-wrap}}\
         blockquote{{margin:.6em 0;padding:0 0 0 .9em;border-left:3px solid #ccc;color:#444}}\
         table{{border-collapse:separate;border-spacing:0;margin:.6em 0}}\
         th,td{{border-bottom:1px solid #ccc;padding:.3em .6em}}\
         th{{background:#f0f0f0;border-top:1px solid #bbb;border-bottom:1px solid #999}}\
         .fig{{text-align:center;margin:.8em 0}}\
         .math{{text-align:center;font-style:italic;margin:.6em 0}}\
         hr{{border:0;border-top:1px solid #ccc;margin:1em 0}}\
         .fm{{color:#555;font-size:9pt;margin-bottom:1em}}\
         .fn{{font-size:9pt;color:#333}}\
         .alt{{color:#666;font-style:italic}}",
        body = style.body_font,
        mono = style.mono_font,
    ));
    out.push_str("</style></head><body>");
    for b in &doc.blocks {
        block(b, &mut out, assets);
    }
    out.push_str("</body></html>");
    out
}

fn esc(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// An `<img>` for a picture from [`PrintAssets`].
fn picture((file, width): &PrintPicture, out: &mut String) {
    out.push_str("<img src=\"");
    esc(file, out);
    out.push('"');
    if let Some(w) = width {
        out.push_str(&format!(" style=\"width:{w:.1}pt\""));
    }
    out.push('>');
}

/// Math written out as a line (x², √2, …); its source when it can't be read.
fn math_line(src: &str) -> String {
    crate::math::parse(src)
        .map(|n| crate::math::to_line(&n))
        .unwrap_or_else(|_| src.to_string())
}

fn inlines(list: &Inlines, out: &mut String, assets: &dyn PrintAssets) {
    for i in list {
        match i {
            Inline::Text { text, style, .. } if style.math => {
                out.push_str("<i>");
                esc(&math_line(text), out);
                out.push_str("</i>");
            }
            Inline::Text { text, style, .. } => {
                let mut close = Vec::new();
                let mut open = |tag: &'static str, out: &mut String| {
                    out.push('<');
                    out.push_str(tag);
                    out.push('>');
                    close.push(tag);
                };
                if style.bold {
                    open("b", out);
                }
                if style.italic {
                    open("i", out);
                }
                if style.strike {
                    open("del", out);
                }
                if style.code || style.math {
                    open("code", out);
                }
                if style.kbd {
                    open("kbd", out);
                }
                if style.sup {
                    open("sup", out);
                }
                if style.sub {
                    open("sub", out);
                }
                esc(text, out);
                for tag in close.iter().rev() {
                    out.push_str("</");
                    out.push_str(tag);
                    out.push('>');
                }
            }
            Inline::Image {
                src, alt, width, ..
            } => {
                if let Some(p) = assets.image(src, *width) {
                    picture(&p, out);
                } else if !alt.is_empty() {
                    out.push_str("<span class=\"alt\">[");
                    esc(alt, out);
                    out.push_str("]</span>");
                }
            }
            Inline::Break => out.push_str("<br>"),
            Inline::FootnoteRef { number, .. } => {
                out.push_str(&format!("<sup>{number}</sup>"));
            }
        }
    }
}

fn blocks(list: &[Block], out: &mut String, assets: &dyn PrintAssets) {
    for b in list {
        block(b, out, assets);
    }
}

fn block(b: &Block, out: &mut String, assets: &dyn PrintAssets) {
    match b {
        Block::Heading { level, content, .. } => {
            let l = (*level).clamp(1, 6);
            out.push_str(&format!("<h{l}>"));
            inlines(content, out, assets);
            out.push_str(&format!("</h{l}>"));
        }
        Block::Paragraph(content) => {
            out.push_str("<p>");
            inlines(content, out, assets);
            out.push_str("</p>");
        }
        Block::List { start, items } => {
            match start {
                Some(n) => out.push_str(&format!("<ol start=\"{n}\">")),
                None => out.push_str("<ul>"),
            }
            for item in items {
                // Bullets and task boxes as text: MuPDF draws no marker for a
                // nested bullet list, and its fonts have no ☐/☑.
                let mark = match (item.task, start) {
                    (Some(true), _) => "[x]\u{a0}",
                    (Some(false), _) => "[\u{a0}\u{a0}]\u{a0}",
                    (None, None) => "\u{2022}\u{a0}\u{a0}",
                    (None, Some(_)) => "",
                };
                out.push_str("<li>");
                match item.blocks.as_slice() {
                    // A tight item's single paragraph prints without a gap.
                    [Block::Paragraph(content)] => {
                        out.push_str(mark);
                        inlines(content, out, assets);
                    }
                    [Block::Paragraph(content), rest @ ..] => {
                        out.push_str("<p>");
                        out.push_str(mark);
                        inlines(content, out, assets);
                        out.push_str("</p>");
                        blocks(rest, out, assets);
                    }
                    many => {
                        out.push_str(mark);
                        blocks(many, out, assets);
                    }
                }
                out.push_str("</li>");
            }
            out.push_str(if start.is_some() { "</ol>" } else { "</ul>" });
        }
        Block::Quote(body) => {
            out.push_str("<blockquote>");
            blocks(body, out, assets);
            out.push_str("</blockquote>");
        }
        Block::Mermaid(text) if let Some(p) = assets.diagram(text) => {
            out.push_str("<p class=\"fig\">");
            picture(&p, out);
            out.push_str("</p>");
        }
        Block::Math(text) if crate::math::parse(text).is_ok() => {
            out.push_str("<p class=\"math\">");
            esc(&math_line(text), out);
            out.push_str("</p>");
        }
        Block::Code { text, .. } | Block::Mermaid(text) | Block::Math(text) => {
            out.push_str("<pre>");
            esc(text.trim_end_matches('\n'), out);
            out.push_str("</pre>");
        }
        Block::Table { aligns, head, rows } => {
            let align = |i: usize| match aligns.get(i) {
                Some(Align::Left) => " style=\"text-align:left\"",
                Some(Align::Center) => " style=\"text-align:center\"",
                Some(Align::Right) => " style=\"text-align:right\"",
                _ => "",
            };
            out.push_str("<table><thead><tr>");
            for (i, cell) in head.iter().enumerate() {
                out.push_str(&format!("<th{}>", align(i)));
                inlines(cell, out, assets);
                out.push_str("</th>");
            }
            out.push_str("</tr></thead><tbody>");
            for row in rows {
                out.push_str("<tr>");
                for (i, cell) in row.iter().enumerate() {
                    out.push_str(&format!("<td{}>", align(i)));
                    inlines(cell, out, assets);
                    out.push_str("</td>");
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table>");
        }
        Block::Rule => out.push_str("<hr>"),
        Block::Details { summary, body } => {
            out.push_str("<p><b>");
            inlines(summary, out, assets);
            out.push_str("</b></p>");
            blocks(body, out, assets);
        }
        Block::Footnote { number, body, .. } => {
            out.push_str(&format!("<div class=\"fn\"><sup>{number}</sup> "));
            blocks(body, out, assets);
            out.push_str("</div>");
        }
        Block::Frontmatter(pairs) => {
            out.push_str("<div class=\"fm\">");
            for (k, v) in pairs {
                esc(k, out);
                out.push_str(": ");
                esc(v, out);
                out.push_str("<br>");
            }
            out.push_str("</div>");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(md: &str) -> String {
        let doc = crate::parse(md);
        to_print_html(
            &doc,
            &PrintStyle {
                body_font: "serif",
                mono_font: "monospace",
            },
        )
    }

    #[test]
    fn text_is_escaped_and_structure_kept() {
        let h =
            html("# A < & >\n\nSome **bold** `x<y`\n\n- one\n- [x] two\n\n```\nfn a() {}\n```\n");
        assert!(h.contains("<h1>A &lt; &amp; &gt;</h1>"), "{h}");
        assert!(h.contains("<b>bold</b>"));
        assert!(h.contains("<code>x&lt;y</code>"));
        assert!(h.contains("<li>\u{2022}\u{a0}\u{a0}one</li>"), "{h}");
        assert!(h.contains("[x]\u{a0}two"), "{h}");
        assert!(h.contains("<pre>fn a() {}</pre>"));
        assert!(!h.contains("<script"));
    }

    #[test]
    fn tables_and_raw_html_stay_safe() {
        let h = html("| a | b |\n|:-|-:|\n| 1 | <img src=x onerror=alert(1)> |\n");
        assert!(h.contains("<table>"));
        assert!(h.contains("text-align:right"));
        assert!(!h.contains("onerror=alert"), "{h}");
    }

    struct Files;
    impl PrintAssets for Files {
        fn diagram(&self, _: &str) -> Option<PrintPicture> {
            Some(("d0.png".into(), Some(150.0)))
        }
        fn image(&self, src: &str, width: Option<f32>) -> Option<PrintPicture> {
            (src == "here.png").then(|| ("i0.png".into(), width.map(|w| w * 0.75)))
        }
    }

    #[test]
    fn diagrams_pictures_and_math_print_as_themselves() {
        let md = "```mermaid\ngraph TD; A-->B\n```\n\n![a](here.png) ![b](https://x/y.png)\n\n$$x^2$$\n\nInline $\\sqrt{2}$.\n";
        let doc = crate::parse(md);
        let style = PrintStyle {
            body_font: "serif",
            mono_font: "monospace",
        };
        let h = to_print_html_with(&doc, &style, &Files);
        assert!(
            h.contains("<p class=\"fig\"><img src=\"d0.png\" style=\"width:150.0pt\"></p>"),
            "{h}"
        );
        assert!(h.contains("<img src=\"i0.png\">"), "{h}");
        // A picture the assets don't give stays its alt text.
        assert!(h.contains("[b]"), "{h}");
        assert!(
            h.contains("<p class=\"math\">") && !h.contains("x^2"),
            "{h}"
        );
        assert!(!h.contains("\\sqrt"), "{h}");
        // Without assets: the diagram's source, as before.
        let plain = to_print_html(&doc, &style);
        assert!(plain.contains("<pre>graph TD; A--&gt;B</pre>"), "{plain}");
    }
}
