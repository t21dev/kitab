// SPDX-License-Identifier: AGPL-3.0-or-later

//! Theme tokens as GPUI values. Views take every colour, shadow and size from
//! here (AGENTS.md rule 16, DESIGN.md).

use crate::settings::{DockAt, MinimalHides, PageCorners, PageNav, Settings, TabsAt};
use gpui::{App, BoxShadow, Global, Hsla, Pixels, point, px};
use kitab_theme::{Accent, Density, DensityMetrics, Shadow, Theme};

/// The active look: theme, density, Ultra mode and the reading-view choices
/// from Settings.
#[derive(Clone)]
pub struct Style {
    /// The theme in use, with the chosen accent applied. Change it with
    /// [`Style::set_theme`] and [`Style::set_accent`].
    pub theme: Theme,
    /// The theme as its file defines it (the accent's "Theme" preset).
    base_theme: Theme,
    pub accent: Accent,
    pub density: Density,
    pub ultra: bool,
    pub page_nav: PageNav,
    pub page_corners: PageCorners,
    pub animations: bool,
    pub dock: DockAt,
    pub dock_fade: bool,
    pub tabs: TabsAt,
    pub minimal_hides: MinimalHides,
    /// Minimal mode is on (F11). For this session only; not saved.
    pub minimal: bool,
    pub sidebar_open: bool,
    pub dock_compact: bool,
    pub code_theme: kitab_theme::CodeTheme,
    /// Text size of books in points.
    pub book_text: f32,
    pub book_style: kitab_core::BookStyle,
    /// Drawing and markup tools are on.
    pub markup: bool,
    /// The file info chip shows.
    pub file_info: bool,
    /// The dock is tucked into its handle.
    pub dock_hidden: bool,
    /// The file info chip is folded to its "i" button.
    pub file_info_folded: bool,
    /// The quick dock's buttons.
    pub dock_items: crate::settings::DockItems,
    pub status_items: crate::settings::StatusItems,
    pub title_items: crate::settings::TitleItems,
    /// Check for updates automatically (opt-in).
    pub update_check: bool,
    /// The release put off with Later.
    pub skipped_update: String,
    /// The scroll bar on the right of the pages.
    pub scroll_bar: bool,
    pub smooth_scroll: bool,
    /// The open-tab count in the title bar.
    pub tab_count: bool,
    pub menu_bar: bool,
    pub read_aloud: crate::settings::ReadAloud,
    /// Home's layout (Settings › Home).
    pub home_layout: crate::settings::HomeLayout,
    pub cover_size: crate::settings::CoverSize,
    pub home_stats: bool,
    pub home_continue: bool,
    /// Reading progress bars on Home (Settings › Home).
    pub home_progress: bool,
    /// Interface size in percent (Settings › Appearance).
    pub ui_scale: u16,
    pub toast_at: crate::settings::ToastAt,
    pub md_face: crate::settings::MdFace,
    /// Markdown text size in percent (Settings › Reading).
    pub md_text: u16,
    pub open_zoom: crate::settings::OpenZoom,
    pub new_tab: crate::settings::NewTab,
    pub startup: crate::settings::Startup,
    pub system_theme: bool,
    pub light_theme: String,
    pub dark_theme: String,
    pub code_font: String,
    pub comic_rtl: bool,
    pub page_colour: crate::settings::PageColour,
    /// The main window's last place, saved with the settings.
    pub window: Option<crate::settings::WindowPlace>,
}

impl Global for Style {}

impl Style {
    pub fn new(theme: Theme) -> Self {
        Self::from_settings(theme, &Settings::default())
    }

