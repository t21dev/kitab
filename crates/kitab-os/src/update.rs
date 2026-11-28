// SPDX-License-Identifier: AGPL-3.0-or-later

//! Check for updates (Settings › About): ask Kitab's website for the latest
//! release's number and notes (`latest.json`, published with every release),
//! and GitHub's API too, which limits requests per address and sometimes says
//! no. Either answer is enough; the newer one wins. Kitab only asks when the user presses Check now, or after
//! they turn on "Check automatically" (AGENTS.md rule 12, PRIVACY.md). Nothing
//! is downloaded or installed unless the user presses Install (`install.rs`);
//! where Kitab can't install itself, Download opens the release page.
//!
//! The request goes through the system's `curl` (in Windows 10 1803 and later,
//! macOS and Linux) rather than an HTTP and TLS stack in the app: no size cost
//! (AGENTS.md rule 7) and the system's own certificate store.

use std::cmp::Ordering;

/// The package manager that updates Kitab and sets up its file types, when
/// it isn't Kitab's own business: Flatpak (whose sandbox gives Kitab no
/// network either), or the Microsoft Store (an MSIX package, whose manifest
/// declares the file types and which installs new versions itself).
pub fn managed_by() -> Option<&'static str> {
    if crate::packaged() {
        return Some("the Microsoft Store");
    }
    std::env::var_os("FLATPAK_ID").map(|_| "Flatpak")
}

/// What updates a Kitab that Scoop installed (`…\scoop\apps\kitab\…`).
pub const SCOOP_UPDATE: &str = "scoop update kitab";

/// Whether Scoop installed this Kitab. Kitab still looks for new versions,
/// and the update dialog offers `scoop update kitab` instead of the download
/// page.
pub fn scoop() -> bool {
    static SCOOP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SCOOP.get_or_init(|| {
        cfg!(windows)
            && std::env::current_exe()
                .ok()
                .is_some_and(|exe| in_scoop(&exe.to_string_lossy()))
    })
}

/// A path inside a Scoop app folder: `…\scoop\apps\<app>\…`, for the
/// user's Scoop or the machine-wide one (`ProgramData\scoop`), or a custom
/// root named in `SCOOP`/`SCOOP_GLOBAL`.
fn in_scoop(exe: &str) -> bool {
    let exe = exe.to_lowercase().replace('/', "\\");
    if exe.contains("\\scoop\\apps\\") {
        return true;
    }
    ["SCOOP", "SCOOP_GLOBAL"].iter().any(|var| {
        std::env::var(var).is_ok_and(|root| {
            let root = root.trim_end_matches(['\\', '/']).to_lowercase();
            !root.is_empty() && exe.starts_with(&format!("{}\\apps\\", root.replace('/', "\\")))
        })
    })
}

#[cfg(test)]
mod scoop_tests {
    #[test]
    fn knows_a_scoop_install() {
        assert!(super::in_scoop(
            r"C:\Users\a\scoop\apps\kitab\current\kitab.exe"
        ));
        assert!(super::in_scoop(
            r"C:\ProgramData\scoop\apps\kitab\1.7.1\kitab.exe"
        ));
        assert!(!super::in_scoop(r"C:\Program Files\Kitab\kitab.exe"));
        assert!(!super::in_scoop(r"D:\scoop-notes\kitab.exe"));
    }
}

/// How to get a new version from `managed_by`, for Settings › About.
pub fn managed_hint(by: &str) -> &'static str {
    if by == "Flatpak" {
        "Your software centre installs new versions, or run: flatpak update"
    } else {
        "The Store installs new versions by itself. To check now, open the Microsoft Store and go to Downloads."
    }
}

