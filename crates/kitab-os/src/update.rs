// SPDX-License-Identifier: AGPL-3.0-or-later

//! Check for updates (Settings › About): ask GitHub for the latest release's
//! number and notes. Kitab only asks when the user presses Check now, or after
//! they turn on "Check automatically" (AGENTS.md rule 12, PRIVACY.md). Nothing
//! is downloaded or installed: Download opens the release page in the browser.
//!
//! The request goes through the system's `curl` (in Windows 10 1803 and later,
//! macOS and Linux) rather than an HTTP and TLS stack in the app: no size cost
//! (AGENTS.md rule 7) and the system's own certificate store.

use std::cmp::Ordering;

/// The package manager that updates Kitab, when it isn't Kitab's own
/// business: Flatpak (whose sandbox gives Kitab no network either).
pub fn managed_by() -> Option<&'static str> {
    std::env::var_os("FLATPAK_ID").map(|_| "Flatpak")
}

/// The project on GitHub.
pub const REPO: &str = "https://github.com/t21dev/kitab";
/// The latest release's page, where Download goes.
pub const RELEASES_PAGE: &str = "https://github.com/t21dev/kitab/releases/latest";
/// The API answer for the latest published release (drafts and pre-releases
/// are left out by GitHub).
const LATEST_API: &str = "https://api.github.com/repos/t21dev/kitab/releases/latest";
/// The answer is a few kilobytes; anything near this is not GitHub's.
const MAX_BYTES: usize = 1 << 20;

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// "0.2.0", without the tag's "v".
    pub version: String,
    /// The release's page.
    pub url: String,
    /// The release notes (Markdown).
    pub notes: String,
    /// "2026-11-07", or empty.
    pub published: String,
}

/// Compare dotted versions ("v1.2.3", "1.2.3-beta.1"): numbers part by part,
/// and a pre-release sorts before the release it leads to.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<u64>, Option<&str>) {
        let v = v.trim().trim_start_matches(['v', 'V']);
        let v = v.split('+').next().unwrap_or(v);
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (v, None),
        };
        let nums = core
            .split('.')
            .map(|n| n.trim().parse::<u64>().unwrap_or(0))
            .collect();
        (nums, pre)
    }
    let (na, pa) = split(a);
    let (nb, pb) = split(b);
    for i in 0..na.len().max(nb.len()) {
        let (x, y) = (
            na.get(i).copied().unwrap_or(0),
            nb.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x.cmp(&y);
        }
    }
    match (pa, pb) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

/// Whether `release` is newer than the running `current` version.
pub fn is_newer(release: &Release, current: &str) -> bool {
    !release.version.is_empty() && compare_versions(&release.version, current) == Ordering::Greater
}

/// Read GitHub's answer for a release.
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "GitHub sent something unexpected".to_string())?;
    let text = |key: &str| {
        v.get(key)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let tag = text("tag_name");
    let version = tag.trim().trim_start_matches(['v', 'V']).to_string();
    if version.is_empty() {
        return Err("GitHub has no release yet".into());
    }
    let url = text("html_url");
    // Only a page on the project's own site is opened.
    let url = if url.starts_with(REPO) {
        url
    } else {
        RELEASES_PAGE.to_string()
    };
    let published = text("published_at").chars().take(10).collect();
    Ok(Release {
        version,
        url,
        notes: text("body"),
        published,
    })
}

/// Ask GitHub for the latest release. Blocks for up to ~15 s: call it off the
/// UI thread.
pub fn fetch_latest() -> Result<Release, String> {
    let mut cmd = std::process::Command::new(curl());
    cmd.args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--proto",
        "=https",
        "--max-time",
        "15",
        "--max-filesize",
        "1048576",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "User-Agent: Kitab-Reader",
        LATEST_API,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .output()
        .map_err(|_| "Couldn't reach GitHub (curl isn't available)".to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(if err.contains("404") {
            "GitHub has no release yet".into()
        } else {
            "Couldn't reach GitHub. Check the connection and try again.".into()
        });
    }
    if out.stdout.len() > MAX_BYTES {
        return Err("GitHub sent something unexpected".into());
    }
    parse_release(&String::from_utf8_lossy(&out.stdout))
}