    /// `theme` is the loaded theme `settings.theme` names.
    pub fn from_settings(theme: Theme, s: &Settings) -> Self {
        let mut style = Self {
            theme: theme.clone(),
            base_theme: theme,
            accent: s.accent,
            density: s.density,
            ultra: s.flat,
            page_nav: s.page_nav,
            page_corners: s.page_corners,
            animations: s.animations,
            dock: s.dock,
            dock_fade: s.dock_fade,
            tabs: s.tabs,
            minimal_hides: s.minimal,
            minimal: false,
            sidebar_open: s.sidebar,
            dock_compact: s.dock_compact,
            code_theme: s.code_theme,
            book_text: s.book_text,
            book_style: s.book_style,
            markup: s.markup,
            file_info: s.file_info,
            dock_hidden: s.dock_hidden,
            file_info_folded: s.file_info_folded,
            dock_items: s.dock_items,
            status_items: s.status_items,
            title_items: s.title_items,
            update_check: s.update_check,
            skipped_update: s.skipped_update.clone(),
            scroll_bar: s.scroll_bar,
            smooth_scroll: s.smooth_scroll,
            tab_count: s.tab_count,
            menu_bar: s.menu_bar,
            read_aloud: s.read_aloud.clone(),
            home_layout: s.home_layout,
            cover_size: s.cover_size,
            home_stats: s.home_stats,
            home_continue: s.home_continue,
            home_progress: s.home_progress,
            ui_scale: s.ui_scale,
            toast_at: s.toast_at,
            md_face: s.md_face,
            md_text: s.md_text,
            open_zoom: s.open_zoom,
            new_tab: s.new_tab,
            startup: s.startup,
            system_theme: s.system_theme,
            light_theme: s.light_theme.clone(),
            dark_theme: s.dark_theme.clone(),
            code_font: s.code_font.clone(),
            comic_rtl: s.comic_rtl,
            page_colour: s.page_colour,
            window: s.window,
        };
        style.resolve_accent();
        style
    }

    /// What a control shows under the pointer: a veil of the text colour,
    /// visible on any surface (the `hover` token is a fill, too close to
    /// the background in dark themes to read as feedback).
    pub fn hover_bg(&self) -> Hsla {
        let light = self.theme.appearance == kitab_theme::Appearance::Light;
        self.ca(|c| c.text, if light { 0.06 } else { 0.08 })
    }

    /// The hover on something raised that casts a shadow: solid, so the
    /// shadow beneath doesn't show through as a grey glow.
    pub fn raised_hover(&self) -> Hsla {
        self.c(|c| c.raised).blend(self.hover_bg())
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.base_theme = theme;
        self.resolve_accent();
    }

    pub fn set_accent(&mut self, accent: Accent) {
        self.accent = accent;
        self.resolve_accent();
    }

    /// The theme as its file defines it, before the accent.
    pub fn base_theme(&self) -> &Theme {
        &self.base_theme
    }

    fn resolve_accent(&mut self) {
        let mut theme = self.base_theme.clone();
        theme.colors.accent = self.accent.resolve(&self.base_theme);
        self.theme = theme;
    }

    /// The settings this look comes from (what `settings.json` should hold).
    pub fn settings(&self) -> Settings {
        Settings {
            theme: self.theme.id.clone(),
            density: self.density,
            accent: self.accent,
            page_nav: self.page_nav,
            page_corners: self.page_corners,
            animations: self.animations,
            dock: self.dock,
            dock_fade: self.dock_fade,
            tabs: self.tabs,
            minimal: self.minimal_hides,
            sidebar: self.sidebar_open,
            flat: self.ultra,
            dock_compact: self.dock_compact,
            code_theme: self.code_theme,
            book_text: self.book_text,
            book_style: self.book_style,
            markup: self.markup,
            file_info: self.file_info,
            dock_hidden: self.dock_hidden,
            file_info_folded: self.file_info_folded,
            dock_items: self.dock_items,
            status_items: self.status_items,
            title_items: self.title_items,
            update_check: self.update_check,
            skipped_update: self.skipped_update.clone(),
            scroll_bar: self.scroll_bar,
            smooth_scroll: self.smooth_scroll,
            tab_count: self.tab_count,
            menu_bar: self.menu_bar,
            read_aloud: self.read_aloud.clone(),
            home_layout: self.home_layout,
            cover_size: self.cover_size,
            home_stats: self.home_stats,
            home_continue: self.home_continue,
            home_progress: self.home_progress,
            ui_scale: self.ui_scale,
            toast_at: self.toast_at,
            md_face: self.md_face,
            md_text: self.md_text,
            open_zoom: self.open_zoom,
            new_tab: self.new_tab,
            startup: self.startup,
            system_theme: self.system_theme,
            light_theme: self.light_theme.clone(),
            dark_theme: self.dark_theme.clone(),
            code_font: self.code_font.clone(),
            comic_rtl: self.comic_rtl,
            page_colour: self.page_colour,
            window: self.window,
        }
    }

