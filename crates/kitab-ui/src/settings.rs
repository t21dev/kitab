// SPDX-License-Identifier: AGPL-3.0-or-later

//! User settings: `settings.json` in the data folder (PLAN.md §3.6), human
//! editable. Unknown keys are ignored and missing ones take their defaults, so
//! old and hand-edited files keep working. The app reads the file before the
//! first frame (the theme is needed to draw it) and writes it off the UI thread.

use kitab_theme::{Accent, CodeTheme, Density};
use serde::{Deserialize, Serialize};

/// The settings format version, independent of the app version (PLAN.md §11).
pub const SETTINGS_VERSION: u32 = 1;

/// The interface sizes on offer (Settings › Appearance), in percent.
pub const UI_SCALES: [u16; 7] = [90, 100, 110, 125, 150, 175, 200];

/// Interface size works only where Kitab's gpui patch scales windows
/// (Windows). Elsewhere the setting is hidden: it would do nothing.
pub const UI_SCALE_WORKS: bool = cfg!(windows);

/// Markdown text sizes on offer (Settings › Reading), in percent. A
/// Markdown file opens at this size; zoom changes it per file from there.
pub const MD_TEXT_SIZES: [u16; 7] = [100, 125, 150, 175, 200, 250, 300];

/// How a PDF, comic or XPS file is sized the first time it is opened
/// (Settings › Reading). A document opened before comes back as it was left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenZoom {
    /// The page as wide as the window.
    #[default]
    FitWidth,
    /// The whole page in the window.
    FitPage,
    /// 100 %.
    ActualSize,
}

impl OpenZoom {
    pub const ALL: [OpenZoom; 3] = [Self::FitWidth, Self::FitPage, Self::ActualSize];

    pub fn label(self) -> &'static str {
        match self {
            Self::FitWidth => "Fit width",
            Self::FitPage => "Fit page",
            Self::ActualSize => "Actual size",
        }
    }

    pub fn mode(self) -> kitab_core::ZoomMode {
        match self {
            Self::FitWidth => kitab_core::ZoomMode::FitWidth,
            Self::FitPage => kitab_core::ZoomMode::FitPage,
            Self::ActualSize => kitab_core::ZoomMode::ActualSize,
        }
    }
}

/// The typeface Markdown is read in (the dock's Aa button, Settings › Reading).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MdFace {
    /// The system's interface font (Segoe UI, San Francisco, …).
    #[default]
    System,
    Serif,
    Mono,
}

impl MdFace {
    pub const ALL: [MdFace; 3] = [Self::System, Self::Serif, Self::Mono];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Serif => "Serif",
            Self::Mono => "Mono",
        }
    }

    /// The next one, for the dock's button.
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Serif,
            Self::Serif => Self::Mono,
            Self::Mono => Self::System,
        }
    }

    /// The font family to set.
    pub fn family(self) -> &'static str {
        match self {
            Self::System => {
                if cfg!(target_os = "macos") {
                    ".SystemUIFont"
                } else if cfg!(windows) {
                    "Segoe UI"
                } else {
                    "Noto Sans"
                }
            }
            // Cambria on Windows: lining figures (Georgia's old-style "1"
            // reads as "ı").
            Self::Serif => {
                if cfg!(windows) {
                    "Cambria"
                } else {
                    "Georgia"
                }
            }
            Self::Mono => {
                if cfg!(target_os = "macos") {
                    "Menlo"
                } else if cfg!(windows) {
                    "Consolas"
                } else {
                    "DejaVu Sans Mono"
                }
            }
        }
    }
}

/// What Kitab shows when it starts without a file (Settings › General).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Startup {
    /// The tabs open when Kitab last closed (files it was started with join them).
    #[default]
    Restore,
    /// Home, with no tabs.
    Home,
    /// Only the tab that was showing when Kitab last closed.
    LastFile,
}

impl Startup {
    pub const ALL: [Startup; 3] = [Self::Restore, Self::LastFile, Self::Home];
    pub fn label(self) -> &'static str {
        match self {
            Self::Restore => "The tabs I had open",
            Self::LastFile => "The last file I read",
            Self::Home => "Home",
        }
    }
}

