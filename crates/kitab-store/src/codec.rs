// SPDX-License-Identifier: AGPL-3.0-or-later

//! Text encodings for values stored in a single column: formats, zoom modes,
//! locators and tab scroll snapshots. Decoding never panics; anything it does
//! not understand is an `Error::Corrupt` (or `None` where the caller can carry on).
//!
//! Floats use Rust's shortest round-trip formatting, so `decode(encode(v)) == v`.
//! SQLite stores NaN as NULL, so non-finite floats are replaced before writing.

use kitab_core::{Format, Locator, ReadingState, ZoomMode};

use crate::{Error, Result};

pub(crate) fn format_to_str(f: Format) -> &'static str {
    match f {
        Format::Pdf => "pdf",
        Format::Xps => "xps",
        Format::Epub => "epub",
        Format::Mobi => "mobi",
        Format::Fb2 => "fb2",
        Format::Cbz => "cbz",
        Format::Cbr => "cbr",
        Format::Cb7 => "cb7",
        Format::Djvu => "djvu",
        Format::Image => "image",
        Format::Txt => "txt",
        Format::Markdown => "markdown",
    }
}

pub(crate) fn format_from_str(s: &str) -> Result<Format> {
    Ok(match s {
        "pdf" => Format::Pdf,
        "xps" => Format::Xps,
        "epub" => Format::Epub,
        "mobi" => Format::Mobi,
        "fb2" => Format::Fb2,
        "cbz" => Format::Cbz,
        "cbr" => Format::Cbr,
        "cb7" => Format::Cb7,
        "djvu" => Format::Djvu,
        "image" => Format::Image,
        "txt" => Format::Txt,
        "markdown" => Format::Markdown,
        _ => return Err(Error::Corrupt("format")),
    })
}

/// `f` if it is finite, else `fallback`.
pub(crate) fn finite(f: f32, fallback: f32) -> f32 {
    if f.is_finite() { f } else { fallback }
}

/// Progress is a fraction; NaN becomes 0.
pub(crate) fn clamp_progress(p: f32) -> f32 {
    finite(p, 0.0).clamp(0.0, 1.0)
}

/// Zoom mode as (`zoom_mode` column, `zoom` column).
pub(crate) fn zoom_to_columns(z: ZoomMode) -> (&'static str, Option<f64>) {
    match z {
        ZoomMode::FitPage => ("fit_page", None),
        ZoomMode::FitWidth => ("fit_width", None),
        ZoomMode::ActualSize => ("actual", None),
        ZoomMode::Custom(f) => ("custom", Some(f64::from(sanitize_zoom(f)))),
    }
}

pub(crate) fn zoom_from_columns(mode: &str, zoom: Option<f64>) -> Result<ZoomMode> {
    Ok(match mode {
        "fit_page" => ZoomMode::FitPage,
        "fit_width" => ZoomMode::FitWidth,
        "actual" => ZoomMode::ActualSize,
        "custom" => {
            let z = zoom.ok_or(Error::Corrupt("zoom"))?;
            ZoomMode::Custom(sanitize_zoom(z as f32))
        }
        _ => return Err(Error::Corrupt("zoom_mode")),
    })
}

/// A custom zoom must be a positive, finite factor.
fn sanitize_zoom(z: f32) -> f32 {
    if z.is_finite() && z > 0.0 { z } else { 1.0 }
}

/// `fixed:<page>:<x>:<y>`.
pub(crate) fn encode_locator(l: &Locator) -> String {
    match *l {
        Locator::Fixed { page, x, y } => {
            format!("fixed:{page}:{}:{}", finite(x, 0.0), finite(y, 0.0))
        }
    }
}

pub(crate) fn decode_locator(s: &str) -> Result<Locator> {
    let bad = || Error::Corrupt("locator");
    let mut parts = s.split(':');
    match parts.next() {
        Some("fixed") => {
            let page = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            let x: f32 = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            let y: f32 = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
            if parts.next().is_some() {
                return Err(bad());
            }
            Ok(Locator::Fixed {
                page,
                x: finite(x, 0.0),
                y: finite(y, 0.0),
            })
        }
        _ => Err(bad()),
    }
}

