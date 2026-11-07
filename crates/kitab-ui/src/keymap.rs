// SPDX-License-Identifier: AGPL-3.0-or-later

//! keymap.json (PLAN.md §3.10): the user's own key bindings, Zed-style, on
//! top of the defaults in `actions.rs`.
//!
//! ```json
//! [
//!   // While reading: Ctrl+K finds the next result, N does nothing.
//!   { "context": "Reader", "bindings": { "ctrl-k": "FindNext", "n": null } }
//! ]
//! ```
//!
//! `//` comments are allowed. An action is a name from the command palette's
//! list (`NextPage`, or `kitab::NextPage`); `null` removes a default key. The
//! file sits in the data folder, is read off the UI thread after the window
//! opens, and again whenever Kitab's window comes back to the front.

use gpui::{Action, App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, NoAction};
use std::collections::HashSet;
use std::rc::Rc;

/// Bigger than any keymap a person writes.
pub const MAX_BYTES: u64 = 256 * 1024;

/// One binding as written in keymap.json.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserBinding {
    /// "ctrl-shift-k", or a sequence "ctrl-k ctrl-s".
    pub keys: String,
    /// `None` removes the keys (`null`).
    pub action: Option<String>,
    /// "Reader", "Workspace", "FindBar"…; `None` binds everywhere.
    pub context: Option<String>,
}

/// The actions keymap.json bound, so the palette shows their keys rather
/// than the defaults'.
#[derive(Default)]
pub struct UserBound(pub HashSet<String>);
impl gpui::Global for UserBound {}

/// Remove `//` comments outside strings.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_str = false;
        let mut escaped = false;
        let mut cut = line.len();
        let bytes = line.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if in_str {
                match b {
                    _ if escaped => escaped = false,
                    b'\\' => escaped = true,
                    b'"' => in_str = false,
                    _ => {}
                }
            } else if b == b'"' {
                in_str = true;
            } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
                cut = i;
                break;
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Read keymap.json. An empty file is no bindings.
pub fn parse(text: &str) -> Result<Vec<UserBinding>, String> {
    let clean = strip_comments(text);
    if clean.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value =
        serde_json::from_str(&clean).map_err(|e| format!("not valid JSON ({e})"))?;
    let sections = value
        .as_array()
        .ok_or("the file should be a list: [ { \"bindings\": { … } } ]")?;
    let mut out = Vec::new();
    for (n, section) in sections.iter().enumerate() {
        let entry = n + 1;
        let context = match section.get("context") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(c)) => Some(c.clone()),
            Some(_) => return Err(format!("entry {entry}: \"context\" should be text")),
        };
        let bindings = section
            .get("bindings")
            .and_then(|b| b.as_object())
            .ok_or(format!("entry {entry}: needs \"bindings\": {{ … }}"))?;
        for (keys, action) in bindings {
            let action = match action {
                serde_json::Value::Null => None,
                serde_json::Value::String(a) => Some(a.clone()),
                _ => {
                    return Err(format!(
                        "entry {entry}: \"{keys}\" should name an action or be null"
                    ));
                }
            };
            out.push(UserBinding {
                keys: keys.trim().to_string(),
                action,
                context: context.clone(),
            });
        }
    }
    Ok(out)
}

/// "NextPage" → "kitab::NextPage"; names with a namespace stay.
pub fn full_name(name: &str) -> String {
    if name.contains("::") {
        name.to_string()
    } else {
        format!("kitab::{name}")
    }
}

