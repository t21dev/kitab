// SPDX-License-Identifier: AGPL-3.0-or-later

//! User settings: `settings.json` in the data folder (PLAN.md §3.6), human
//! editable. Unknown keys are ignored and missing ones take their defaults, so
//! old and hand-edited files keep working. The app reads the file before the
//! first frame (the theme is needed to draw it) and writes it off the UI thread.

use kitab_theme::{Accent, Density};
use serde::{Deserialize, Serialize};

/// The settings format version, independent of the app version (PLAN.md §11).
pub const SETTINGS_VERSION: u32 = 1;

/// How the reader shows and changes the current page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageNav {
    /// Floating pill: page field, a track you can click or drag, the total.
    #[default]
    Scrubber,
    /// Floating pill: previous, page field, "of N", next.
    Stepper,
    /// A small page field and total in the bottom-right corner.
    Counter,
    /// Nothing on screen; Ctrl+G still opens the page field.
    Hidden,
}

impl PageNav {
    pub const ALL: [PageNav; 4] = [Self::Scrubber, Self::Stepper, Self::Counter, Self::Hidden];

    pub fn label(self) -> &'static str {
        match self {
            Self::Scrubber => "Scrubber",
            Self::Stepper => "Stepper",
            Self::Counter => "Counter",
            Self::Hidden => "Hidden",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Scrubber => "Drag or click the track",
            Self::Stepper => "Previous and next buttons",
            Self::Counter => "Quiet, in the corner",
            Self::Hidden => "Ctrl+G when you need it",
        }
    }
}

/// The corner radius of page sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageCorners {
    /// DESIGN.md's 2 px sheet: reads as paper.
    #[default]
    Square,
    Soft,
    Round,
}

impl PageCorners {
    pub const ALL: [PageCorners; 3] = [Self::Square, Self::Soft, Self::Round];

    /// Radius in logical pixels at 100 % zoom; the reader scales it with the page.
    pub fn radius(self) -> f32 {
        match self {
            Self::Square => 2.0,
            Self::Soft => 8.0,
            Self::Round => 16.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Soft => "Soft",
            Self::Round => "Round",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DensityName {
    Dense,
    #[default]
    Compact,
    Standard,
}

impl From<DensityName> for Density {
    fn from(d: DensityName) -> Self {
        match d {
            DensityName::Dense => Density::Dense,
            DensityName::Compact => Density::Compact,
            DensityName::Standard => Density::Standard,
        }
    }
}

impl From<Density> for DensityName {
    fn from(d: Density) -> Self {
        match d {
            Density::Dense => DensityName::Dense,
            Density::Compact => DensityName::Compact,
            Density::Standard => DensityName::Standard,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// A built-in theme id (`kitab_theme::BUILTIN_IDS`).
    pub theme: String,
    pub density: Density,
    /// A preset or a custom colour; adjusted per theme for contrast.
    pub accent: Accent,
    pub page_nav: PageNav,
    pub page_corners: PageCorners,
    /// Opening and closing motion for menus, dialogs, panels and lists.
    pub animations: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            density: Density::Compact,
            accent: Accent::default(),
            page_nav: PageNav::default(),
            page_corners: PageCorners::default(),
            animations: true,
        }
    }
}

/// The file's shape. Every field is optional on read.
#[derive(Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
struct File {
    version: u32,
    theme: String,
    density: DensityName,
    /// `"theme"`, a preset id (`"emerald"`) or a hex colour (`"#10B981"`).
    accent: String,
    page_navigator: PageNav,
    page_corners: PageCorners,
    animations: bool,
}

impl Default for File {
    fn default() -> Self {
        Settings::default().into()
    }
}

impl From<Settings> for File {
    fn from(s: Settings) -> Self {
        Self {
            version: SETTINGS_VERSION,
            theme: s.theme,
            density: s.density.into(),
            accent: s.accent.id(),
            page_navigator: s.page_nav,
            page_corners: s.page_corners,
            animations: s.animations,
        }
    }
}

impl Settings {
    /// Parse `settings.json`. A value that doesn't parse (say, an unknown
    /// navigator name) fails the whole file, and the caller keeps the last good
    /// settings (PLAN.md §3.6).
    pub fn from_json(text: &str) -> Result<Self, String> {
        let f: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let theme = if kitab_theme::BUILTIN_IDS.contains(&f.theme.as_str()) {
            f.theme
        } else {
            Settings::default().theme
        };
        Ok(Self {
            theme,
            density: f.density.into(),
            // An accent Kitab doesn't know falls back to the theme's.
            accent: Accent::parse(&f.accent).unwrap_or_default(),
            page_nav: f.page_navigator,
            page_corners: f.page_corners,
            animations: f.animations,
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&File::from(self.clone())).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings {
            theme: "nord".into(),
            density: Density::Standard,
            accent: Accent::Preset(kitab_theme::AccentPreset::Emerald),
            page_nav: PageNav::Stepper,
            page_corners: PageCorners::Round,
            animations: false,
        };
        assert_eq!(Settings::from_json(&s.to_json()), Ok(s));
    }

    #[test]
    fn missing_keys_take_defaults_and_unknown_keys_are_ignored() {
        let s = Settings::from_json(r#"{ "page_navigator": "counter", "future_key": 3 }"#)
            .expect("parses");
        assert_eq!(s.page_nav, PageNav::Counter);
        assert_eq!(s.page_corners, PageCorners::Square);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.accent, Accent::default());
    }

    #[test]
    fn accent_presets_and_hex_round_trip() {
        let s = Settings::from_json(r#"{ "accent": "wine" }"#).expect("parses");
        assert_eq!(s.accent.id(), "wine");
        let s = Settings::from_json(r#"{ "accent": "not-a-colour" }"#).expect("parses");
        assert_eq!(s.accent, Accent::default());
    }

    #[test]
    fn unknown_theme_falls_back_and_bad_values_fail() {
        assert_eq!(
            Settings::from_json(r#"{ "theme": "neon" }"#).map(|s| s.theme),
            Ok("dark".into())
        );
        assert!(Settings::from_json(r#"{ "page_navigator": "wheel" }"#).is_err());
        assert!(Settings::from_json("not json").is_err());
    }
}
