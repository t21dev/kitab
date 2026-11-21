// SPDX-License-Identifier: AGPL-3.0-or-later

//! The interface in other languages (GitHub #12).
//!
//! Text in the code stays in English and is the key: `tr("Open a file…")`
//! shows the chosen language's translation of it, or the English when that
//! language has none yet, so a half-translated language still works.
//! Catalogs are `assets/locales/<code>.json`, one object of English →
//! translation plus `"@language"` (the language's own name). build.rs builds
//! in every file there, so adding a language needs no code change.
//! `tools/check_i18n.py` lists and fills in what each catalog is missing;
//! docs/translating.md walks a translator through it.
//!
//! Text with values in it uses `{name}` places, filled by `trf`, so a
//! translation can put them in its own order.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// A language Kitab's interface can be in: its code (BCP 47) and its name
/// in itself, as the language picker lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
    /// Written right to left (Arabic, Urdu…): `"@direction": "rtl"`.
    pub rtl: bool,
    catalog: &'static str,
}

mod built {
    // `CATALOGS`: every `assets/locales/<code>.json`, found by build.rs.
    include!(concat!(env!("OUT_DIR"), "/locales.rs"));
}

/// A catalog entry whose key starts with this is about the file (its
/// `@language` name, its `@translators`), not text to translate.
const META: char = '@';

/// English first (the code's own text, no catalog), then every catalog in
/// `assets/locales/`, by code. A catalog names its language with
/// `"@language"`; without one, its code shows.
pub fn languages() -> &'static [Language] {
    static ALL: OnceLock<Vec<Language>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = vec![Language {
            code: "en",
            name: "English",
            rtl: false,
            catalog: "",
        }];
        for (code, catalog) in built::CATALOGS {
            let meta: HashMap<String, serde_json::Value> =
                serde_json::from_str(catalog).unwrap_or_default();
            let name = meta
                .get("@language")
                .and_then(|v| v.as_str())
                .filter(|n| !n.trim().is_empty())
                .map_or(*code, |n| Box::leak(n.to_string().into_boxed_str()));
            let rtl = meta.get("@direction").and_then(|v| v.as_str()) == Some("rtl");
            all.push(Language {
                code,
                name,
                rtl,
                catalog,
            });
        }
        all
    })
}

/// The setting's value for "follow the system".
pub const SYSTEM: &str = "system";

type Catalog = HashMap<&'static str, &'static str>;

fn current() -> &'static RwLock<Option<&'static Catalog>> {
    static CURRENT: OnceLock<RwLock<Option<&'static Catalog>>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(None))
}

/// Each catalog parsed once, the first time it's chosen, and kept.
fn catalog(lang: &'static Language) -> Option<&'static Catalog> {
    if lang.catalog.is_empty() {
        return None;
    }
    static PARSED: OnceLock<RwLock<HashMap<&'static str, &'static Catalog>>> = OnceLock::new();
    let parsed = PARSED.get_or_init(Default::default);
    if let Some(c) = parsed.read().ok()?.get(lang.code) {
        return Some(c);
    }
    let c: &'static Catalog = Box::leak(Box::new(parse(lang.catalog, lang.rtl)));
    parsed.write().ok()?.insert(lang.code, c);
    Some(c)
}

/// Right-to-left embedding (U+202B) and its end (U+202C). The isolates
/// (U+2067, U+2069) would be the modern choice, but with them DirectWrite
/// left a label's trailing "PDF" on the wrong side.
const RLE: char = '\u{202B}';
const PDF: char = '\u{202C}';

/// A catalog's entries; a broken file (caught by the tests) counts as empty.
/// Empty translations are left out, so their English shows. In a
/// right-to-left language each translation is an RTL embedding: the interface
/// lays text out left to right, and without it a label's "…", full stop or
/// "Ctrl+G" lands on the wrong side of the Arabic. The marks have no width.
fn parse(json: &'static str, rtl: bool) -> Catalog {
    let map: HashMap<String, serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    map.into_iter()
        .filter(|(k, _)| !k.starts_with(META))
        .filter_map(|(k, v)| Some((k, v.as_str()?.to_string())))
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(k, v)| {
            if rtl {
                (k, format!("{RLE}{v}{PDF}"))
            } else {
                (k, v)
            }
        })
        .map(|(k, v)| -> (&'static str, &'static str) {
            (Box::leak(k.into_boxed_str()), Box::leak(v.into_boxed_str()))
        })
        .collect()
}

/// The language for a setting: a code Kitab has, or for "system" (or a code
/// it doesn't have) the closest to the system's languages, else English.
pub fn resolve(setting: &str) -> &'static Language {
    if setting != SYSTEM
        && let Some(l) = languages().iter().find(|l| l.code == setting)
    {
        return l;
    }
    sys_locale::get_locales()
        .find_map(|tag| best_match(&tag))
        .unwrap_or(&languages()[0])
}

/// The language Kitab has for a system language tag ("zh-Hans-CN",
/// "zh_TW", "en-GB"…), if any.
pub fn best_match(tag: &str) -> Option<&'static Language> {
    let tag = tag.replace('_', "-").to_ascii_lowercase();
    let mut parts = tag.split('-');
    let base = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    if base == "zh" {
        // Traditional for Taiwan, Hong Kong and Macau or a Hant script tag;
        // Simplified otherwise.
        let traditional = rest
            .iter()
            .any(|p| matches!(*p, "hant" | "tw" | "hk" | "mo"))
            && !rest.contains(&"hans");
        let code = if traditional { "zh-TW" } else { "zh-CN" };
        return languages().iter().find(|l| l.code == code);
    }
    // The exact tag ("pt-br"), else the language alone ("es" for "es-MX"),
    // else another region's ("pt-BR" for "pt-PT").
    let all = languages();
    let base_of = |code: &str| code.split('-').next().unwrap_or(code).to_ascii_lowercase();
    all.iter()
        .find(|l| l.code.to_ascii_lowercase() == tag)
        .or_else(|| all.iter().find(|l| l.code.eq_ignore_ascii_case(base)))
        .or_else(|| all.iter().find(|l| base_of(l.code) == base))
}