impl PageColour {
    pub const ALL: [PageColour; 3] = [Self::Original, Self::Dark, Self::Theme];
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "As the file has them",
            Self::Dark => "Dark",
            Self::Theme => "Theme colours",
        }
    }
}

/// How the pages of PDFs, comics, DjVu and XPS are coloured (Settings ›
/// Reading). Books, Markdown and text follow the theme already.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageColour {
    /// As the file draws them.
    #[default]
    Original,
    /// Light and dark swapped, colours kept (night reading).
    Dark,
    /// White paper takes the theme's page colour, black ink its text colour.
    Theme,
}

/// What the tab bar's + does (Settings › Appearance › Tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NewTab {
    /// Show Home, with Open file… and the recent books.
    #[default]
    Home,
    /// Go straight to the Open dialog.
    OpenFile,
}

/// Where messages ("Saved", "Couldn't print") appear (Settings › Appearance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToastAt {
    TopLeft,
    #[default]
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl ToastAt {
    pub const ALL: [ToastAt; 6] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "Top left",
            Self::TopCenter => "Top centre",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomCenter => "Bottom centre",
            Self::BottomRight => "Bottom right",
        }
    }

    pub fn top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopCenter | Self::TopRight)
    }

    /// 0 left, 1 centre, 2 right.
    pub fn side(self) -> u8 {
        match self {
            Self::TopLeft | Self::BottomLeft => 0,
            Self::TopCenter | Self::BottomCenter => 1,
            Self::TopRight | Self::BottomRight => 2,
        }
    }

    /// The one at `top` (or the bottom) and `side` (0 left, 1 centre, 2 right).
    pub fn at(top: bool, side: u8) -> Self {
        match (top, side) {
            (true, 0) => Self::TopLeft,
            (true, 2) => Self::TopRight,
            (true, _) => Self::TopCenter,
            (false, 0) => Self::BottomLeft,
            (false, 2) => Self::BottomRight,
            (false, _) => Self::BottomCenter,
        }
    }
}

/// How Home shows the books (Settings › Home).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HomeLayout {
    /// A grid of covers.
    #[default]
    Covers,
    /// One book per row: a small cover, the title and author, progress.
    List,
}

impl HomeLayout {
    pub const ALL: [HomeLayout; 2] = [Self::Covers, Self::List];

    pub fn label(self) -> &'static str {
        match self {
            Self::Covers => "Covers",
            Self::List => "List",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Covers => "A shelf of first pages",
            Self::List => "More books on screen, with details",
        }
    }
}

/// How big the covers are on Home.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl CoverSize {
    pub const ALL: [CoverSize; 3] = [Self::Small, Self::Medium, Self::Large];

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }

    /// A card's width in points.
    pub fn width(self) -> f32 {
        match self {
            Self::Small => 104.0,
            Self::Medium => 132.0,
            Self::Large => 168.0,
        }
    }
}

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

/// What the quick dock shows (Settings › Appearance › Dock items). More (⋯)
/// and the hide chevron are always there: everything else is reachable from
/// More, and the chevron is how a hidden dock comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockItems {
    pub sidebar: bool,
    pub history: bool,
    pub zoom: bool,
    pub fit: bool,
    pub find: bool,
    pub markup: bool,
    pub rotate: bool,
    pub bookmark: bool,
    /// Read aloud: start, pause and resume (stop sits beside it while reading).
    pub read_aloud: bool,
    pub minimal: bool,
    pub print: bool,
    pub password: bool,
}

impl Default for DockItems {
    fn default() -> Self {
        Self {
            sidebar: true,
            history: true,
            zoom: true,
            fit: true,
            find: true,
            markup: true,
            rotate: true,
            bookmark: false,
            read_aloud: true,
            minimal: false,
            print: false,
            password: false,
        }
    }
}

/// Read aloud's voice, speed and tone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadAloud {
    /// A system voice's id; empty for the system's default voice.
    pub voice: String,
    /// Its name, to show before the system's voices are loaded.
    pub voice_name: String,
    /// Speed, a multiple of the voice's own (0.5 to 3).
    pub rate: f32,
    /// Tone, a multiple of the voice's own (0.5 to 1.5).
    pub pitch: f32,
    /// Keep the spoken word in view (scrolling away turns it off for that reading).
    pub follow: bool,
}

