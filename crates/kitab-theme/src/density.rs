// SPDX-License-Identifier: AGPL-3.0-or-later

//! Density sizes (DESIGN.md § Density), in logical pixels. Where DESIGN.md
//! gives a range (list rows), the middle of it is used.

/// The three UI densities. Compact is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    Dense,
    #[default]
    Compact,
    Standard,
}

/// Sizes that change with density.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DensityMetrics {
    pub control_height: f32,
    pub icon_button: f32,
    pub list_row: f32,
    pub title_bar: f32,
    pub header_row: f32,
    pub ui_text: f32,
    pub icon_size: f32,
    pub control_radius: f32,
    pub reading_text: f32,
}

impl Density {
    pub const fn metrics(self) -> DensityMetrics {
        match self {
            Density::Dense => DensityMetrics {
                control_height: 26.0,
                icon_button: 26.0,
                list_row: 27.0,
                title_bar: 38.0,
                header_row: 42.0,
                ui_text: 12.5,
                icon_size: 14.0,
                control_radius: 7.0,
                reading_text: 17.0,
            },
            Density::Compact => DensityMetrics {
                control_height: 30.0,
                icon_button: 28.0,
                list_row: 31.0,
                title_bar: 44.0,
                header_row: 50.0,
                ui_text: 13.0,
                icon_size: 15.0,
                control_radius: 8.0,
                reading_text: 18.0,
            },
            Density::Standard => DensityMetrics {
                control_height: 36.0,
                icon_button: 32.0,
                list_row: 37.0,
                title_bar: 50.0,
                header_row: 58.0,
                ui_text: 14.0,
                icon_size: 16.0,
                control_radius: 9.0,
                reading_text: 19.0,
            },
        }
    }
}