    /// Whether the tabs, sidebar, dock or page navigator show right now.
    pub fn shows_tabs(&self) -> bool {
        !(self.minimal && self.minimal_hides.tabs)
    }
    pub fn shows_sidebar(&self) -> bool {
        !(self.minimal && self.minimal_hides.sidebar)
    }
    pub fn shows_dock(&self) -> bool {
        !(self.minimal && self.minimal_hides.dock)
    }
    pub fn shows_page_nav(&self) -> bool {
        !(self.minimal && self.minimal_hides.page_navigator)
    }
    /// Room above the first page for the floating window buttons.
    pub fn top_inset(&self) -> f32 {
        if self.tabs == TabsAt::Sidebar && !self.minimal {
            crate::reader::TOP_INSET
        } else {
            0.0
        }
    }

    /// The code palette for this theme, or `None` to use the theme's tokens.
    pub fn code_palette(&self) -> Option<kitab_theme::CodePalette> {
        let dark = self.theme.appearance != kitab_theme::Appearance::Light;
        self.code_theme.palette(dark)
    }

    /// Apply a layout preset to everything it covers.
    pub fn apply_layout(&mut self, layout: crate::settings::Layout) {
        let mut s = self.settings();
        s.apply_layout(layout);
        self.animations = s.animations;
        self.ultra = s.flat;
        self.dock_compact = s.dock_compact;
        self.page_nav = s.page_nav;
        self.dock_fade = s.dock_fade;
        self.sidebar_open = s.sidebar;
    }

    pub fn get(cx: &App) -> &Style {
        cx.global::<Style>()
    }

    pub fn m(&self) -> DensityMetrics {
        self.density.metrics()
    }

    pub fn px(&self, f: impl Fn(&DensityMetrics) -> f32) -> Pixels {
        px(f(&self.m()))
    }

    /// A colour token by field.
    pub fn c(&self, f: impl Fn(&kitab_theme::Colors) -> kitab_theme::Rgba) -> Hsla {
        hsla(f(&self.theme.colors))
    }

    /// A colour token with its alpha multiplied (hover washes, scrims).
    pub fn ca(&self, f: impl Fn(&kitab_theme::Colors) -> kitab_theme::Rgba, alpha: f32) -> Hsla {
        let mut c = hsla(f(&self.theme.colors));
        c.a *= alpha;
        c
    }

    fn stack(&self, f: impl Fn(&kitab_theme::Depth) -> &Vec<Shadow>) -> Vec<BoxShadow> {
        let depth = if self.ultra {
            self.theme.depth.ultra()
        } else {
            self.theme.depth.clone()
        };
        // GPUI has no inset shadows; DESIGN.md's inset highlight layers are left out.
        f(&depth)
            .iter()
            .filter(|s| !s.inset)
            .map(|s| BoxShadow {
                color: hsla(s.color),
                offset: point(px(s.x), px(s.y)),
                blur_radius: px(s.blur),
                spread_radius: px(s.spread),
            })
            .collect()
    }

    pub fn elev(&self) -> Vec<BoxShadow> {
        self.stack(|d| &d.elev)
    }
    pub fn float(&self) -> Vec<BoxShadow> {
        self.stack(|d| &d.float)
    }
    pub fn modal(&self) -> Vec<BoxShadow> {
        self.stack(|d| &d.modal)
    }
    pub fn sheet(&self) -> Vec<BoxShadow> {
        self.stack(|d| &d.sheet)
    }
    pub fn well(&self) -> Vec<BoxShadow> {
        self.stack(|d| &d.well)
    }
}

pub fn hsla(c: kitab_theme::Rgba) -> Hsla {
    gpui::Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
    .into()
}
