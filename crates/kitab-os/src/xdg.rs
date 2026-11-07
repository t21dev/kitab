// SPDX-License-Identifier: AGPL-3.0-or-later

//! The freedesktop.org files behind file types on Linux (PLAN.md §3.13):
//! Kitab's .desktop entry, its thumbnailer, its shared-mime-info package, and
//! the user's `mimeapps.list`. Plain text in, plain text out, so it is tested
//! on every OS; assoc.rs reads and writes the files on Linux.

/// The application id: `dev.t21.kitab.desktop`.
pub const DESKTOP_ID: &str = "dev.t21.kitab.desktop";

/// Marks a file Kitab wrote itself (Settings › File types › Set up), so
/// Remove deletes only those and never a package's.
pub const WRITTEN_MARK: &str = "X-Kitab-Written=true";

/// The packaged files, the one source for both the package and Set up.
const DESKTOP: &str = include_str!("../../../packaging/linux/dev.t21.kitab.desktop");
const THUMBNAILER: &str = include_str!("../../../packaging/linux/dev.t21.kitab.thumbnailer");
const MIME_XML: &str = include_str!("../../../packaging/linux/dev.t21.kitab.xml");

/// The other names some systems use for Kitab's MIME types: set as the
/// default together with the main one.
pub fn aliases(mime: &str) -> &'static [&'static str] {
    match mime {
        "text/markdown" => &["text/x-markdown"],
        "application/vnd.comicbook+zip" => &["application/x-cbz"],
        "application/oxps" => &["application/vnd.ms-xpsdocument"],
        _ => &[],
    }
}

/// `exe` as one argument of an `Exec=` line (Desktop Entry spec: quoted when
/// it holds anything but plain path characters, with `"`, `` ` ``, `$` and
/// `\` escaped, and `%` doubled so it isn't read as a field code).
pub fn exec_quote(exe: &str) -> String {
    let plain = !exe.is_empty()
        && exe
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c));
    if plain {
        return exe.to_string();
    }
    let mut out = String::from("\"");
    for c in exe.chars() {
        match c {
            // Escaped once for the quoting, then every backslash once more
            // for the keyfile: `$` is written `\\$`, `\` is written `\\\\`.
            '"' | '`' | '$' => {
                out.push_str("\\\\");
                out.push(c);
            }
            '\\' => out.push_str("\\\\\\\\"),
            '%' => out.push_str("%%"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A packaged keyfile with every `kitab` command pointed at `exe` (Set up
/// runs from wherever Kitab is, which may not be on PATH), and the mark.
fn point_at(template: &str, exe: &str) -> String {
    let exe = exec_quote(exe);
    let mut out: String = template
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            for key in ["Exec=", "TryExec="] {
                if let Some(rest) = l.strip_prefix(key) {
                    if rest == "kitab" || rest.starts_with("kitab ") {
                        return format!("{key}{exe}{}", &rest["kitab".len()..]);
                    }
                }
            }
            l.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out.push_str(WRITTEN_MARK);
    out.push('\n');
    out
}

/// Kitab's .desktop entry, opening files with `exe`.
pub fn desktop_entry(exe: &str) -> String {
    point_at(DESKTOP, exe)
}

/// Kitab's thumbnailer entry, drawing with `exe`.
pub fn thumbnailer_entry(exe: &str) -> String {
    point_at(THUMBNAILER, exe)
}

/// Kitab's shared-mime-info package (Markdown's type and the document
/// icons), with the mark in a comment.
pub fn mime_package() -> String {
    MIME_XML.replacen("?>", &format!("?>\n<!-- {WRITTEN_MARK} -->"), 1)
}

/// The MIME types a .desktop entry lists.
pub fn desktop_mimes(entry: &str) -> Vec<String> {
    entry
        .lines()
        .find_map(|l| l.trim().strip_prefix("MimeType="))
        .map(|v| {
            v.split(';')
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// `mimeapps.list` without Kitab: its id taken out of the defaults and the
/// added associations, keys left with nothing dropped, the rest untouched.
pub fn without_kitab(mimeapps: &str) -> String {
    let mut out = Vec::new();
    let mut ours = false;
    for line in mimeapps.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            ours = t == "[Default Applications]" || t == "[Added Associations]";
            out.push(line.to_string());
            continue;
        }
        match t.split_once('=') {
            Some((key, value)) if ours && value.split(';').any(|a| a.trim() == DESKTOP_ID) => {
                let rest: Vec<&str> = value
                    .split(';')
                    .map(str::trim)
                    .filter(|a| !a.is_empty() && *a != DESKTOP_ID)
                    .collect();
                if !rest.is_empty() {
                    out.push(format!("{}={};", key.trim(), rest.join(";")));
                }
            }
            _ => out.push(line.to_string()),
        }
    }
    let mut s = out.join("\n");
    if mimeapps.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_paths_are_quoted_only_when_needed() {
        assert_eq!(exec_quote("/opt/kitab/kitab"), "/opt/kitab/kitab");
        assert_eq!(
            exec_quote("/home/a b/Kitab$1/kitab"),
            "\"/home/a b/Kitab\\\\$1/kitab\""
        );
        assert_eq!(exec_quote("/tmp/100%/kitab"), "\"/tmp/100%%/kitab\"");
    }

    #[test]
    fn written_entries_point_at_this_kitab_and_carry_the_mark() {
        let d = desktop_entry("/home/me/Apps/kitab");
        assert!(d.contains("\nExec=/home/me/Apps/kitab %F\n"), "{d}");
        assert!(d.contains(WRITTEN_MARK) && !d.contains("SPDX"));
        assert!(desktop_mimes(&d).contains(&"application/pdf".to_string()));
        let t = thumbnailer_entry("/home/me/Apps/kitab");
        assert!(t.contains("TryExec=/home/me/Apps/kitab\n"));
        assert!(t.contains("Exec=/home/me/Apps/kitab --thumbnail -s %s %i %o"));
        let x = mime_package();
        assert!(x.starts_with("<?xml") && x.contains(WRITTEN_MARK));
    }

    #[test]
    fn every_registered_type_is_in_the_desktop_entry() {
        let listed = desktop_mimes(&desktop_entry("kitab"));
        for t in crate::assoc::FILE_TYPES {
            assert!(listed.iter().any(|m| m == t.mime), "{}", t.mime);
            for a in aliases(t.mime) {
                assert!(listed.iter().any(|m| m == a), "{a}");
            }
        }
    }

    #[test]
    fn removing_kitab_leaves_other_apps_alone() {
        let before = "[Default Applications]\n\
            application/pdf=dev.t21.kitab.desktop;\n\
            text/markdown=dev.t21.kitab.desktop;org.gnome.TextEditor.desktop;\n\
            image/png=org.gnome.Loupe.desktop\n\
            \n[Added Associations]\n\
            application/epub+zip=dev.t21.kitab.desktop;\n\
            [Removed Associations]\n\
            text/plain=dev.t21.kitab.desktop;\n";
        let after = without_kitab(before);
        assert!(!after.contains("application/pdf="));
        assert!(after.contains("text/markdown=org.gnome.TextEditor.desktop;"));
        assert!(after.contains("image/png=org.gnome.Loupe.desktop\n"));
        assert!(!after.contains("application/epub+zip="));
        // A user's own "never offer Kitab" stays.
        assert!(after.contains("[Removed Associations]\ntext/plain=dev.t21.kitab.desktop;"));
        assert!(after.ends_with('\n'));
        assert_eq!(without_kitab(&after), after);
    }
}
