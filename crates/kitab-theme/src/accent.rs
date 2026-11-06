// SPDX-License-Identifier: AGPL-3.0-or-later

//! Accent presets and custom accents (DESIGN.md § Accent). Any base colour is
//! adjusted per theme, keeping hue and chroma, until it reaches 3:1 on `bg`.

use crate::color::{Rgba, accent_for_contrast, contrast_ratio};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentPreset {
    /// The theme's own accent.
    Theme,
    Amber,
    Rose,
    Blue,
    Emerald,
    Grape,
    Wine,
    /// The theme's text colour: no hue at all.
    Mono,
}

impl AccentPreset {
    pub const ALL: [AccentPreset; 8] = [
        Self::Theme,
        Self::Amber,
        Self::Rose,
        Self::Blue,
        Self::Emerald,
        Self::Grape,
        Self::Wine,
        Self::Mono,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Amber => "amber",
            Self::Rose => "rose",
            Self::Blue => "blue",
            Self::Emerald => "emerald",
            Self::Grape => "grape",
            Self::Wine => "wine",
            Self::Mono => "mono",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Amber => "Amber",
            Self::Rose => "Rose",
            Self::Blue => "Blue",
            Self::Emerald => "Emerald",
            Self::Grape => "Grape",
            Self::Wine => "Wine",
            Self::Mono => "Mono",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The colour before the per-theme contrast adjustment.
    pub fn base(self, theme: &Theme) -> Rgba {
        match self {
            Self::Theme => theme.colors.accent,
            Self::Amber => Rgba::rgb(0xE6, 0xB4, 0x50),
            Self::Rose => Rgba::rgb(0xE0, 0x46, 0x6A),
            Self::Blue => Rgba::rgb(0x5B, 0x8C, 0xFF),
            Self::Emerald => Rgba::rgb(0x10, 0xB9, 0x81),
            Self::Grape => Rgba::rgb(0x8B, 0x5C, 0xF6),
            Self::Wine => Rgba::rgb(0x9F, 0x23, 0x4A),
            Self::Mono => theme.colors.text,
        }
    }
}

/// The accent as chosen: a preset or any colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Accent {
    Preset(AccentPreset),
    Custom(Rgba),
}

impl Default for Accent {
    fn default() -> Self {
        Self::Preset(AccentPreset::Theme)
    }
}

impl Accent {
    /// `"theme"`, a preset id, or a hex colour such as `"#12AB34"`.
    pub fn parse(s: &str) -> Option<Self> {
        AccentPreset::from_id(s).map(Self::Preset).or_else(|| {
            s.starts_with('#')
                .then(|| Rgba::parse(s))
                .flatten()
                .map(|c| Self::Custom(Rgba { a: 1.0, ..c }))
        })
    }

    pub fn id(&self) -> String {
        match self {
            Self::Preset(p) => p.id().to_string(),
            Self::Custom(c) => c.to_string(),
        }
    }

    pub fn base(&self, theme: &Theme) -> Rgba {
        match self {
            Self::Preset(p) => p.base(theme),
            Self::Custom(c) => *c,
        }
    }

    /// The accent `theme` uses: the base colour, made to reach 3:1 on `bg`.
    pub fn resolve(&self, theme: &Theme) -> Rgba {
        accent_for_contrast(self.base(theme), theme.colors.bg, 3.0)
    }

    /// The base colour's own contrast on `theme`'s background, and whether
    /// Kitab had to adjust it (shown next to the custom hex field).
    pub fn measure(&self, theme: &Theme) -> (f32, bool) {
        let base = self.base(theme);
        let ratio = contrast_ratio(base, theme.colors.bg);
        (ratio, ratio < 3.0)
    }
}

/// A colour on the hue strip of the custom picker: `turn` in 0..1 around the
/// OKLCH hue circle at a lightness and chroma that suit both light and dark themes.
pub fn hue_swatch(turn: f32) -> Rgba {
    Rgba::from_oklch(0.68, 0.16, turn * std::f32::consts::TAU)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::builtin_all;

    #[test]
    fn every_preset_and_hue_reaches_three_to_one_on_every_theme() {
        let themes = builtin_all().expect("themes parse");
        for t in &themes {
            let mut accents: Vec<Accent> =
                AccentPreset::ALL.into_iter().map(Accent::Preset).collect();
            accents.extend((0..36).map(|i| Accent::Custom(hue_swatch(i as f32 / 36.0))));
            accents.push(Accent::Custom(Rgba::rgb(0, 0, 0)));
            accents.push(Accent::Custom(Rgba::rgb(255, 255, 255)));
            for a in accents {
                let got = contrast_ratio(a.resolve(t), t.colors.bg);
                assert!(got >= 2.99, "{} {:?}: {got:.2}", t.id, a);
            }
        }
    }

    #[test]
    fn parses_presets_and_hex() {
        assert_eq!(
            Accent::parse("emerald"),
            Some(Accent::Preset(AccentPreset::Emerald))
        );
        assert_eq!(
            Accent::parse("#10B981").map(|a| a.id()),
            Some("#10B981".to_string())
        );
        assert_eq!(Accent::parse("teal"), None);
        assert_eq!(Accent::parse("#12"), None);
    }
}
