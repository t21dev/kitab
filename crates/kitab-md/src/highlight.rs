// SPDX-License-Identifier: AGPL-3.0-or-later

//! Syntax highlighting for code blocks (PLAN.md §3.4), with syntect's built-in
//! grammars. Scopes are reduced to a handful of kinds the UI colours from the
//! theme's tokens, so code follows every theme and the accent. Runs while the
//! document is parsed, off the UI thread. The grammars load once, on the first
//! code block (about 20 ms), never at start-up.

use crate::model::{Token, TokenKind};
use std::sync::OnceLock;
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

/// Larger blocks stay plain (a generated file pasted into a README).
const MAX_BYTES: usize = 256 * 1024;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Fence names syntect knows under another name.
fn alias(lang: &str) -> &str {
    match lang {
        "ts" | "tsx" | "typescript" | "jsx" | "mjs" | "cjs" | "node" => "js",
        "sh" | "shell" | "zsh" | "console" => "bash",
        "py" | "python3" => "python",
        "rs" => "rust",
        "yml" => "yaml",
        "c++" | "cxx" | "hpp" => "cpp",
        "cs" | "csharp" => "c#",
        "golang" => "go",
        "kt" | "kotlin" => "java",
        "ps1" | "powershell" | "pwsh" => "bash",
        "toml" | "ini" | "cfg" => "properties",
        "jsonc" | "json5" => "json",
        "html" | "htm" | "xhtml" | "svelte" | "vue" => "html",
        "dockerfile" => "bash",
        other => other,
    }
}

fn find(lang: &str) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let lang = lang.trim().to_ascii_lowercase();
    if lang.is_empty() || lang == "text" || lang == "plain" || lang == "txt" {
        return None;
    }
    let name = alias(&lang);
    set.find_syntax_by_token(name)
        .or_else(|| set.find_syntax_by_extension(name))
}

fn classify(stack: &ScopeStack) -> TokenKind {
    // Inside a comment or a string everything is that (the `//` of a comment
    // is "punctuation" one level deeper).
    for scope in stack.as_slice() {
        let s = scope.build_string();
        if s.starts_with("comment") {
            return TokenKind::Comment;
        }
        if s.starts_with("string") {
            return TokenKind::String;
        }
    }
    for scope in stack.as_slice().iter().rev() {
        let s = scope.build_string();
        let has = |p: &str| s.starts_with(p);
        let kind = if has("comment") {
            TokenKind::Comment
        } else if has("string") || has("markup.inline.raw") {
            TokenKind::String
        } else if has("constant.numeric") || has("constant.language") || has("constant.character") {
            TokenKind::Number
        } else if has("keyword.operator") || has("punctuation") {
            TokenKind::Punctuation
        } else if has("keyword")
            || has("storage.modifier")
            || has("storage.type") && !has("storage.type.primitive")
        {
            TokenKind::Keyword
        } else if has("entity.name.function")
            || has("support.function")
            || has("meta.function-call")
        {
            TokenKind::Function
        } else if has("entity.name.type")
            || has("entity.name.class")
            || has("support.type")
            || has("support.class")
            || has("storage.type.primitive")
            || has("entity.other.inherited-class")
        {
            TokenKind::Type
        } else if has("entity.name.tag")
            || has("entity.other.attribute-name")
            || has("variable.parameter")
        {
            TokenKind::Tag
        } else if has("constant") || has("variable.language") {
            TokenKind::Number
        } else {
            continue;
        };
        return kind;
    }
    TokenKind::Plain
}

/// Tokens for `text` in `lang`, or an empty list when the language is unknown
/// (the block shows in one colour).
pub fn highlight(lang: &str, text: &str) -> Vec<Token> {
    if text.len() > MAX_BYTES {
        return Vec::new();
    }
    let Some(syntax) = find(lang) else {
        return Vec::new();
    };
    let set = syntaxes();
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out: Vec<Token> = Vec::new();
    let push = |start: usize, end: usize, kind: TokenKind, out: &mut Vec<Token>| {
        if start >= end {
            return;
        }
        if let Some(last) = out.last_mut()
            && last.kind == kind
            && last.end == start
        {
            last.end = end;
            return;
        }
        out.push(Token { start, end, kind });
    };
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        // A grammar that fails on a line leaves the rest of the block plain.
        let Ok(ops) = state.parse_line(line, set) else {
            return out;
        };
        let mut last = 0;
        for (pos, op) in ops {
            let pos = pos.min(line.len());
            push(offset + last, offset + pos, classify(&stack), &mut out);
            if stack.apply(&op).is_err() {
                return out;
            }
            last = pos;
        }
        push(
            offset + last,
            offset + line.len(),
            classify(&stack),
            &mut out,
        );
        offset += line.len();
    }
    out.retain(|t| t.kind != TokenKind::Plain);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_keywords_strings_and_comments() {
        let src = "// hi\nfn main() { let s = \"x\"; }";
        let t = highlight("rust", src);
        let kind_of = |needle: &str| {
            let at = src.find(needle).unwrap_or(0);
            t.iter()
                .find(|t| t.start <= at && at < t.end)
                .map(|t| t.kind)
        };
        assert_eq!(kind_of("// hi"), Some(TokenKind::Comment));
        assert_eq!(kind_of("fn"), Some(TokenKind::Keyword));
        assert_eq!(kind_of("\"x\""), Some(TokenKind::String));
    }

    #[test]
    fn unknown_languages_stay_plain() {
        assert!(highlight("no-such-language", "x = 1").is_empty());
        assert!(highlight("", "x = 1").is_empty());
        // Aliases reach a grammar.
        assert!(!highlight("ts", "const a = 'x';").is_empty());
    }
}
