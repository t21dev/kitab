// SPDX-License-Identifier: AGPL-3.0-or-later

//! Every shortcut, grouped, for Settings › Shortcuts and the cheat sheet
//! (Ctrl+/). Keep in step with `actions::bind_default_keys` and PLAN.md §3.10;
//! a test checks the bound keys are all listed.

pub struct Group {
    pub title: &'static str,
    /// (what it does, keys). Alternatives are separated by " / ".
    pub rows: &'static [(&'static str, &'static str)],
}

pub const GROUPS: &[Group] = &[
    Group {
        title: "Files and tabs",
        rows: &[
            ("Open a file", "Ctrl+O"),
            ("Close tab", "Ctrl+W / Ctrl+F4"),
            ("Reopen closed tab", "Ctrl+Shift+T"),
            ("Next tab", "Ctrl+Tab / Ctrl+PgDn"),
            ("Previous tab", "Ctrl+Shift+Tab / Ctrl+PgUp"),
            ("Go to tab 1–8", "Alt+1–8"),
            ("Go to the last tab", "Alt+9"),
            ("Home", "Ctrl+Shift+H"),
        ],
    },
    Group {
        title: "Navigate",
        rows: &[
            ("Next page", "Right / N"),
            ("Previous page", "Left / P"),
            ("First page", "Home"),
            ("Last page", "End"),
            ("Scroll", "Down / Up / J / K"),
            ("Screen down", "PgDn / Space"),
            ("Screen up", "PgUp / Shift+Space"),
            ("Go to page", "Ctrl+G"),
            ("Back", "Alt+Left"),
            ("Forward", "Alt+Right"),
            ("Contents", "F12 / Ctrl+B"),
        ],
    },
    Group {
        title: "Find",
        rows: &[
            ("Find in document", "Ctrl+F / /"),
            ("Next result", "F3 / Enter"),
            ("Previous result", "Shift+F3 / Shift+Enter"),
            ("Match case", "Alt+C"),
            ("Whole word", "Alt+W"),
            ("Close find", "Esc"),
        ],
    },
    Group {
        title: "Markdown",
        rows: &[
            ("Edit beside the preview", "Ctrl+E"),
            ("Save (while editing)", "Ctrl+S"),
            ("Undo / redo (while editing)", "Ctrl+Z / Ctrl+Y"),
        ],
    },
    Group {
        title: "Select and copy",
        rows: &[
            ("Select text", "Drag on the page"),
            ("Select a word / line", "Double-click / triple-click"),
            ("Extend the selection", "Shift+click"),
            ("Copy the selection", "Ctrl+C"),
            ("Clear the selection", "Esc"),
        ],
    },
    Group {
        title: "Zoom and view",
        rows: &[
            ("Zoom in", "Ctrl+= / Ctrl+wheel"),
            ("Zoom out", "Ctrl+- / Ctrl+wheel"),
            ("Fit page", "Ctrl+0"),
            ("Actual size", "Ctrl+1"),
            ("Fit width", "Ctrl+2"),
            ("Rotate clockwise", "Ctrl+Shift+="),
            ("Rotate counter-clockwise", "Ctrl+Shift+-"),
            ("Next theme", "Ctrl+Shift+L"),
            ("Minimal mode", "F11"),
        ],
    },
    Group {
        title: "PDF tools",
        rows: &[
            ("Save pages as a new PDF", "Ctrl+Shift+E"),
            ("Split view with the next tab", "Ctrl+\\"),
        ],
    },
    Group {
        title: "App",
        rows: &[
            ("Settings", "Ctrl+,"),
            ("Keyboard shortcuts", "Ctrl+/"),
            ("Close a menu or dialog", "Esc"),
        ],
    },
    Group {
        title: "Mouse",
        rows: &[
            ("Close a tab", "Middle-click"),
            (
                "Menu for a page, tab, card or contents entry",
                "Right-click",
            ),
            ("Back / forward", "Mouse back / forward"),
            ("Jump or scrub", "Click / drag the scrubber"),
            ("Type a page number", "Click the page number"),
        ],
    },
];

/// The keys of one alternative as separate caps: "Ctrl+Shift+T" → [Ctrl, Shift, T].
pub fn caps(keys: &str) -> Vec<&str> {
    if keys.contains('+') && !keys.ends_with("++") && keys != "+" {
        keys.split('+').collect()
    } else {
        vec![keys]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_split_on_plus() {
        assert_eq!(caps("Ctrl+Shift+T"), vec!["Ctrl", "Shift", "T"]);
        assert_eq!(caps("F12"), vec!["F12"]);
        assert_eq!(caps("Ctrl+="), vec!["Ctrl", "="]);
    }

    /// Every default key binding appears in the list (AGENTS.md rule 19).
    #[test]
    fn every_bound_key_is_listed() {
        let listed: String = GROUPS
            .iter()
            .flat_map(|g| g.rows.iter().map(|(_, k)| k.to_lowercase()))
            .collect::<Vec<_>>()
            .join(" / ");
        let pretty = |b: &str| {
            let (body, minus) = match b.strip_suffix("--") {
                Some(body) => (body, true),
                None => (b, false),
            };
            let mut parts: Vec<String> = body
                .split('-')
                .filter(|p| !p.is_empty())
                .map(|p| match p {
                    "pagedown" => "pgdn".to_string(),
                    "pageup" => "pgup".to_string(),
                    other => other.to_string(),
                })
                .collect();
            if minus {
                parts.push("-".into());
            }
            parts.join("+")
        };
        let source = include_str!("actions.rs");
        let mut checked = 0;
        for chunk in source.split("KeyBinding::new(\"").skip(1) {
            let Some(binding) = chunk.split('"').next() else {
                continue;
            };
            // The source escapes a backslash (`ctrl-\\`).
            let binding = binding.replace(r"\\", r"\");
            let binding = binding.as_str();
            // Field keys, an alias of Ctrl+=, and Alt+2..8 (listed as the range Alt+1–8).
            let skip = matches!(binding, "enter" | "escape" | "ctrl-+")
                || binding
                    .strip_prefix("alt-")
                    .is_some_and(|d| ("2"..="8").contains(&d));
            if skip {
                continue;
            }
            let want = pretty(binding);
            assert!(
                listed.contains(&want),
                "{binding} ({want}) is bound but not listed"
            );
            checked += 1;
        }
        assert!(checked > 30, "found only {checked} bindings");
    }
}
