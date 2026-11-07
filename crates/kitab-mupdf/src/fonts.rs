// SPDX-License-Identifier: AGPL-3.0-or-later

//! Fallback fonts for text a document doesn't carry fonts for: EPUB, FB2,
//! MOBI and TXT in non-Latin scripts, and PDFs with unembedded CJK fonts.
//!
//! MuPDF's own Noto fonts are left out for size, and font-kit's system lookup
//! took 23 s to lay out a small EPUB (it enumerates every installed font on
//! each request). This loader instead maps a script straight to a font file
//! the system ships: a table on Windows and macOS, one `fc-match` call on
//! Linux. MuPDF caches the font it gets for each script, so each script is
//! looked up at most once per process, on a worker thread (never the UI).

use mupdf::font_loader::{FontHints, FontLoader, set_font_loader};
use mupdf::{CjkFontOrdering, Font};
use mupdf_sys::*;
use std::path::{Path, PathBuf};
use std::sync::Once;

/// Install the loader. Call before the first document opens.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| set_font_loader(SystemFallback));
}

struct SystemFallback;

/// What a document needs: a script, with a language for Han text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Arabic,
    Hebrew,
    Indic,
    Thai,
    Lao,
    Khmer,
    Myanmar,
    Tibetan,
    Mongolian,
    Ethiopic,
    Armenian,
    Georgian,
    /// Han in simplified Chinese, the default for Han.
    ChineseSimplified,
    ChineseTraditional,
    Japanese,
    Korean,
    /// Anything else (symbols, Greek and Cyrillic the base fonts lack).
    General,
}

/// The need for a MuPDF script/language pair (`UCDN_SCRIPT_*`, `FZ_LANG_*`).
pub fn need_for(script: u32, language: fz_text_language) -> Need {
    match script {
        UCDN_SCRIPT_ARABIC | UCDN_SCRIPT_SYRIAC | UCDN_SCRIPT_THAANA => Need::Arabic,
        UCDN_SCRIPT_HEBREW => Need::Hebrew,
        UCDN_SCRIPT_DEVANAGARI
        | UCDN_SCRIPT_BENGALI
        | UCDN_SCRIPT_GURMUKHI
        | UCDN_SCRIPT_GUJARATI
        | UCDN_SCRIPT_ORIYA
        | UCDN_SCRIPT_TAMIL
        | UCDN_SCRIPT_TELUGU
        | UCDN_SCRIPT_KANNADA
        | UCDN_SCRIPT_MALAYALAM
        | UCDN_SCRIPT_SINHALA => Need::Indic,
        UCDN_SCRIPT_THAI => Need::Thai,
        UCDN_SCRIPT_LAO => Need::Lao,
        UCDN_SCRIPT_KHMER => Need::Khmer,
        UCDN_SCRIPT_MYANMAR => Need::Myanmar,
        UCDN_SCRIPT_TIBETAN => Need::Tibetan,
        UCDN_SCRIPT_MONGOLIAN => Need::Mongolian,
        UCDN_SCRIPT_ETHIOPIC => Need::Ethiopic,
        UCDN_SCRIPT_ARMENIAN => Need::Armenian,
        UCDN_SCRIPT_GEORGIAN => Need::Georgian,
        UCDN_SCRIPT_HIRAGANA | UCDN_SCRIPT_KATAKANA => Need::Japanese,
        UCDN_SCRIPT_HANGUL => Need::Korean,
        // MuPDF's language constants are lower case (`FZ_LANG_ja`), so compare
        // rather than match on them.
        UCDN_SCRIPT_HAN | UCDN_SCRIPT_BOPOMOFO => {
            if language == FZ_LANG_ja {
                Need::Japanese
            } else if language == FZ_LANG_ko {
                Need::Korean
            } else if language == FZ_LANG_zh_Hant {
                Need::ChineseTraditional
            } else {
                Need::ChineseSimplified
            }
        }
        _ => Need::General,
    }
}

fn need_for_ordering(ordering: CjkFontOrdering) -> Need {
    match ordering {
        CjkFontOrdering::AdobeCns => Need::ChineseTraditional,
        CjkFontOrdering::AdobeGb => Need::ChineseSimplified,
        CjkFontOrdering::AdobeJapan => Need::Japanese,
        CjkFontOrdering::AdobeKorea => Need::Korean,
    }
}

/// Font files to try for a need, in order: (file name, index in a .ttc).
#[cfg(windows)]
fn candidates(need: Need) -> &'static [(&'static str, i32)] {
    match need {
        Need::Arabic | Need::Hebrew | Need::Armenian | Need::Georgian => {
            &[("segoeui.ttf", 0), ("times.ttf", 0)]
        }
        Need::Indic => &[("Nirmala.ttc", 0), ("Nirmala.ttf", 0)],
        Need::Thai | Need::Lao | Need::Khmer => &[("leelawui.ttf", 0)],
        Need::Myanmar => &[("mmrtext.ttf", 0)],
        Need::Tibetan => &[("himalaya.ttf", 0)],
        Need::Mongolian => &[("monbaiti.ttf", 0)],
        Need::Ethiopic => &[("ebrima.ttf", 0)],
        Need::ChineseSimplified => &[("msyh.ttc", 0), ("simsun.ttc", 0)],
        Need::ChineseTraditional => &[("msjh.ttc", 0), ("mingliu.ttc", 0)],
        Need::Japanese => &[("YuGothR.ttc", 0), ("msgothic.ttc", 0)],
        Need::Korean => &[("malgun.ttf", 0)],
        Need::General => &[("seguisym.ttf", 0), ("segoeui.ttf", 0)],
    }
}