fn encode_zoom(z: ZoomMode) -> String {
    match zoom_to_columns(z) {
        (mode, Some(f)) => format!("{mode}={}", f as f32),
        (mode, None) => mode.to_string(),
    }
}

fn decode_zoom(s: &str) -> Result<ZoomMode> {
    match s.split_once('=') {
        Some((mode, f)) => {
            let f: f32 = f.parse().map_err(|_| Error::Corrupt("zoom"))?;
            zoom_from_columns(mode, Some(f64::from(f)))
        }
        None => zoom_from_columns(s, None),
    }
}

/// A whole reading state in one column (tab scroll snapshots):
/// `v1;<locator>;<zoom>;<rotation>;<progress>`.
pub(crate) fn encode_state(s: &ReadingState) -> String {
    format!(
        "v1;{};{};{};{}",
        encode_locator(&s.locator),
        encode_zoom(s.zoom),
        s.rotation,
        clamp_progress(s.progress)
    )
}

pub(crate) fn decode_state(s: &str) -> Result<ReadingState> {
    let bad = || Error::Corrupt("scroll_state");
    let mut parts = s.split(';');
    if parts.next() != Some("v1") {
        return Err(bad());
    }
    let locator = decode_locator(parts.next().ok_or_else(bad)?)?;
    let zoom = decode_zoom(parts.next().ok_or_else(bad)?)?;
    let rotation: u16 = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
    let progress: f32 = parts.next().and_then(|p| p.parse().ok()).ok_or_else(bad)?;
    if parts.next().is_some() {
        return Err(bad());
    }
    Ok(ReadingState {
        locator,
        zoom,
        rotation,
        progress: clamp_progress(progress),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_FORMATS: [Format; 12] = [
        Format::Pdf,
        Format::Xps,
        Format::Epub,
        Format::Mobi,
        Format::Fb2,
        Format::Cbz,
        Format::Cbr,
        Format::Cb7,
        Format::Djvu,
        Format::Image,
        Format::Txt,
        Format::Markdown,
    ];

    #[test]
    fn formats_round_trip() {
        for f in ALL_FORMATS {
            assert_eq!(format_from_str(format_to_str(f)).unwrap(), f);
        }
        assert!(format_from_str("docx").is_err());
    }

    #[test]
    fn states_round_trip() {
        for zoom in [
            ZoomMode::FitPage,
            ZoomMode::FitWidth,
            ZoomMode::ActualSize,
            ZoomMode::Custom(1.337),
            ZoomMode::Custom(0.1),
        ] {
            let s = ReadingState {
                locator: Locator::Fixed {
                    page: 41,
                    x: 12.25,
                    y: 0.1 + 0.2,
                },
                zoom,
                rotation: 270,
                progress: 0.333_333_34,
            };
            assert_eq!(decode_state(&encode_state(&s)).unwrap(), s);
        }
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        for s in [
            "",
            "v1",
            "v2;fixed:1:0:0;fit_page;0;0",
            "v1;fixed:1:0;fit_page;0;0",
            "v1;fixed:-1:0:0;fit_page;0;0",
            "v1;fixed:1:0:0;zoomy;0;0",
            "v1;fixed:1:0:0;custom;0;0",
            "v1;fixed:1:0:0;fit_page;99999;0",
            "v1;fixed:1:0:0;fit_page;0;0;extra",
        ] {
            assert!(decode_state(s).is_err(), "{s:?} should not decode");
        }
    }

    #[test]
    fn non_finite_values_are_replaced() {
        let s = ReadingState {
            locator: Locator::Fixed {
                page: 0,
                x: f32::NAN,
                y: f32::INFINITY,
            },
            zoom: ZoomMode::Custom(f32::NAN),
            rotation: 0,
            progress: f32::NAN,
        };
        let back = decode_state(&encode_state(&s)).unwrap();
        assert_eq!(
            back.locator,
            Locator::Fixed {
                page: 0,
                x: 0.0,
                y: 0.0
            }
        );
        assert_eq!(back.zoom, ZoomMode::Custom(1.0));
        assert_eq!(back.progress, 0.0);
    }
}