/// The system's curl: by full path on Windows, so a `curl.exe` in the
/// working folder is never the one that runs.
fn curl() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let p = std::path::PathBuf::from(root)
                .join("System32")
                .join("curl.exe");
            if p.is_file() {
                return p;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        return "/usr/bin/curl".into();
    }
    #[allow(unreachable_code)]
    "curl".into()
}

/// A block of release notes, for the update dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteBlock {
    Heading(String),
    Bullet(String),
    Text(String),
}

/// The release notes as headings, bullets and paragraphs, with inline
/// Markdown (links, bold, code) reduced to its text. At most `max` blocks.
pub fn note_blocks(notes: &str, max: usize) -> Vec<NoteBlock> {
    let mut out = Vec::new();
    for line in notes.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("<!--") || t.starts_with("```") {
            continue;
        }
        let block = if let Some(h) = t.strip_prefix('#') {
            NoteBlock::Heading(plain(h.trim_start_matches('#').trim()))
        } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            NoteBlock::Bullet(plain(b))
        } else {
            NoteBlock::Text(plain(t))
        };
        out.push(block);
        if out.len() == max {
            break;
        }
    }
    out
}

/// Inline Markdown to plain text: `[text](url)` → text, no `**`, `__`, `` ` ``.
fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](") {
            Some(close) if after[close + 2..].contains(')') => {
                out.push_str(&after[..close]);
                let tail = &after[close + 2..];
                let end = tail.find(')').map_or(tail.len(), |e| e + 1);
                rest = &tail[end..];
            }
            _ => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace("__", "").replace('`', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number_and_pre_release() {
        assert_eq!(compare_versions("0.10.0", "0.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("v0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.1", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.2.0-beta.1", "0.2.0"), Ordering::Less);
        assert_eq!(compare_versions("0.2.0-beta.1", "0.1.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0+build", "1.0.0"), Ordering::Equal);
    }

    fn github(tag: &str, url: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": url,
            "body": "## Added\n- **Fast** [find](https://x.y)\n",
            "published_at": "2026-11-07T10:00:00Z",
        })
        .to_string()
    }

    #[test]
    fn a_release_is_read_and_compared() {
        let r = parse_release(&github(
            "v0.2.0",
            "https://github.com/t21dev/kitab/releases/tag/v0.2.0",
        ))
        .expect("parses");
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.published, "2026-11-07");
        assert!(r.url.ends_with("/v0.2.0"));
        assert!(is_newer(&r, "0.1.0"));
        assert!(!is_newer(&r, "0.2.0"));
        assert!(!is_newer(&r, "0.3.0"));
    }

    #[test]
    fn only_the_projects_own_page_is_opened() {
        let r = parse_release(&github("0.2.0", "https://evil.example/download")).expect("parses");
        assert_eq!(r.url, RELEASES_PAGE);
    }

    #[test]
    fn junk_and_empty_answers_are_errors() {
        assert!(parse_release("<html>").is_err());
        assert!(parse_release("{}").is_err());
    }

    #[test]
    fn notes_become_plain_blocks() {
        let blocks = note_blocks(
            "## Added\n\n- **Fast** [find](https://x.y) in `PDF`\n* Two\n<!-- hidden -->\nThanks [a [b]",
            10,
        );
        assert_eq!(
            blocks,
            vec![
                NoteBlock::Heading("Added".into()),
                NoteBlock::Bullet("Fast find in PDF".into()),
                NoteBlock::Bullet("Two".into()),
                NoteBlock::Text("Thanks [a [b]".into()),
            ]
        );
        assert_eq!(note_blocks("- a\n- b\n- c", 2).len(), 2);
    }
}