/// The project on GitHub.
pub const REPO: &str = "https://github.com/t21dev/kitab";
/// The user guide on the website.
pub const DOCS: &str = "https://kitab.t21.dev/docs";
/// Where a new issue starts: GitHub offers the bug and feature forms.
pub const NEW_ISSUE: &str = "https://github.com/t21dev/kitab/issues/new/choose";
/// The latest release's page, where Download goes.
pub const RELEASES_PAGE: &str = "https://github.com/t21dev/kitab/releases/latest";
/// The API answer for the latest published release (drafts and pre-releases
/// are left out by GitHub).
const LATEST_API: &str = "https://api.github.com/repos/t21dev/kitab/releases/latest";
/// The same answer from Kitab's website, which has no request limit.
const SITE_LATEST: &str = "https://kitab.t21.dev/latest.json";
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

/// Read the website's `latest.json`: `{"version", "url", "published", "notes"}`.
pub fn parse_site(json: &str) -> Result<Release, String> {
    let bad = || "The website sent something unexpected".to_string();
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| bad())?;
    let text = |key: &str| {
        v.get(key)
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string()
    };
    let version = text("version")
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    if !version.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(bad());
    }
    let url = text("url");
    let url = if url.starts_with(REPO) {
        url
    } else {
        RELEASES_PAGE.to_string()
    };
    Ok(Release {
        version,
        url,
        notes: text("notes"),
        published: text("published").chars().take(10).collect(),
    })
}

/// Why a request failed.
enum Failed {
    /// curl isn't there.
    NoCurl,
    /// The HTTP error status (404, 403, 429…), or 0 when none came back.
    Status(u16),
}

/// Ask for the latest release in three places at once, and the newest answer
/// wins, so any of them failing (or behind) is fine: the website's
/// `latest.json`; GitHub's API (rate-limited, sometimes "busy"); and where
/// github.com's own "latest release" page redirects to, which has no limit
/// and is never behind, but only gives the number. Blocks for up to ~15 s:
/// call it off the UI thread.
pub fn fetch_latest() -> Result<Release, String> {
    let (site, github, page) = std::thread::scope(|s| {
        let site = s.spawn(|| get(SITE_LATEST, "application/json"));
        let page = s.spawn(|| redirect_of(RELEASES_PAGE));
        let github = get(LATEST_API, "application/vnd.github+json");
        (
            site.join().unwrap_or(Err(Failed::Status(0))),
            github,
            page.join().unwrap_or(None),
        )
    });
    let site = site.map_err(|_| String::new()).and_then(|j| parse_site(&j));
    let page = page
        .and_then(|url| release_from_page(&url))
        .ok_or_else(String::new);
    let github = match github {
        Ok(j) => parse_release(&j),
        Err(Failed::NoCurl) => Err("Couldn't check for updates (curl isn't available)".into()),
        Err(Failed::Status(404)) => Err("GitHub has no release yet".into()),
        Err(Failed::Status(403 | 429)) => {
            Err("GitHub is busy and asked Kitab to wait. Try again in an hour.".into())
        }
        Err(_) => Err("Couldn't check for updates. Check the connection and try again.".into()),
    };
    newest(vec![github, site, page])
}

/// The newest answer, one with release notes first when two say the same
/// version; GitHub's error (the first) when none came.
fn newest(answers: Vec<Result<Release, String>>) -> Result<Release, String> {
    let mut error = None;
    let mut best: Option<Release> = None;
    for a in answers {
        match a {
            Ok(r) => {
                let better =
                    best.as_ref()
                        .is_none_or(|b| match compare_versions(&r.version, &b.version) {
                            Ordering::Greater => true,
                            Ordering::Equal => b.notes.is_empty() && !r.notes.is_empty(),
                            Ordering::Less => false,
                        });
                if better {
                    best = Some(r);
                }
            }
            Err(e) => {
                if error.is_none() {
                    error = Some(e);
                }
            }
        }
    }
    best.ok_or_else(|| error.unwrap_or_default())
}

