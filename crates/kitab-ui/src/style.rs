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
            ultra: false,
            page_nav: s.page_nav,
            page_corners: s.page_corners,
            animations: s.animations,
            dock: s.dock,
            dock_fade: s.dock_fade,
            tabs: s.tabs,
            minimal_hides: s.minimal,
            minimal: false,
            sidebar_open: s.sidebar,
        };
        style.resolve_accent();
        style
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
