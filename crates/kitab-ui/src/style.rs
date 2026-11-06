// SPDX-License-Identifier: AGPL-3.0-or-later

//! Theme tokens as GPUI values. Views take every colour, shadow and size from
//! here (AGENTS.md rule 16, DESIGN.md).

use gpui::{App, BoxShadow, Global, Hsla, Pixels, point, px};
use kitab_theme::{Density, DensityMetrics, Shadow, Theme};

/// The active look: theme, density and whether Ultra mode is on.
#[derive(Clone)]
pub struct Style {
    pub theme: Theme,
    pub density: Density,
    pub ultra: bool,
}

impl Global for Style {}

impl Style {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            density: Density::Compact,
            ultra: false,
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