/// Bind the defaults and then `user` (later bindings win). Returns what
/// couldn't be bound, for a toast; the rest still applies.
pub fn apply(cx: &mut App, user: &[UserBinding]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut bound = HashSet::new();
    let mut bindings = Vec::new();
    for b in user {
        let predicate = match b.context.as_deref() {
            None => None,
            Some(c) => match KeyBindingContextPredicate::parse(c) {
                Ok(p) => Some(Rc::new(p)),
                Err(_) => {
                    errors.push(format!("unknown context \"{c}\""));
                    continue;
                }
            },
        };
        let action: Box<dyn Action> = match &b.action {
            None => Box::new(NoAction),
            Some(name) => match cx.build_action(&full_name(name), None) {
                Ok(a) => {
                    bound.insert(a.name().to_string());
                    a
                }
                Err(_) => {
                    errors.push(format!("no action called \"{name}\""));
                    continue;
                }
            },
        };
        match KeyBinding::load(
            &b.keys,
            action,
            predicate,
            false,
            None,
            &DummyKeyboardMapper,
        ) {
            Ok(k) => bindings.push(k),
            Err(_) => errors.push(format!("can't read the keys \"{}\"", b.keys)),
        }
    }
    // A plain key bound for the page still types in fields.
    let typing = [
        crate::actions::FIND_BAR,
        crate::actions::MD_EDITOR,
        crate::actions::MARKUP_HEX,
        crate::text_field::TEXT_FIELD,
    ];
    let plain: Vec<&str> = user
        .iter()
        .filter(|b| b.action.is_some() && !b.keys.contains("ctrl-") && !b.keys.contains("alt-"))
        .map(|b| b.keys.as_str())
        .collect();
    for ctx in typing {
        for keys in &plain {
            if let Ok(k) = KeyBinding::load(
                keys,
                Box::new(NoAction),
                KeyBindingContextPredicate::parse(ctx).ok().map(Rc::new),
                false,
                None,
                &DummyKeyboardMapper,
            ) {
                bindings.push(k);
            }
        }
    }
    cx.clear_key_bindings();
    crate::actions::bind_default_keys(cx);
    cx.bind_keys(bindings);
    cx.set_global(UserBound(bound));
    errors
}

/// A new keymap.json: how it works, and every action there is.
pub fn template(action_names: &[&str]) -> String {
    let mut names: Vec<&str> = action_names
        .iter()
        .filter_map(|n| n.strip_prefix("kitab::"))
        .collect();
    names.sort_unstable();
    let mut list = String::new();
    for chunk in names.chunks(6) {
        list.push_str("//   ");
        list.push_str(&chunk.join(", "));
        list.push('\n');
    }
    format!(
        "// Kitab key bindings. They add to the defaults (Settings › Shortcuts)
// and win over them. Save, then switch back to Kitab: they apply at once.
//
// Each entry has a context and its bindings:
//   \"Workspace\" anywhere, \"Reader\" on a page, \"FindBar\" in the find field.
// Keys: ctrl-, shift-, alt- and a key, like \"ctrl-shift-k\", \"f5\", \"space\";
// two in a row with a space: \"ctrl-k ctrl-s\".
// Action: a name below. null removes a default key.
//
// Actions:
{list}
[
  // {{ \"context\": \"Reader\", \"bindings\": {{ \"ctrl-k\": \"FindNext\", \"n\": null }} }}
]
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bindings_contexts_and_nulls() {
        let text = r#"
            // comment
            [
              { "context": "Reader", "bindings": { "ctrl-k": "FindNext", "n": null } },
              { "bindings": { "f5": "kitab::OpenSettings" } } // trailing
            ]"#;
        let b = parse(text).expect("parses");
        assert_eq!(b.len(), 3);
        assert!(b.contains(&UserBinding {
            keys: "n".into(),
            action: None,
            context: Some("Reader".into()),
        }));
        assert!(b.contains(&UserBinding {
            keys: "f5".into(),
            action: Some("kitab::OpenSettings".into()),
            context: None,
        }));
    }

    #[test]
    fn slashes_inside_strings_are_not_comments() {
        let b = parse(r#"[{ "bindings": { "ctrl-/": "ShowShortcuts" } }]"#).expect("parses");
        assert_eq!(b[0].keys, "ctrl-/");
    }

    #[test]
    fn mistakes_say_where() {
        assert!(parse("{").unwrap_err().contains("JSON"));
        assert!(parse("{}").unwrap_err().contains("list"));
        assert!(
            parse(r#"[{ "context": "Reader" }]"#)
                .unwrap_err()
                .contains("entry 1")
        );
        assert!(
            parse(r#"[{ "bindings": { "x": 3 } }]"#)
                .unwrap_err()
                .contains("\"x\"")
        );
        assert!(
            parse("  // nothing yet\n")
                .expect("empty is fine")
                .is_empty()
        );
    }

    #[test]
    fn short_names_get_the_namespace() {
        assert_eq!(full_name("NextPage"), "kitab::NextPage");
        assert_eq!(full_name("zed::NoAction"), "zed::NoAction");
    }

    #[test]
    fn the_template_parses_and_lists_actions() {
        let t = template(&["kitab::NextPage", "kitab::ZoomIn", "zed::NoAction"]);
        assert!(t.contains("NextPage, ZoomIn"));
        assert!(!t.contains("NoAction,"));
        assert!(parse(&t).expect("template parses").is_empty());
    }
}