impl Default for ReadAloud {
    fn default() -> Self {
        Self {
            voice: String::new(),
            voice_name: String::new(),
            rate: 1.0,
            pitch: 1.0,
            follow: true,
        }
    }
}

impl ReadAloud {
    pub const SPEEDS: [f32; 6] = [0.75, 1.0, 1.25, 1.5, 2.0, 3.0];
    pub const TONES: [(&'static str, f32); 3] = [("Lower", 0.8), ("Normal", 1.0), ("Higher", 1.2)];

    pub fn params(&self) -> kitab_speech::Params {
        kitab_speech::Params {
            rate: self.rate,
            pitch: self.pitch,
        }
        .clamped()
    }

    /// A hand-edited value out of range, pulled back in.
    pub fn clamped(self) -> Self {
        let p = self.params();
        Self {
            voice: self.voice.chars().take(512).collect(),
            voice_name: self.voice_name.chars().take(200).collect(),
            rate: p.rate,
            pitch: p.pitch,
            follow: self.follow,
        }
    }

    /// The voice to ask for: `None` for the system's default.
    pub fn voice(&self) -> Option<String> {
        (!self.voice.is_empty()).then(|| self.voice.clone())
    }
}

/// The buttons that sit in the title bar instead of the dock (Settings ›
/// Appearance › Toolbar). A button here is left out of the dock. More (⋯)
/// lives in one of the two; the dock's hide chevron always stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleItems {
    pub sidebar: bool,
    pub history: bool,
    pub zoom: bool,
    pub fit: bool,
    pub find: bool,
    pub markup: bool,
    pub rotate: bool,
    pub bookmark: bool,
    pub minimal: bool,
    pub print: bool,
    pub password: bool,
    pub more: bool,
    /// Open file… (the title bar only; on Home too).
    pub open: bool,
}

impl TitleItems {
    pub const NONE: TitleItems = TitleItems {
        sidebar: false,
        history: false,
        zoom: false,
        fit: false,
        find: false,
        markup: false,
        rotate: false,
        bookmark: false,
        minimal: false,
        print: false,
        password: false,
        more: false,
        open: false,
    };
}

impl Default for TitleItems {
    /// Find, Draw, Password and More up top: the dock keeps paging and zoom.
    fn default() -> Self {
        Self {
            find: true,
            markup: true,
            password: true,
            more: true,
            open: true,
            ..Self::NONE
        }
    }
}

/// What the status bar shows (Settings › Reading › Status bar): facts about
/// the file on the left, where you are on the right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusItems {
    pub format: bool,
    pub size: bool,
    /// Pages, or lines in Markdown.
    pub pages: bool,
    /// Words in Markdown.
    pub words: bool,
    /// Minutes to read a Markdown file (at 230 words a minute).
    pub reading_time: bool,
    /// The current page's paper size, or a book's text size.
    pub paper: bool,
    /// "Page 3 of 40", or how far through a Markdown file.
    pub position: bool,
    pub zoom: bool,
    /// Where the file is: its folder and name (your home folder as ~).
    pub path: bool,
}

