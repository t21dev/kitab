// SPDX-License-Identifier: AGPL-3.0-or-later

//! User settings: `settings.json` in the data folder (PLAN.md §3.6), human
//! editable. Unknown keys are ignored and missing ones take their defaults, so
//! old and hand-edited files keep working. The app reads the file before the
//! first frame (the theme is needed to draw it) and writes it off the UI thread.

use kitab_theme::{Accent, CodeTheme, Density};
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

/// Where the quick dock (zoom, fit, find, rotate, ⋯) floats over the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DockAt {
    Top,
    /// With the page navigator, as one bar.
    #[default]
    Bottom,
    Left,
    Right,
}

impl DockAt {
    pub const ALL: [DockAt; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Top => "Under the tabs",
            Self::Bottom => "With the page navigator",
            Self::Left => "Upright, on the left",
            Self::Right => "Upright, on the right",
        }
    }

    pub fn upright(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// Where open documents are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TabsAt {
    /// Along the top, like a browser.
    #[default]
    Top,
    /// A list at the top of the sidebar; no title bar.
    Sidebar,
}

impl TabsAt {
    pub const ALL: [TabsAt; 2] = [Self::Top, Self::Sidebar];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Along the top",
            Self::Sidebar => "In the sidebar",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Top => "Like a browser",
            Self::Sidebar => "Full titles, more page height",
        }
    }
}

/// What minimal mode (F11) hides. The window buttons and a way back stay
/// reachable at the top edge whatever is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MinimalHides {
    pub tabs: bool,
    pub sidebar: bool,
    pub dock: bool,
    pub page_navigator: bool,
}

impl Default for MinimalHides {
    fn default() -> Self {
        Self {
            tabs: true,
            sidebar: true,
            dock: true,
            page_navigator: true,
        }
    }
}

/// A whole-look preset (Settings › Appearance › Layout). Not stored: the
/// current settings either match a preset or are "Custom".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Everything on: motion, depth, the full dock and the scrubber.
    Full,
    /// Lean and fast: no motion, flat surfaces, a small dock, the page counter.
    Minimal,
    Custom,
}

impl Layout {
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Minimal => "Minimal",
            Self::Custom => "Custom",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Full => "Everything, with motion and depth",
            Self::Minimal => "Fewer controls, no motion, faster",
            Self::Custom => "Your own mix",
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
    pub dock: DockAt,
    /// The dock fades out while reading and comes back on pointer movement.
    pub dock_fade: bool,
    pub tabs: TabsAt,
    pub minimal: MinimalHides,
    /// The sidebar (contents, and tabs when they live there) was open last time.
    pub sidebar: bool,
    /// Flat surfaces: shadows reduced to hairline rings (less to draw).
    pub flat: bool,
    /// The dock keeps only zoom, find and ⋯.
    pub dock_compact: bool,
    /// Colours of code blocks in Markdown.
    pub code_theme: CodeTheme,
    /// Text size of books (EPUB, MOBI, FB2, TXT) in points; zoom in a book
    /// changes it.
    pub book_text: f32,
    /// Drawing and markup tools (the dock's pen, Ctrl+Shift+D).
    pub markup: bool,
    /// The file info chip at the bottom left (format, size, pages, paper).
    pub file_info: bool,
    /// The dock is tucked into a handle at its edge (Ctrl+.).
    pub dock_hidden: bool,
}

impl Settings {
    /// Which preset these settings match, if any.
    pub fn layout(&self) -> Layout {
        let full = self.animations
            && !self.flat
            && !self.dock_compact
            && self.page_nav == PageNav::Scrubber
            && !self.dock_fade;
        let minimal = !self.animations
            && self.flat
            && self.dock_compact
            && self.page_nav == PageNav::Counter
            && !self.dock_fade;
        if full {
            Layout::Full
        } else if minimal {
            Layout::Minimal
        } else {
            Layout::Custom
        }
    }