#[cfg(windows)]
fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(win) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(win).join("Fonts"));
    }
    // Fonts installed for this user only.
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join(r"Microsoft\Windows\Fonts"));
    }
    dirs
}

#[cfg(target_os = "macos")]
fn candidates(need: Need) -> &'static [(&'static str, i32)] {
    match need {
        Need::Arabic => &[("GeezaPro.ttc", 0)],
        Need::Hebrew => &[("ArialHB.ttc", 0)],
        Need::Indic => &[("KohinoorBangla.ttc", 0), ("Kohinoor.ttc", 0)],
        Need::Thai | Need::Lao | Need::Khmer | Need::Myanmar => &[("Thonburi.ttc", 0)],
        Need::ChineseSimplified | Need::ChineseTraditional => {
            &[("PingFang.ttc", 0), ("STHeiti Light.ttc", 0)]
        }
        Need::Japanese => &[("ヒラギノ角ゴシック W3.ttc", 0)],
        Need::Korean => &[("AppleSDGothicNeo.ttc", 0)],
        _ => &[("Apple Symbols.ttf", 0), ("Helvetica.ttc", 0)],
    }
}

#[cfg(target_os = "macos")]
fn font_dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/Library/Fonts"),
    ]
}

/// The file for a need: the first candidate that exists.
#[cfg(any(windows, target_os = "macos"))]
pub fn file_for(need: Need) -> Option<(PathBuf, i32)> {
    let dirs = font_dirs();
    candidates(need).iter().find_map(|(name, index)| {
        dirs.iter()
            .map(|d| d.join(name))
            .find(|p| p.is_file())
            .map(|p| (p, *index))
    })
}

/// Linux and the BSDs: ask fontconfig once for a language's best font.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn file_for(need: Need) -> Option<(PathBuf, i32)> {
    let lang = match need {
        Need::Arabic => "ar",
        Need::Hebrew => "he",
        Need::Indic => "bn",
        Need::Thai => "th",
        Need::Lao => "lo",
        Need::Khmer => "km",
        Need::Myanmar => "my",
        Need::Tibetan => "bo",
        Need::Mongolian => "mn",
        Need::Ethiopic => "am",
        Need::Armenian => "hy",
        Need::Georgian => "ka",
        Need::ChineseSimplified => "zh-cn",
        Need::ChineseTraditional => "zh-tw",
        Need::Japanese => "ja",
        Need::Korean => "ko",
        Need::General => "en",
    };
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", &format!(":lang={lang}")])
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines();
    let file = PathBuf::from(lines.next()?);
    let index = lines
        .next()
        .and_then(|i| i.trim().parse().ok())
        .unwrap_or(0);
    file.is_file().then_some((file, index))
}

/// The most a fallback font file may be (the big CJK collections are ~20 MB).
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

fn load(path: &Path, index: i32) -> Option<Font> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > MAX_FONT_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let name = path.file_stem()?.to_string_lossy().to_string();
    Font::from_bytes_with_index(&name, index, &bytes).ok()
}

impl FontLoader for SystemFallback {
    fn load_cjk_font(&self, _name: &str, ordering: CjkFontOrdering, _serif: bool) -> Option<Font> {
        let (path, index) = file_for(need_for_ordering(ordering))?;
        load(&path, index)
    }

    fn load_fallback_font(&self, script: u32, language: u32, _hints: FontHints) -> Option<Font> {
        // Latin, Greek and Cyrillic are in MuPDF's built-in fonts.
        if matches!(
            script,
            UCDN_SCRIPT_LATIN | UCDN_SCRIPT_GREEK | UCDN_SCRIPT_CYRILLIC
        ) {
            return None;
        }
        let (path, index) = file_for(need_for(script, language as fz_text_language))?;
        load(&path, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_map_to_needs() {
        assert_eq!(need_for(UCDN_SCRIPT_BENGALI, 0), Need::Indic);
        assert_eq!(need_for(UCDN_SCRIPT_ARABIC, 0), Need::Arabic);
        assert_eq!(need_for(UCDN_SCRIPT_HAN, FZ_LANG_ja), Need::Japanese);
        assert_eq!(
            need_for(UCDN_SCRIPT_HAN, FZ_LANG_zh_Hant),
            Need::ChineseTraditional
        );
        assert_eq!(need_for(UCDN_SCRIPT_HAN, 0), Need::ChineseSimplified);
        assert_eq!(need_for(UCDN_SCRIPT_HANGUL, 0), Need::Korean);
    }

    /// On Windows every need the fixtures use has a font file that loads.
    #[cfg(windows)]
    #[test]
    fn windows_has_fonts_for_the_fixture_scripts() {
        for need in [
            Need::Arabic,
            Need::Hebrew,
            Need::Indic,
            Need::ChineseSimplified,
            Need::Japanese,
            Need::Korean,
        ] {
            let (path, index) = file_for(need).unwrap_or_else(|| panic!("{need:?}: no font file"));
            assert!(
                load(&path, index).is_some(),
                "{need:?}: {path:?} does not load"
            );
        }
    }
}
