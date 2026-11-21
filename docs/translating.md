# Translating Kitab

Kitab's interface can be shown in other languages (GitHub #12). Settings › Appearance › Language picks one, and **System** (the default) follows the computer's language.

You don't need to know Rust to translate. Each language is one JSON file, and a new file in `assets/locales/` shows up in the app with no code to change.

## Languages

| Code | Language | Status |
|---|---|---|
| `en` | English | Built in: the English in the code |
| `es` | Español (Spanish) | Draft, needs a native speaker's review |
| `pt-BR` | Português (Brasil) | Draft, needs review. Also chosen for other Portuguese |
| `fr` | Français (French) | Draft, needs review |
| `de` | Deutsch (German) | Draft, needs review |
| `bn` | বাংলা (Bangla) | Draft, needs review |
| `hi` | हिन्दी (Hindi) | Draft, needs review |
| `ja` | 日本語 (Japanese) | Draft, needs review |
| `zh-CN` | 简体中文 (Simplified Chinese) | Draft, needs review. Settings group titles still blank |
| `zh-TW` | 繁體中文 (Traditional Chinese) | Draft, needs review. Settings group titles still blank |
| `ar` | العربية (Arabic) | Draft, needs review. Right to left (see below) |
| `ur` | اردو (Urdu) | Draft, needs review. Right to left (see below) |

Run `python tools/check_i18n.py` for up-to-date numbers.

## How a catalog looks

```json
{
  "@language": "简体中文",
  "@translators": "Your name",
  "Open a file…": "打开文件…",
  "Zoom in": "放大",
  "Show or hide the status bar": ""
}
```

- **The key is the English** exactly as Kitab shows it, including `…` and capitals. Never change a key; change only the value.
- **A blank value shows the English.** A language works while it is only partly done, so you can translate a few texts at a time.
- **`@` keys describe the file.** `"@language"` is the language's name written in that language (it appears in the language picker), `"@translators"` credits you, and `"@direction": "rtl"` marks a language written right to left. They are never shown as text.
- **Placeholders** such as `{percent}` must stay in the translation, but you can move them: `"Saved a copy {percent} smaller"` → `"副本缩小了 {percent}"`. The tests check this.
- **Keep it short and plain.** Menus and buttons have little room. Match the tone of the English: short sentences, sentence case, no exclamation marks (see DESIGN.md, Plain copy).

## Finish a language Kitab has

```
python tools/check_i18n.py --fill zh-CN    # adds every missing text to zh-CN.json, blank
```

Translate the blank values and open a pull request. `python tools/check_i18n.py zh-CN` lists what is still missing.

## Start a new language

```
python tools/check_i18n.py --new ja "日本語"
```

This writes `assets/locales/ja.json` with every text Kitab shows, blank. Use a [BCP 47](https://www.w3.org/International/articles/language-tags/) code: `ja`, `de`, `bn`, `pt-BR`. Translate what you can and open a pull request. Kitab picks the file up on the next build, and **System** chooses it for people whose computer uses that language.

If the system names your language with tags other than the file's code (Chinese, for example, arrives as `zh-Hans-CN` or `zh-HK`), say so in the pull request. `best_match` in `crates/kitab-ui/src/i18n.rs` then needs a line and a test.

## Right-to-left languages

Arabic, Urdu, Persian, Hebrew and other right-to-left languages work like the rest, with `"@direction": "rtl"` in the file (`--new` adds it for them). Write the translation in its normal order, and don't add direction marks: Kitab shapes the script, reads each label right to left, and keeps "…", full stops, "Ctrl+G" and names like PDF on the correct side.

The window itself keeps its left-to-right arrangement for now: the sidebar on the left, menus opening to the right. Mirroring the whole interface is planned separately.

## Not using Git?

Open a [Translation issue](https://github.com/t21dev/kitab/issues/new?template=translation.yml) with your translations, one `English → translation` per line, or attach the JSON file.

## Seeing your translation

Build Kitab (README › Build from source), open Settings › Appearance › Language and pick it. Before sending, run:

```
python tools/check_i18n.py --strict    # what CI runs: catalogs and texts agree
cargo test -p kitab-ui i18n            # catalogs parse, placeholders kept
```

## For developers: making text translatable

- Wrap text the user sees in `tr("…")` (`crate::i18n::tr`), or `trf("… {name} …", &[("name", value)])` when it has values. Never build a sentence from pieces: word order differs between languages.
- Menu bar rows, command palette entries, Settings section names and Settings group titles and notes go through `tr` already. Add new ones as usual and they are picked up.
- After changing UI text, run `python tools/check_i18n.py`. Texts whose English changed show up as "unused" in each catalog. Move their translations to the new English, or delete them.
- Scripts that Inter doesn't cover (Chinese, Japanese, Korean, Arabic, Bangla and others) use the system's UI font (DESIGN.md, Typography), so no font work is needed.