    /// Apply a preset; theme, accent, density, tabs and the dock position stay.
    pub fn apply_layout(&mut self, layout: Layout) {
        match layout {
            Layout::Full => {
                self.animations = true;
                self.flat = false;
                self.dock_compact = false;
                self.page_nav = PageNav::Scrubber;
                self.dock_fade = false;
                self.sidebar = true;
            }
            Layout::Minimal => {
                self.animations = false;
                self.flat = true;
                self.dock_compact = true;
                self.page_nav = PageNav::Counter;
                self.dock_fade = false;
                self.sidebar = false;
            }
            Layout::Custom => {}
        }
    }
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
            dock: DockAt::default(),
            dock_fade: false,
            tabs: TabsAt::default(),
            minimal: MinimalHides::default(),
            sidebar: true,
            flat: false,
            dock_compact: false,
            code_theme: CodeTheme::Kitab,
            book_text: BOOK_TEXT_DEFAULT,
            markup: true,
            file_info: true,
            dock_hidden: false,
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
    dock: DockAt,
    dock_fade: bool,
    tabs: TabsAt,
    minimal: MinimalHides,
    sidebar_open: bool,
    flat: bool,
    dock_compact: bool,
    /// A `CodeTheme` id ("kitab", "github", …).
    code_colours: String,
    /// Points.
    book_text_size: f32,
    markup_tools: bool,
    file_info_chip: bool,
    dock_hidden: bool,
}

/// Books open at this text size (12 pt reads as 16 px at 100 %).
pub const BOOK_TEXT_DEFAULT: f32 = 12.0;

/// A text size Kitab offers; a hand-edited or broken value falls back.
pub fn clamp_book_text(pt: f32) -> f32 {
    use kitab_core::BookLayout;
    if pt.is_finite() {
        pt.clamp(BookLayout::EM_MIN, BookLayout::EM_MAX)
    } else {
        BOOK_TEXT_DEFAULT
    }
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
            dock: s.dock,
            dock_fade: s.dock_fade,
            tabs: s.tabs,
            minimal: s.minimal,
            sidebar_open: s.sidebar,
            flat: s.flat,
            dock_compact: s.dock_compact,
            code_colours: s.code_theme.id().to_string(),
            book_text_size: s.book_text,
            markup_tools: s.markup,
            file_info_chip: s.file_info,
            dock_hidden: s.dock_hidden,
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
            dock: f.dock,
            dock_fade: f.dock_fade,
            tabs: f.tabs,
            minimal: f.minimal,
            sidebar: f.sidebar_open,
            flat: f.flat,
            dock_compact: f.dock_compact,
            // A palette Kitab doesn't know falls back to the theme's colours.
            code_theme: CodeTheme::from_id(&f.code_colours).unwrap_or_default(),
            book_text: clamp_book_text(f.book_text_size),
            markup: f.markup_tools,
            file_info: f.file_info_chip,
            dock_hidden: f.dock_hidden,
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
            dock: DockAt::Right,
            dock_fade: true,
            tabs: TabsAt::Sidebar,
            minimal: MinimalHides {
                tabs: true,
                sidebar: false,
                dock: true,
                page_navigator: false,
            },
            sidebar: false,
            flat: true,
            dock_compact: true,
            code_theme: CodeTheme::Dracula,
            book_text: 15.0,
            markup: false,
            file_info: false,
            dock_hidden: true,
        };
        assert_eq!(Settings::from_json(&s.to_json()), Ok(s));
    }

    #[test]
    fn book_text_is_kept_in_range() {
        let s = Settings::from_json(r#"{"book_text_size": 400}"#).expect("parses");
        assert_eq!(s.book_text, kitab_core::BookLayout::EM_MAX);
        let s = Settings::from_json(r#"{"book_text_size": 1}"#).expect("parses");
        assert_eq!(s.book_text, kitab_core::BookLayout::EM_MIN);
        assert_eq!(Settings::default().book_text, BOOK_TEXT_DEFAULT);
    }

    #[test]
    fn layout_presets_round_trip() {
        let mut s = Settings::default();
        assert_eq!(s.layout(), Layout::Full);
        s.apply_layout(Layout::Minimal);
        assert_eq!(s.layout(), Layout::Minimal);
        assert!(!s.animations && s.flat && s.dock_compact);
        s.page_nav = PageNav::Stepper;
        assert_eq!(s.layout(), Layout::Custom);
        s.apply_layout(Layout::Full);
        assert_eq!(s.layout(), Layout::Full);
    }

    #[test]
    fn missing_keys_take_defaults_and_unknown_keys_are_ignored() {
        let s = Settings::from_json(r#"{ "page_navigator": "counter", "future_key": 3 }"#)
            .expect("parses");
        assert_eq!(s.page_nav, PageNav::Counter);
        assert_eq!(s.page_corners, PageCorners::Square);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.accent, Accent::default());
        assert_eq!(s.dock, DockAt::Bottom);
        assert!(!s.dock_fade);
        assert_eq!(s.tabs, TabsAt::Top);
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