impl Default for StatusItems {
    fn default() -> Self {
        Self {
            format: true,
            size: true,
            pages: true,
            words: true,
            reading_time: false,
            paper: true,
            position: true,
            zoom: false,
            path: false,
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

/// Where the main window was when Kitab last closed: its normal (restored)
/// position and size in screen points, and whether it was maximised.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowPlace {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub maximized: bool,
    /// Windows: the normal rectangle in Windows' own pixels, which puts the
    /// window back exactly on any monitor whatever its scaling.
    pub raw: Option<[i32; 4]>,
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
    /// Typeface, line spacing and margins for flowing books (the default
    /// keeps the book's own).
    pub book_style: kitab_core::BookStyle,
    /// Drawing and markup tools (the dock's pen, Ctrl+Shift+D).
    pub markup: bool,
    /// The file info chip at the bottom left (format, size, pages, paper).
    pub file_info: bool,
    /// The dock is tucked into a handle at its edge (Ctrl+.).
    pub dock_hidden: bool,
    /// The file info chip is folded to an "i" button (Ctrl+I).
    pub file_info_folded: bool,
    /// The quick dock's buttons.
    pub dock_items: DockItems,
    /// The status bar's items.
    pub status_items: StatusItems,
    /// Buttons in the title bar rather than the dock.
    pub title_items: TitleItems,
    /// Ask GitHub for a newer release shortly after start-up and every six
    /// hours (Settings › About › Check on start-up). On by default, the
    /// owner's call (AGENTS.md rule 12); the switch turns it off.
    pub update_check: bool,
    /// A release the user put off with Later; its pill stays hidden.
    pub skipped_update: String,
    /// The scroll bar along the right of the pages (Settings › Reading).
    pub scroll_bar: bool,
    /// The mouse wheel and the keys glide to where they scroll, instead of
    /// jumping there (Settings › Reading).
    pub smooth_scroll: bool,
    /// How many tabs are open, as a small number in the title bar.
    pub tab_count: bool,
    /// File, Edit, View, Go, Tools and Help under the title bar
    /// (Settings › Appearance). Off unless asked for.
    pub menu_bar: bool,
    /// Read aloud's voice, speed and tone (Settings › Reading).
    pub read_aloud: ReadAloud,
    /// Home: covers or a list, the covers' size, and what's above them.
    pub home_layout: HomeLayout,
    pub cover_size: CoverSize,
    pub home_stats: bool,
    pub home_continue: bool,
    /// The reading progress bar under each book on Home (covers and list).
    pub home_progress: bool,
    /// Interface size, in percent of the system's scaling (100 = as is).
    pub ui_scale: u16,
    /// Where messages appear.
    pub toast_at: ToastAt,
    /// Markdown's typeface.
    pub md_face: MdFace,
    /// Markdown's text size in percent (one of `MD_TEXT_SIZES`).
    pub md_text: u16,
    /// How a document is sized the first time it is opened.
    pub open_zoom: OpenZoom,
    /// What the tab bar's + does.
    pub new_tab: NewTab,
    pub startup: Startup,
    /// Follow the system's light or dark mode with `light_theme` / `dark_theme`.
    pub system_theme: bool,
    pub light_theme: String,
    pub dark_theme: String,
    /// The font for code and the Markdown editor; empty for the system's own.
    pub code_font: String,
    /// Comics read right to left (manga): the first page on the right.
    pub comic_rtl: bool,
    pub page_colour: PageColour,
    /// The main window's last place (none before the first close).
    pub window: Option<WindowPlace>,
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
            book_style: kitab_core::BookStyle::DEFAULT,
            markup: true,
            file_info: true,
            dock_hidden: false,
            file_info_folded: false,
            dock_items: DockItems::default(),
            status_items: StatusItems::default(),
            title_items: TitleItems::default(),
            update_check: true,
            skipped_update: String::new(),
            scroll_bar: true,
            smooth_scroll: true,
            tab_count: false,
            menu_bar: false,
            read_aloud: ReadAloud::default(),
            home_layout: HomeLayout::Covers,
            cover_size: CoverSize::Medium,
            home_stats: true,
            home_continue: true,
            home_progress: true,
            ui_scale: 100,
            toast_at: ToastAt::TopCenter,
            md_face: MdFace::System,
            md_text: 100,
            open_zoom: OpenZoom::FitWidth,
            new_tab: NewTab::Home,
            startup: Startup::Restore,
            system_theme: false,
            light_theme: "light".into(),
            dark_theme: "dark".into(),
            code_font: String::new(),
            comic_rtl: false,
            page_colour: PageColour::Original,
            window: None,
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
    /// "book", "serif", "sans" or "mono".
    book_face: String,
    /// Line height in text sizes; null keeps the book's.
    book_line_spacing: Option<f32>,
    /// Page margins in em; null keeps the book's.
    book_margin: Option<f32>,
    markup_tools: bool,
    file_info_chip: bool,
    dock_hidden: bool,
    file_info_folded: bool,
    dock_items: DockItems,
    status_items: StatusItems,
    title_items: TitleItems,
    check_for_updates: bool,
    skipped_update: String,
    scroll_bar: bool,
    smooth_scroll: bool,
    tab_count: bool,
    menu_bar: bool,
    read_aloud: ReadAloud,
    home_layout: HomeLayout,
    cover_size: CoverSize,
    home_stats: bool,
    home_continue: bool,
    home_progress: bool,
    ui_scale: u16,
    toast_at: ToastAt,
    md_face: MdFace,
    /// Percent.
    md_text_size: u16,
    open_zoom: OpenZoom,
    new_tab: NewTab,
    startup: Startup,
    system_theme: bool,
    light_theme: String,
    dark_theme: String,
    code_font: String,
    comic_rtl: bool,
    page_colour: PageColour,
    window: Option<WindowPlace>,
}

/// A built-in theme id, or `fallback` for one Kitab doesn't have.
fn theme_id(id: String, fallback: &str) -> String {
    if kitab_theme::BUILTIN_IDS.contains(&id.as_str()) {
        id
    } else {
        fallback.into()
    }
}

fn face_name(f: kitab_core::BookFace) -> &'static str {
    use kitab_core::BookFace::*;
    match f {
        Book => "book",
        Serif => "serif",
        Sans => "sans",
        Mono => "mono",
    }
}

/// Book typography from settings.json; a value out of range falls back to
/// the book's own.
fn book_style_from(face: &str, line: Option<f32>, margin: Option<f32>) -> kitab_core::BookStyle {
    use kitab_core::{BookFace, BookStyle};
    let face = match face {
        "serif" => BookFace::Serif,
        "sans" => BookFace::Sans,
        "mono" => BookFace::Mono,
        _ => BookFace::Book,
    };
    let line =
        line.filter(|v| v.is_finite() && (BookStyle::LINE_MIN..=BookStyle::LINE_MAX).contains(v));
    let margin = margin.filter(|v| v.is_finite() && (0.0..=BookStyle::MARGIN_MAX).contains(v));
    BookStyle {
        face,
        line_spacing: line,
        margin,
        ..BookStyle::DEFAULT
    }
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
            book_face: face_name(s.book_style.face).into(),
            book_line_spacing: s.book_style.line_spacing,
            book_margin: s.book_style.margin,
            markup_tools: s.markup,
            file_info_chip: s.file_info,
            dock_hidden: s.dock_hidden,
            file_info_folded: s.file_info_folded,
            dock_items: s.dock_items,
            status_items: s.status_items,
            title_items: s.title_items,
            check_for_updates: s.update_check,
            skipped_update: s.skipped_update,
            scroll_bar: s.scroll_bar,
            smooth_scroll: s.smooth_scroll,
            tab_count: s.tab_count,
            menu_bar: s.menu_bar,
            read_aloud: s.read_aloud,
            home_layout: s.home_layout,
            cover_size: s.cover_size,
            home_stats: s.home_stats,
            home_continue: s.home_continue,
            home_progress: s.home_progress,
            ui_scale: s.ui_scale,
            toast_at: s.toast_at,
            md_face: s.md_face,
            md_text_size: s.md_text,
            open_zoom: s.open_zoom,
            new_tab: s.new_tab,
            startup: s.startup,
            system_theme: s.system_theme,
            light_theme: s.light_theme,
            dark_theme: s.dark_theme,
            code_font: s.code_font,
            comic_rtl: s.comic_rtl,
            page_colour: s.page_colour,
            window: s.window,
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
            book_style: book_style_from(&f.book_face, f.book_line_spacing, f.book_margin),
            markup: f.markup_tools,
            file_info: f.file_info_chip,
            dock_hidden: f.dock_hidden,
            file_info_folded: f.file_info_folded,
            dock_items: f.dock_items,
            status_items: f.status_items,
            title_items: f.title_items,
            update_check: f.check_for_updates,
            skipped_update: f.skipped_update.chars().take(32).collect(),
            scroll_bar: f.scroll_bar,
            smooth_scroll: f.smooth_scroll,
            tab_count: f.tab_count,
            menu_bar: f.menu_bar,
            read_aloud: f.read_aloud.clamped(),
            home_layout: f.home_layout,
            cover_size: f.cover_size,
            home_stats: f.home_stats,
            home_continue: f.home_continue,
            home_progress: f.home_progress,
            ui_scale: if UI_SCALES.contains(&f.ui_scale) {
                f.ui_scale
            } else {
                100
            },
            toast_at: f.toast_at,
            md_face: f.md_face,
            // A size Kitab doesn't offer (hand-edited, older file) is 100 %.
            md_text: if MD_TEXT_SIZES.contains(&f.md_text_size) {
                f.md_text_size
            } else {
                100
            },
            open_zoom: f.open_zoom,
            new_tab: f.new_tab,
            startup: f.startup,
            system_theme: f.system_theme,
            light_theme: theme_id(f.light_theme, "light"),
            dark_theme: theme_id(f.dark_theme, "dark"),
            code_font: f.code_font.chars().take(200).collect(),
            comic_rtl: f.comic_rtl,
            page_colour: f.page_colour,
            window: f.window,
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
            book_style: kitab_core::BookStyle {
                face: kitab_core::BookFace::Sans,
                line_spacing: Some(1.6),
                margin: Some(2.5),
                ..kitab_core::BookStyle::DEFAULT
            },
            markup: false,
            file_info: false,
            dock_hidden: true,
            file_info_folded: true,
            dock_items: DockItems {
                rotate: false,
                bookmark: true,
                ..DockItems::default()
            },
            status_items: StatusItems {
                zoom: true,
                words: false,
                ..StatusItems::default()
            },
            title_items: TitleItems {
                zoom: true,
                more: false,
                ..TitleItems::default()
            },
            update_check: false,
            skipped_update: "0.2.0".into(),
            scroll_bar: false,
            smooth_scroll: false,
            tab_count: true,
            menu_bar: true,
            read_aloud: ReadAloud {
                voice: "MSTTS_V110_enUS_ZiraM".into(),
                voice_name: "Microsoft Zira".into(),
                rate: 1.5,
                pitch: 0.8,
                follow: false,
            },
            home_layout: HomeLayout::List,
            cover_size: CoverSize::Large,
            home_stats: false,
            home_continue: false,
            home_progress: false,
            ui_scale: 125,
            toast_at: ToastAt::BottomRight,
            md_face: MdFace::Mono,
            md_text: 175,
            open_zoom: OpenZoom::FitPage,
            new_tab: NewTab::OpenFile,
            startup: Startup::LastFile,
            system_theme: true,
            light_theme: "paper".into(),
            dark_theme: "nord".into(),
            code_font: "Cascadia Code".into(),
            comic_rtl: true,
            page_colour: PageColour::Dark,
            window: Some(WindowPlace {
                x: 40.0,
                y: 30.0,
                w: 1200.0,
                h: 800.0,
                maximized: true,
                raw: Some([10, 20, 1210, 820]),
            }),
        };
        assert_eq!(Settings::from_json(&s.to_json()), Ok(s));
    }

    #[test]
    fn book_typography_out_of_range_falls_back() {
        let s = Settings::from_json(
            r#"{"book_face": "comic", "book_line_spacing": 40, "book_margin": -1}"#,
        )
        .expect("parses");
        assert_eq!(s.book_style, kitab_core::BookStyle::DEFAULT);
        let s = Settings::from_json(r#"{"book_face": "serif", "book_line_spacing": 1.5}"#)
            .expect("parses");
        assert_eq!(s.book_style.face, kitab_core::BookFace::Serif);
        assert_eq!(s.book_style.line_spacing, Some(1.5));
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
    fn home_progress_is_on_unless_turned_off() {
        // Settings saved before this field existed keep showing the bars.
        assert!(Settings::from_json("{}").expect("parses").home_progress);
        let s = Settings::from_json(r#"{"home_progress": false}"#).expect("parses");
        assert!(!s.home_progress);
    }

    #[test]
    fn md_text_defaults_to_100_and_falls_back() {
        // Settings saved before this field existed read Markdown as before.
        assert_eq!(Settings::from_json("{}").expect("parses").md_text, 100);
        assert_eq!(Settings::default().md_text, 100);
        let s = Settings::from_json(r#"{"md_text_size": 200}"#).expect("parses");
        assert_eq!(s.md_text, 200);
        let s = Settings::from_json(r#"{"md_text_size": 9000}"#).expect("parses");
        assert_eq!(s.md_text, 100);
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
