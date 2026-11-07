// SPDX-License-Identifier: AGPL-3.0-or-later

//! Theme files (`assets/themes/<id>.json`) and the built-in themes.

use crate::color::Rgba;
use serde::Deserialize;
use std::fmt;

/// Ids of the built-in themes, in the order Settings shows them.
pub const BUILTIN_IDS: [&str; 8] = [
    "dark", "gray", "light", "paper", "nord", "noctua", "vampire", "ink",
];

const BUILTIN_JSON: [(&str, &str); 8] = [
    ("dark", include_str!("../../../assets/themes/dark.json")),
    ("gray", include_str!("../../../assets/themes/gray.json")),
    ("light", include_str!("../../../assets/themes/light.json")),
    ("paper", include_str!("../../../assets/themes/paper.json")),
    ("nord", include_str!("../../../assets/themes/nord.json")),
    ("noctua", include_str!("../../../assets/themes/noctua.json")),
    (
        "vampire",
        include_str!("../../../assets/themes/vampire.json"),
    ),
    ("ink", include_str!("../../../assets/themes/ink.json")),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Dark,
    Light,
}

/// Every colour token in DESIGN.md § Colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Colors {
    pub bg: Rgba,
    pub panel: Rgba,
    pub desk: Rgba,
    pub raised: Rgba,
    pub hover: Rgba,
    pub line: Rgba,
    pub text: Rgba,
    pub read: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub ring: Rgba,
    pub accent: Rgba,
    pub highlight: Rgba,
    pub ok: Rgba,
    pub warn: Rgba,
    pub bad: Rgba,
    pub scrim: Rgba,
}

impl Colors {
    /// Look a token up by its DESIGN.md name.
    pub fn get(&self, token: &str) -> Option<Rgba> {
        Some(match token {
            "bg" => self.bg,
            "panel" => self.panel,
            "desk" => self.desk,
            "raised" => self.raised,
            "hover" => self.hover,
            "line" => self.line,
            "text" => self.text,
            "read" => self.read,
            "muted" => self.muted,
            "faint" => self.faint,
            "ring" => self.ring,
            "accent" => self.accent,
            "highlight" => self.highlight,
            "ok" => self.ok,
            "warn" => self.warn,
            "bad" => self.bad,
            "scrim" => self.scrim,
            _ => return None,
        })
    }
}

/// One shadow layer, with CSS `box-shadow` semantics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub inset: bool,
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
}

impl Shadow {
    /// The `0 0 0 1px` ring layer, the only layer Ultra mode keeps.
    pub fn is_ring(&self) -> bool {
        !self.inset && self.x == 0.0 && self.y == 0.0 && self.blur == 0.0 && self.spread > 0.0
    }
}

/// Depth tokens (DESIGN.md § Depth tokens). Each is a stack of layers.
#[derive(Debug, Clone, PartialEq)]
pub struct Depth {
    pub elev: Vec<Shadow>,
    pub well: Vec<Shadow>,
    pub float: Vec<Shadow>,
    pub modal: Vec<Shadow>,
    pub sheet: Vec<Shadow>,
}

