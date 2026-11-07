// SPDX-License-Identifier: AGPL-3.0-or-later

//! Design tokens for Kitab Reader: colours, depth, density and the accent
//! maths. DESIGN.md is the specification; `assets/themes/*.json` holds the
//! values. Views take every colour, shadow and size from here (AGENTS.md
//! rule 16).

mod accent;
mod code;
mod color;
mod density;
mod theme;
mod thumb;

pub use accent::{Accent, AccentPreset, hue_swatch};
pub use code::{CodePalette, CodeTheme};
pub use color::{Rgba, accent_for_contrast, contrast_ratio};
pub use density::{Density, DensityMetrics};
pub use theme::{
    Appearance, BUILTIN_IDS, Colors, Depth, Shadow, Theme, ThemeError, builtin, builtin_all,
};
pub use thumb::{ThumbPalette, thumb_palette};