/// Where `url` redirects to, without following it (a HEAD request).
fn redirect_of(url: &str) -> Option<String> {
    let mut cmd = std::process::Command::new(curl());
    cmd.args([
        "--silent",
        "--head",
        "--proto",
        "=https",
        "--max-time",
        "15",
        "-H",
        "User-Agent: Kitab-Reader",
        "--write-out",
        "
KITAB-REDIRECT:%{redirect_url}",
        url,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let target = text.rsplit_once("KITAB-REDIRECT:")?.1.trim().to_string();
    (!target.is_empty()).then_some(target)
}

/// The release that github.com's latest-release page redirected to:
/// `…/releases/tag/v0.9.0` is 0.9.0 (no notes; the page has them).
fn release_from_page(url: &str) -> Option<Release> {
    let tag = url.strip_prefix(&format!("{REPO}/releases/tag/"))?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    if !version.starts_with(|c: char| c.is_ascii_digit())
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))
    {
        return None;
    }
    Some(Release {
        version,
        url: url.to_string(),
        notes: String::new(),
        published: String::new(),
    })
}

/// The HTTP status in curl's "The requested URL returned error: 403".
fn status_of(stderr: &str) -> u16 {
    stderr
        .split("error: ")
        .nth(1)
        .and_then(|t| t.get(..3))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// GET `url` with the system's curl. Blocks for up to ~15 s.
fn get(url: &str, accept: &str) -> Result<String, Failed> {
    let accept = format!("Accept: {accept}");
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
        &accept,
        "-H",
        "User-Agent: Kitab-Reader",
        url,
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|_| Failed::NoCurl)?;
    if !out.status.success() {
        return Err(Failed::Status(status_of(&String::from_utf8_lossy(
            &out.stderr,
        ))));
    }
    if out.stdout.len() > MAX_BYTES {
        return Err(Failed::Status(0));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The system's curl: by full path on Windows, so a `curl.exe` in the
/// working folder is never the one that runs.
pub(crate) fn curl() -> std::path::PathBuf {
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
    /// A fenced code block, line by line (shown as is, scrolling sideways).
    Code(Vec<String>),
    /// A table: the header row first. Cells are plain text.
    Table(Vec<Vec<String>>),
}

/// A table row's cells: `| a | b |` → ["a", "b"].
fn cells(row: &str) -> Vec<String> {
    let t = row.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| plain(c.trim())).collect()
}

/// `|---|:--:|` and the like: the line under a table's header.
fn is_rule(row: &str) -> bool {
    let t = row.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// The release notes as headings, bullets and paragraphs, with inline
/// Markdown (links, bold, code) reduced to its text. At most `max` blocks.
pub fn note_blocks(notes: &str, max: usize) -> Vec<NoteBlock> {
    let mut out: Vec<NoteBlock> = Vec::new();
    let mut code: Option<Vec<String>> = None;
    for line in notes.lines() {
        let t = line.trim();
        // Code fences: the lines between them kept as they are.
        if t.starts_with("```") {
            match code.take() {
                Some(lines) => out.push(NoteBlock::Code(lines)),
                None => code = Some(Vec::new()),
            }
        } else if let Some(lines) = code.as_mut() {
            if lines.len() < 200 {
                lines.push(line.trim_end().to_string());
            }
            continue;
        } else if t.starts_with('|') {
            // Table rows join the table just above them.
            if is_rule(t) {
                continue;
            }
            match out.last_mut() {
                Some(NoteBlock::Table(rows)) if rows.len() < 200 => rows.push(cells(t)),
                Some(NoteBlock::Table(_)) => {}
                _ => out.push(NoteBlock::Table(vec![cells(t)])),
            }
            continue;
        } else if t.is_empty() || t.starts_with("<!--") {
            continue;
        } else if let Some(h) = t.strip_prefix('#') {
            out.push(NoteBlock::Heading(plain(h.trim_start_matches('#').trim())));
        } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            out.push(NoteBlock::Bullet(plain(b)));
        } else if let Some((n, rest)) = t.split_once(". ")
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
        {
            out.push(NoteBlock::Bullet(plain(rest)));
        } else {
            let t = t.strip_prefix("> ").unwrap_or(t);
            out.push(NoteBlock::Text(plain(t)));
        }
        if out.len() >= max {
            out.truncate(max);
            return out;
        }
    }
    // A fence never closed: what was in it still shows.
    if let Some(lines) = code.filter(|l| !l.is_empty()) {
        out.push(NoteBlock::Code(lines));
    }
    out.truncate(max);
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
    fn notes_keep_code_blocks_and_tables() {
        let notes = "## Added\n- **Forms** [docs](https://x.y)\n1. First\n\n| Key | What |\n|---|:--:|\n| `Ctrl+S` | Save |\n| Esc | Close |\n\n```\nabc123  Kitab.exe\n```\n";
        let b = note_blocks(notes, 60);
        assert_eq!(
            b,
            vec![
                NoteBlock::Heading("Added".into()),
                NoteBlock::Bullet("Forms docs".into()),
                NoteBlock::Bullet("First".into()),
                NoteBlock::Table(vec![
                    vec!["Key".into(), "What".into()],
                    vec!["Ctrl+S".into(), "Save".into()],
                    vec!["Esc".into(), "Close".into()],
                ]),
                NoteBlock::Code(vec!["abc123  Kitab.exe".into()]),
            ]
        );
    }

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
    fn the_website_answers_when_github_cannot() {
        let site = parse_site(
            r#"{"version":"0.8.0","url":"https://github.com/t21dev/kitab/releases/tag/v0.8.0","published":"2026-11-17","notes":"- X"}"#,
        )
        .expect("parses");
        assert_eq!(
            (site.version.as_str(), site.published.as_str()),
            ("0.8.0", "2026-11-17")
        );
        let busy: Result<Release, String> = Err("GitHub is busy".into());
        assert_eq!(
            newest(vec![busy.clone(), Ok(site.clone())])
                .unwrap()
                .version,
            "0.8.0"
        );
        // A website not yet redeployed loses to GitHub's newer answer.
        let gh = parse_release(&github(
            "v0.9.0",
            "https://github.com/t21dev/kitab/releases/tag/v0.9.0",
        ))
        .unwrap();
        assert_eq!(newest(vec![Ok(gh), Ok(site)]).unwrap().version, "0.9.0");
        assert!(newest(vec![busy, Err(String::new())]).is_err());
        assert!(parse_site("{}").is_err());
        assert!(parse_site("<html>").is_err());
        let r = parse_site(r#"{"version":"1.0.0","url":"https://evil.example"}"#).unwrap();
        assert_eq!(r.url, RELEASES_PAGE);
        assert_eq!(
            status_of("curl: (22) The requested URL returned error: 403"),
            403
        );
        assert_eq!(status_of("curl: (6) Could not resolve host: x"), 0);
    }

    #[test]
    fn the_release_page_redirect_gives_the_number() {
        let r = release_from_page("https://github.com/t21dev/kitab/releases/tag/v0.9.0").unwrap();
        assert_eq!(r.version, "0.9.0");
        assert!(release_from_page("https://evil.example/releases/tag/v9.9.9").is_none());
        assert!(release_from_page("https://github.com/t21dev/kitab/releases").is_none());
        assert!(release_from_page("https://github.com/t21dev/kitab/releases/tag/v1;rm").is_none());
        // A stale website loses to the page; the same number prefers notes.
        let stale = Release {
            version: "0.8.1".into(),
            url: String::new(),
            notes: "notes".into(),
            published: String::new(),
        };
        let busy: Result<Release, String> = Err("busy".into());
        assert_eq!(
            newest(vec![busy.clone(), Ok(stale.clone()), Ok(r.clone())])
                .unwrap()
                .version,
            "0.9.0"
        );
        let same = Release {
            version: "0.9.0".into(),
            notes: "## Added".into(),
            ..stale
        };
        assert_eq!(
            newest(vec![busy, Ok(r), Ok(same)]).unwrap().notes,
            "## Added"
        );
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