impl Depth {
    /// Ultra mode: every stack reduced to its 1px ring layer.
    pub fn ultra(&self) -> Depth {
        let ring = |v: &[Shadow]| v.iter().copied().filter(Shadow::is_ring).collect();
        Depth {
            elev: ring(&self.elev),
            well: ring(&self.well),
            float: ring(&self.float),
            modal: ring(&self.modal),
            sheet: ring(&self.sheet),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub appearance: Appearance,
    pub colors: Colors,
    pub depth: Depth,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThemeError {
    Json(String),
    Color { token: String, value: String },
    UnknownBuiltin(String),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::Json(e) => write!(f, "theme file is not valid: {e}"),
            ThemeError::Color { token, value } => {
                write!(f, "token `{token}` has an invalid colour `{value}`")
            }
            ThemeError::UnknownBuiltin(id) => write!(f, "no built-in theme `{id}`"),
        }
    }
}

impl std::error::Error for ThemeError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    id: String,
    name: String,
    appearance: Appearance,
    colors: RawColors,
    depth: RawDepth,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawColors {
    bg: String,
    panel: String,
    desk: String,
    raised: String,
    hover: String,
    line: String,
    text: String,
    read: String,
    muted: String,
    faint: String,
    ring: String,
    accent: String,
    highlight: String,
    ok: String,
    warn: String,
    bad: String,
    scrim: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawShadow {
    inset: bool,
    x: f32,
    y: f32,
    blur: f32,
    spread: f32,
    color: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDepth {
    elev: Vec<RawShadow>,
    well: Vec<RawShadow>,
    float: Vec<RawShadow>,
    modal: Vec<RawShadow>,
    sheet: Vec<RawShadow>,
}

fn color(token: &str, value: &str) -> Result<Rgba, ThemeError> {
    Rgba::parse(value).ok_or_else(|| ThemeError::Color {
        token: token.to_string(),
        value: value.to_string(),
    })
}

fn shadows(token: &str, raw: Vec<RawShadow>) -> Result<Vec<Shadow>, ThemeError> {
    raw.into_iter()
        .map(|s| {
            Ok(Shadow {
                inset: s.inset,
                x: s.x,
                y: s.y,
                blur: s.blur,
                spread: s.spread,
                color: color(token, &s.color)?,
            })
        })
        .collect()
}

impl Theme {
    /// Parse a theme file. User themes go through the same path, so bad input
    /// is an error, never a panic.
    pub fn from_json(json: &str) -> Result<Theme, ThemeError> {
        let raw: RawTheme =
            serde_json::from_str(json).map_err(|e| ThemeError::Json(e.to_string()))?;
        let c = raw.colors;
        let colors = Colors {
            bg: color("bg", &c.bg)?,
            panel: color("panel", &c.panel)?,
            desk: color("desk", &c.desk)?,
            raised: color("raised", &c.raised)?,
            hover: color("hover", &c.hover)?,
            line: color("line", &c.line)?,
            text: color("text", &c.text)?,
            read: color("read", &c.read)?,
            muted: color("muted", &c.muted)?,
            faint: color("faint", &c.faint)?,
            ring: color("ring", &c.ring)?,
            accent: color("accent", &c.accent)?,
            highlight: color("highlight", &c.highlight)?,
            ok: color("ok", &c.ok)?,
            warn: color("warn", &c.warn)?,
            bad: color("bad", &c.bad)?,
            scrim: color("scrim", &c.scrim)?,
        };
        let d = raw.depth;
        let depth = Depth {
            elev: shadows("elev", d.elev)?,
            well: shadows("well", d.well)?,
            float: shadows("float", d.float)?,
            modal: shadows("modal", d.modal)?,
            sheet: shadows("sheet", d.sheet)?,
        };
        Ok(Theme {
            id: raw.id,
            name: raw.name,
            appearance: raw.appearance,
            colors,
            depth,
        })
    }
}

/// Load one built-in theme by id.
pub fn builtin(id: &str) -> Result<Theme, ThemeError> {
    let json = BUILTIN_JSON
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, j)| *j)
        .ok_or_else(|| ThemeError::UnknownBuiltin(id.to_string()))?;
    Theme::from_json(json)
}

/// Load all built-in themes, in Settings order.
pub fn builtin_all() -> Result<Vec<Theme>, ThemeError> {
    BUILTIN_IDS.iter().map(|id| builtin(id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{accent_for_contrast, contrast_ratio};

    fn themes() -> Vec<Theme> {
        builtin_all().expect("built-in themes must parse")
    }

    #[test]
    fn all_builtins_parse_with_matching_ids() {
        for (id, t) in BUILTIN_IDS.iter().zip(themes()) {
            assert_eq!(*id, t.id);
        }
    }

    // DESIGN.md § Colour: text and read ≥ 7:1 on bg, muted ≥ 4.5:1, faint
    // ≥ 4.5:1 on bg and panel, accent ≥ 3:1 as a mark.
    #[test]
    fn contrast_rules_hold_for_every_theme() {
        let mut failures = Vec::new();
        for t in themes() {
            let c = &t.colors;
            let checks = [
                ("text on bg", c.text, c.bg, 7.0),
                ("read on bg", c.read, c.bg, 7.0),
                ("muted on bg", c.muted, c.bg, 4.5),
                ("faint on bg", c.faint, c.bg, 4.5),
                ("faint on panel", c.faint, c.panel, 4.5),
                ("accent mark on bg", c.accent, c.bg, 3.0),
            ];
            for (what, fg, bg, need) in checks {
                let got = contrast_ratio(fg, bg);
                if got < need {
                    failures.push(format!("{}: {what} is {got:.2}:1, needs {need}:1", t.id));
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    // Accent used as text goes through the accent maths and must reach 4.5:1.
    #[test]
    fn accent_as_text_reaches_four_point_five() {
        for t in themes() {
            let text = accent_for_contrast(t.colors.accent, t.colors.bg, 4.5);
            assert!(contrast_ratio(text, t.colors.bg) >= 4.5, "{}: {text}", t.id);
        }
    }

    #[test]
    fn ultra_keeps_only_rings() {
        for t in themes() {
            let u = t.depth.ultra();
            for stack in [&u.elev, &u.float, &u.modal, &u.sheet] {
                assert_eq!(stack.len(), 1, "{}", t.id);
                assert!(stack[0].is_ring());
            }
        }
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert!(matches!(Theme::from_json("{"), Err(ThemeError::Json(_))));
        let dark = include_str!("../../../assets/themes/dark.json").replace("#141415", "#nothex");
        assert!(matches!(
            Theme::from_json(&dark),
            Err(ThemeError::Color { .. })
        ));
        assert!(builtin("solarized").is_err());
    }

    /// DESIGN.md and the theme files must agree (AGENTS.md: docs and code never
    /// disagree). Change both in the same commit.
    #[test]
    fn design_md_table_matches_theme_files() {
        let design = include_str!("../../../DESIGN.md");
        let section = design
            .split("### Surface and text tokens")
            .nth(1)
            .and_then(|s| s.split("### Accent").next())
            .expect("DESIGN.md has the colour table");
        let themes = themes();
        let mut rows = 0;
        for line in section.lines() {
            let Some(rest) = line.strip_prefix("| `") else {
                continue;
            };
            let cells: Vec<&str> = rest
                .split('|')
                .map(|c| c.trim().trim_matches('`'))
                .collect();
            let token = cells[0];
            for (i, t) in themes.iter().enumerate() {
                let want = Rgba::parse(cells[i + 1]).expect("DESIGN.md colour parses");
                let got = t.colors.get(token).expect("DESIGN.md token is known");
                assert_eq!(got.to_string(), want.to_string(), "{}.{token}", t.id);
            }
            rows += 1;
        }
        assert_eq!(rows, 17, "DESIGN.md lists 17 colour tokens");
    }
}