/// Show the interface in the language `setting` names (see `resolve`).
/// Windows redraw with it on their next frame.
pub fn set_language(setting: &str) {
    let c = catalog(resolve(setting));
    if let Ok(mut cur) = current().write() {
        *cur = c;
    }
}

/// `en` in the interface's language, or `en` itself when it has no
/// translation.
pub fn tr(en: &'static str) -> &'static str {
    current()
        .read()
        .ok()
        .and_then(|c| c.and_then(|c| c.get(en).copied()))
        .unwrap_or(en)
}

/// `tr` for text that isn't `'static` (a title passed down to a helper).
pub fn tr_any(en: &str) -> &str {
    current()
        .read()
        .ok()
        .and_then(|c| c.and_then(|c| c.get(en).copied()))
        .unwrap_or(en)
}

/// `tr`, then each `{name}` filled with its value.
pub fn trf(en: &'static str, values: &[(&str, &str)]) -> String {
    let mut s = tr(en).to_string();
    for (name, value) in values {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that change the interface's language run one at a time: it is
    /// one setting for the whole program.
    fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn every_catalog_parses_and_keeps_its_places() {
        for lang in languages().iter().filter(|l| !l.catalog.is_empty()) {
            let map: HashMap<String, String> =
                serde_json::from_str(lang.catalog).unwrap_or_else(|e| panic!("{}: {e}", lang.code));
            assert!(!map.is_empty(), "{}", lang.code);
            // Each catalog names its language, in that language.
            assert!(
                map.get("@language").is_some_and(|n| !n.trim().is_empty()),
                "{}: add \"@language\"",
                lang.code
            );
            // A translation keeps every {name} its English has, so trf can
            // fill it.
            for (en, t) in &map {
                if t.is_empty() || en.starts_with(META) {
                    continue;
                }
                let places = |s: &str| {
                    let mut v: Vec<String> = s
                        .split('{')
                        .skip(1)
                        .filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string()))
                        .collect();
                    v.sort();
                    v
                };
                assert_eq!(places(en), places(t), "{}: {en:?}", lang.code);
            }
        }
    }

    #[test]
    fn every_catalog_in_the_folder_is_offered() {
        let codes: Vec<&str> = languages().iter().map(|l| l.code).collect();
        assert_eq!(codes[0], "en");
        assert!(
            codes.contains(&"zh-CN") && codes.contains(&"zh-TW"),
            "{codes:?}"
        );
        let zh = languages()
            .iter()
            .find(|l| l.code == "zh-CN")
            .expect("zh-CN");
        assert_eq!(zh.name, "简体中文");
        // A file's own details are never shown as text.
        let _one = one_at_a_time();
        set_language("zh-CN");
        assert_eq!(tr("@language"), "@language");
        set_language("en");
    }

    #[test]
    fn right_to_left_translations_are_embedded() {
        let json = r#"{"@direction": "rtl", "File": "ملف", "Open a file…": "فتح ملف…"}"#;
        let c = parse(json, true);
        assert_eq!(c.get("File").copied(), Some("\u{202B}ملف\u{202C}"));
        assert!(!c.contains_key("@direction"));
        // Left-to-right catalogs are left as written.
        let c = parse(r#"{"File": "Datei"}"#, false);
        assert_eq!(c.get("File").copied(), Some("Datei"));
        // Every catalog that says rtl is offered as rtl.
        for l in languages() {
            if matches!(l.code, "ar" | "ur") {
                assert!(l.rtl, "{} should be right to left", l.code);
            }
        }
    }

    #[test]
    fn regional_tags_find_their_language() {
        let code = |t: &str| best_match(t).map(|l| l.code);
        assert_eq!(code("es-MX"), Some("es"));
        assert_eq!(code("pt-BR"), Some("pt-BR"));
        assert_eq!(code("pt-PT"), Some("pt-BR"));
        assert_eq!(code("bn-IN"), Some("bn"));
        assert_eq!(code("ur-PK"), Some("ur"));
        assert_eq!(code("ar-SA"), Some("ar"));
        assert_eq!(code("sw-KE"), None);
    }

    #[test]
    fn system_tags_find_the_right_chinese() {
        let code = |t: &str| best_match(t).map(|l| l.code);
        assert_eq!(code("zh-CN"), Some("zh-CN"));
        assert_eq!(code("zh-Hans-SG"), Some("zh-CN"));
        assert_eq!(code("zh"), Some("zh-CN"));
        assert_eq!(code("zh_TW"), Some("zh-TW"));
        assert_eq!(code("zh-Hant"), Some("zh-TW"));
        assert_eq!(code("zh-HK"), Some("zh-TW"));
        assert_eq!(code("en-GB"), Some("en"));
        assert_eq!(code("ko-KR"), None);
    }

    #[test]
    fn untranslated_text_shows_in_english_and_places_fill() {
        let _one = one_at_a_time();
        set_language("zh-CN");
        assert_eq!(
            tr("No such text in any catalog"),
            "No such text in any catalog"
        );
        assert_ne!(tr("Settings…"), "Settings…");
        set_language("en");
        assert_eq!(tr("Settings…"), "Settings…");
        assert_eq!(
            trf("Saved a copy {percent} smaller", &[("percent", "40 %")]),
            "Saved a copy 40 % smaller"
        );
    }
}
