// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `kitab` command end to end: the built binary on the fixtures, its
//! output (human and JSON) and its exit codes. Outputs go to a temp folder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_kitab-cli");

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/pdf")
        .join(name)
}

fn fx(name: &str) -> String {
    fixture(name).to_string_lossy().into_owned()
}

/// A fresh folder for one test's outputs.
fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("kitab-cli-tests-{}", std::process::id()))
        .join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn json(&self) -> Value {
        let lines: Vec<&str> = self.stdout.lines().collect();
        assert_eq!(lines.len(), 1, "one JSON line, got: {}", self.stdout);
        serde_json::from_str(lines[0]).expect("valid JSON")
    }
}

fn run_with(args: &[&str], env: &[(&str, &str)], stdin: Option<&str>) -> Run {
    let mut cmd = Command::new(BIN);
    cmd.args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("runs");
    if let Some(text) = stdin {
        use std::io::Write;
        let mut pipe = child.stdin.take().expect("stdin");
        pipe.write_all(text.as_bytes()).expect("writes stdin");
    }
    let out = child.wait_with_output().expect("finishes");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn run(args: &[&str]) -> Run {
    run_with(args, &[], None)
}

/// Run and expect success; returns the JSON object (adds --json).
fn ok_json(args: &[&str]) -> Value {
    let mut all = args.to_vec();
    all.push("--json");
    let r = run(&all);
    assert_eq!(r.code, 0, "{args:?}: {} {}", r.stdout, r.stderr);
    let v = r.json();
    assert_eq!(v["ok"], true);
    v
}

fn pages_of(path: &Path) -> u64 {
    ok_json(&["info", &s(path)])["pages"]
        .as_u64()
        .expect("page count")
}

const COMMANDS: &[&str] = &[
    "info",
    "text",
    "markdown",
    "search",
    "render",
    "annotations",
    "verify",
    "pages",
    "merge",
    "split",
    "compress",
    "password",
    "images-to-pdf",
    "images",
    "watermark",
    "number",
    "annotate",
    "sign",
    "agent-skill",
    "help",
];

#[test]
fn help_overview_and_every_command() {
    let r = run(&["--help"]);
    assert_eq!(r.code, 0);
    for want in [
        "Read",
        "Pages",
        "Files",
        "Marks",
        "Signatures",
        "Agent",
        "Exit codes",
    ] {
        assert!(r.stdout.contains(want), "overview lacks {want}");
    }
    assert!(r.stdout.contains("For AI agents"));
    for c in COMMANDS {
        let a = run(&["help", c]);
        assert_eq!(a.code, 0, "help {c}");
        assert!(a.stdout.contains(&format!("kitab {c}")), "help {c}");
        assert!(a.stdout.contains("Examples"), "help {c} has examples");
        let b = run(&[c, "--help"]);
        assert_eq!(b.code, 0, "{c} --help");
        assert_eq!(a.stdout, b.stdout);
        assert!(
            a.stdout.lines().all(|l| l.chars().count() <= 100),
            "{c}: a help line is over 100 characters"
        );
    }
    assert_eq!(run(&["help", "nonsense"]).code, 2);
    let v = run(&["--version"]);
    assert_eq!(v.code, 0);
    assert_eq!(
        v.stdout.trim(),
        format!("kitab {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn is_command_only_for_commands() {
    use kitab_cli::is_command;
    for yes in [
        "info",
        "merge",
        "pages",
        "help",
        "--help",
        "-h",
        "agent-skill",
    ] {
        assert!(is_command(OsStr::new(yes)), "{yes}");
    }
    for no in [
        "report.pdf",
        "merge.pdf",
        "./merge",
        "dir/info",
        "C:\\docs\\a.pdf",
        "--version",
        "--thumbnail",
        "--print",
        "--unregister",
        "--relink",
        "--stdin",
        "--json",
        "",
        "Merge",
    ] {
        assert!(!is_command(OsStr::new(no)), "{no}");
    }
}

#[test]
fn info_json_describes_the_document() {
    let v = ok_json(&["info", &fx("01-text-outline.pdf")]);
    assert_eq!(v["command"], "info");
    assert_eq!(v["format"], "pdf");
    assert_eq!(v["pages"], 40);
    assert_eq!(v["encrypted"], false);
    assert_eq!(v["needs_password"], false);
    assert_eq!(v["outline"].as_array().map(Vec::len), Some(8));
    assert_eq!(v["outline"][5]["page"], 26);
    assert!(v["page_size"]["width"].as_f64().is_some_and(|w| w > 500.0));
    let mixed = ok_json(&["info", &fx("02-mixed-page-sizes.pdf")]);
    assert_eq!(mixed["page_sizes_differ"], true);
    assert!(mixed["page_sizes"].as_array().is_some_and(|a| a.len() > 1));
    // Locked: still succeeds and says so.
    let locked = ok_json(&["info", &fx("04a-encrypted-password-kitab.pdf")]);
    assert_eq!(locked["needs_password"], true);
    let opened = run_with(
        &[
            "info",
            &fx("04a-encrypted-password-kitab.pdf"),
            "--password-env",
            "KITAB_TEST_PW",
            "--json",
        ],
        &[("KITAB_TEST_PW", "kitab")],
        None,
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    assert_eq!(opened.json()["encrypted"], true);
    assert_eq!(opened.json()["needs_password"], false);
    // Human output is short sentences.
    let h = run(&["info", &fx("01-text-outline.pdf")]);
    assert!(h.stdout.contains("40 pages"), "{}", h.stdout);
}

#[test]
fn search_finds_the_needle_on_its_page() {
    let v = ok_json(&["search", &fx("01-text-outline.pdf"), "kitab-needle-17"]);
    assert_eq!(v["count"], 1);
    assert_eq!(v["hits"][0]["page"], 17);
    assert!(
        v["hits"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("kitab-needle-17"))
    );
    assert_eq!(v["hits"][0]["rects"][0].as_array().map(Vec::len), Some(4));
    // Counts from MANIFEST.md.
    assert_eq!(
        ok_json(&["search", &fx("01-text-outline.pdf"), "lantern"])["count"],
        10
    );
    assert_eq!(
        ok_json(&[
            "search",
            &fx("01-text-outline.pdf"),
            "lantern",
            "--whole-word"
        ])["count"],
        9
    );
    assert_eq!(
        ok_json(&["search", &fx("01-text-outline.pdf"), "Lantern", "--case"])["count"],
        1
    );
    let limited = ok_json(&[
        "search",
        &fx("01-text-outline.pdf"),
        "lantern",
        "--limit",
        "3",
    ]);
    assert_eq!(limited["count"], 3);
    assert_eq!(limited["truncated"], true);
    // A scan: nothing, and it says why.
    let scan = ok_json(&["search", &fx("10-scanned-and-images.pdf"), "zebra"]);
    assert_eq!(scan["count"], 0);
    assert!(scan["pages_without_text"].as_u64().is_some_and(|n| n >= 1));
    let h = run(&["search", &fx("01-text-outline.pdf"), "kitab-needle-17"]);
    assert!(h.stdout.contains("page 17:"), "{}", h.stdout);
}

#[test]
fn page_tools_give_the_right_page_counts() {
    let dir = temp("pages");
    let src = fx("01-text-outline.pdf");
    let out = |n: &str| s(&dir.join(n));

    let v = ok_json(&["pages", "keep", &src, "1-3,7", "-o", &out("keep.pdf")]);
    assert_eq!(v["command"], "pages keep");
    assert_eq!(v["pages"], 4);
    assert_eq!(pages_of(&dir.join("keep.pdf")), 4);
    // In the order typed.
    ok_json(&["pages", "keep", &src, "17,1", "-o", &out("order.pdf")]);
    let first = ok_json(&["search", &out("order.pdf"), "kitab-needle-17"]);
    assert_eq!(first["hits"][0]["page"], 1);

    ok_json(&["pages", "delete", &src, "1-10", "-o", &out("delete.pdf")]);
    assert_eq!(pages_of(&dir.join("delete.pdf")), 30);

    ok_json(&[
        "pages",
        "rotate",
        &src,
        "1",
        "--by",
        "90",
        "-o",
        &out("rot.pdf"),
    ]);
    let rot = ok_json(&["info", &out("rot.pdf")]);
    assert_eq!(rot["pages"], 40);
    let size = &rot["page_size"];
    assert!(size["width"].as_f64() > size["height"].as_f64(), "{size}");

    ok_json(&[
        "pages",
        "move",
        &src,
        "17",
        "--before",
        "1",
        "-o",
        &out("move.pdf"),
    ]);
    assert_eq!(pages_of(&dir.join("move.pdf")), 40);
    let moved = ok_json(&["search", &out("move.pdf"), "kitab-needle-17"]);
    assert_eq!(moved["hits"][0]["page"], 1);

    let v = ok_json(&[
        "pages",
        "insert",
        &src,
        "--from",
        &fx("10-scanned-and-images.pdf"),
        "--from-pages",
        "2",
        "--before",
        "end",
        "-o",
        &out("insert.pdf"),
    ]);
    assert_eq!(v["changed"], 1);
    assert_eq!(pages_of(&dir.join("insert.pdf")), 41);
    ok_json(&[
        "pages",
        "insert",
        &src,
        "--from",
        &fx("10-scanned-and-images.pdf"),
        "--before",
        "1",
        "-o",
        &out("insert2.pdf"),
    ]);
    assert_eq!(pages_of(&dir.join("insert2.pdf")), 42);

    // Out of range is a failed operation, not a usage error.
    let bad = run(&["pages", "keep", &src, "41", "-o", &out("bad.pdf")]);
    assert_eq!(bad.code, 1, "{}", bad.stderr);
    assert!(!dir.join("bad.pdf").exists());
    assert_eq!(
        run(&["pages", "spin", &src, "1", "-o", &out("x.pdf")]).code,
        2
    );
}

#[test]
fn merge_split_and_compress() {
    let dir = temp("files");
    let a = fx("01-text-outline.pdf");
    let b = fx("10-scanned-and-images.pdf");
    let merged = dir.join("merged.pdf");
    let v = ok_json(&["merge", &a, &b, "-o", &s(&merged)]);
    assert_eq!(v["pages"], 42);
    assert_eq!(v["inputs"], 2);
    assert_eq!(pages_of(&merged), 42);
    let h = run(&["merge", &a, &b, "-o", &s(&merged), "--force"]);
    assert_eq!(h.code, 0);
    assert!(
        h.stdout.starts_with("Merged 2 PDFs (42 pages)"),
        "{}",
        h.stdout
    );
    assert_eq!(run(&["merge", &a, "-o", &s(&dir.join("one.pdf"))]).code, 2);

    let parts = dir.join("parts");
    let v = ok_json(&["split", &a, "--every", "15", "-o", &s(&parts)]);
    let files = v["files"].as_array().expect("files");
    assert_eq!(files.len(), 3);
    assert_eq!(pages_of(&parts.join("01-text-outline-3.pdf")), 10);
    let pieces = dir.join("pieces");
    ok_json(&["split", &a, "--ranges", "1-2,3-7,8-", "-o", &s(&pieces)]);
    assert_eq!(pages_of(&pieces.join("01-text-outline-1.pdf")), 2);
    assert_eq!(pages_of(&pieces.join("01-text-outline-2.pdf")), 5);
    assert_eq!(pages_of(&pieces.join("01-text-outline-3.pdf")), 33);
    // Again without --force: the files exist.
    assert_eq!(
        run(&["split", &a, "--every", "15", "-o", &s(&parts)]).code,
        5
    );

    let small = dir.join("small.pdf");
    let v = ok_json(&["compress", &b, "--quality", "smallest", "-o", &s(&small)]);
    assert!(v["bytes_before"].as_u64().is_some_and(|n| n > 0));
    assert!(v["bytes_after"].as_u64().is_some_and(|n| n > 0));
    assert!(v["pictures_shrunk"].is_u64());
    assert_eq!(pages_of(&small), 2);
    assert_eq!(
        run(&[
            "compress",
            &b,
            "--quality",
            "tiny",
            "-o",
            &s(&dir.join("t.pdf"))
        ])
        .code,
        2
    );
}

#[test]
fn password_set_and_remove_round_trip() {
    let dir = temp("password");
    let locked = dir.join("locked.pdf");
    let r = run_with(
        &[
            "password",
            "set",
            &fx("01-text-outline.pdf"),
            "--new-password-env",
            "KITAB_NEW_PW",
            "-o",
            &s(&locked),
            "--json",
        ],
        &[("KITAB_NEW_PW", "s3cret word")],
        None,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.json()["encrypted"], true);
    assert_eq!(ok_json(&["info", &s(&locked)])["needs_password"], true);
    // Without it, and with a wrong one: exit 4.
    assert_eq!(run(&["text", &s(&locked)]).code, 4);
    let wrong = run(&["info", &s(&locked), "--password", "nope", "--json"]);
    assert_eq!(wrong.code, 4);
    assert_eq!(wrong.json()["code"], "wrong_password");
    // Off again, the password from stdin.
    let open = dir.join("open.pdf");
    let r = run_with(
        &[
            "password",
            "remove",
            &s(&locked),
            "--password-stdin",
            "-o",
            &s(&open),
        ],
        &[],
        Some("s3cret word\n"),
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let info = ok_json(&["info", &s(&open)]);
    assert_eq!(info["encrypted"], false);
    assert_eq!(info["pages"], 40);
    // A new password as a plain argument is refused.
    assert_eq!(
        run(&[
            "password",
            "set",
            &fx("01-text-outline.pdf"),
            "--new-password",
            "x",
            "-o",
            &s(&dir.join("x.pdf")),
        ])
        .code,
        2
    );
}

#[test]
fn pictures_out_and_back_in() {
    let dir = temp("pictures");
    let pics = dir.join("pics");
    let v = ok_json(&["images", &fx("10-scanned-and-images.pdf"), "-o", &s(&pics)]);
    let n = v["images"].as_u64().expect("count");
    assert!(n >= 3, "{v}");
    let files: Vec<String> = v["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect();
    assert_eq!(files.len() as u64, n);
    // Not into a folder that has files, without --force.
    assert_eq!(
        run(&["images", &fx("10-scanned-and-images.pdf"), "-o", &s(&pics)]).code,
        5
    );
    let out = dir.join("album.pdf");
    let out_s = s(&out);
    let mut args: Vec<&str> = vec!["images-to-pdf"];
    args.extend(files.iter().map(String::as_str));
    args.extend(["-o", out_s.as_str()]);
    let v = ok_json(&args);
    assert_eq!(v["pages"], n);
    assert_eq!(pages_of(&out), n);
}

#[test]
fn watermark_and_numbers() {
    let dir = temp("stamps");
    let w = dir.join("w.pdf");
    let v = ok_json(&[
        "watermark",
        &fx("01-text-outline.pdf"),
        "DRAFT",
        "-o",
        &s(&w),
    ]);
    assert_eq!(v["pages"], 40);
    assert!(ok_json(&["search", &s(&w), "DRAFT"])["count"].as_u64() >= Some(40));
    let n = dir.join("n.pdf");
    let v = ok_json(&[
        "number",
        &fx("01-text-outline.pdf"),
        "--start",
        "5",
        "-o",
        &s(&n),
    ]);
    assert_eq!(v["first"], 5);
    assert_eq!(pages_of(&n), 40);
}

/// Width and height from a PNG's IHDR chunk.
fn png_size(path: &Path) -> (u32, u32) {
    let b = std::fs::read(path).expect("png");
    assert_eq!(&b[..8], b"\x89PNG\r\n\x1a\n");
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    (be(16), be(20))
}

#[test]
fn render_writes_a_png_of_the_right_width() {
    let dir = temp("render");
    let p = dir.join("p.png");
    let v = ok_json(&["render", &fx("01-text-outline.pdf"), "-o", &s(&p)]);
    assert_eq!(v["width"], 1200);
    assert_eq!(png_size(&p).0, 1200);
    let p2 = dir.join("p2.png");
    ok_json(&[
        "render",
        &fx("02-mixed-page-sizes.pdf"),
        "--page",
        "2",
        "--width",
        "640",
        "-o",
        &s(&p2),
    ]);
    let (w, h) = png_size(&p2);
    assert_eq!(w, 640);
    assert!(h < w, "page 2 is A4 landscape");
    let p3 = dir.join("p3.png");
    ok_json(&[
        "render",
        &fx("01-text-outline.pdf"),
        "--dpi",
        "72",
        "-o",
        &s(&p3),
    ]);
    assert!((png_size(&p3).0 as i64 - 595).abs() <= 1);
    assert_eq!(
        run(&[
            "render",
            &fx("01-text-outline.pdf"),
            "--page",
            "99",
            "-o",
            &s(&p3),
            "--force"
        ])
        .code,
        1
    );
    assert_eq!(run(&["render", &fx("01-text-outline.pdf")]).code, 2);
}

#[test]
fn annotate_then_list_and_remove() {
    let dir = temp("annotate");
    let src = fx("01-text-outline.pdf");
    let h = dir.join("h.pdf");
    let v = ok_json(&[
        "annotate",
        &src,
        "highlight",
        "--page",
        "17",
        "--text",
        "kitab-needle-17",
        "-o",
        &s(&h),
    ]);
    assert_eq!(v["added"], 1);
    let listed = ok_json(&["annotations", "list", &s(&h)]);
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["annotations"][0]["kind"], "Highlight");
    assert_eq!(listed["annotations"][0]["page"], 17);

    // The kind first works too; a note and a styled text box.
    let n = dir.join("n.pdf");
    ok_json(&[
        "annotate",
        "note",
        &s(&h),
        "--page",
        "1",
        "--at",
        "300,100",
        "--text",
        "Check this",
        "-o",
        &s(&n),
    ]);
    let t = dir.join("t.pdf");
    let colour = format!("#{}", "C00000");
    ok_json(&[
        "annotate",
        &s(&n),
        "textbox",
        "--page",
        "1",
        "--rect",
        "72,600,250,30",
        "--text",
        "Approved",
        "--bold",
        "--align",
        "centre",
        "--color",
        &colour,
        "-o",
        &s(&t),
    ]);
    let listed = ok_json(&["annotations", "list", &s(&t)]);
    assert_eq!(listed["count"], 3);
    let kinds: Vec<&str> = listed["annotations"]
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|a| a["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"Text") && kinds.contains(&"FreeText"),
        "{kinds:?}"
    );

    // Remove one page's, then all.
    let r = dir.join("r.pdf");
    let v = ok_json(&[
        "annotations",
        "remove",
        &s(&t),
        "--page",
        "17",
        "-o",
        &s(&r),
    ]);
    assert_eq!(v["removed"], 1);
    assert_eq!(ok_json(&["annotations", "list", &s(&r)])["count"], 2);
    let clean = dir.join("clean.pdf");
    ok_json(&["annotations", "remove", &s(&t), "--all", "-o", &s(&clean)]);
    assert_eq!(ok_json(&["annotations", "list", &s(&clean)])["count"], 0);

    // A phrase that isn't on the page fails (exit 1) and writes nothing.
    let miss = dir.join("miss.pdf");
    let r = run(&[
        "annotate",
        &src,
        "highlight",
        "--page",
        "1",
        "--text",
        "kitab-needle-17",
        "-o",
        &s(&miss),
    ]);
    assert_eq!(r.code, 1);
    assert!(!miss.exists());
    // The fixture with annotations of every kind.
    let fixture_list = ok_json(&["annotations", "list", &fx("09-annotations.pdf")]);
    assert!(fixture_list["count"].as_u64() >= Some(7), "{fixture_list}");
}

#[test]
fn verify_reports_each_fixture() {
    let status = |file: &str, extra: &[&str]| {
        let mut args = vec!["verify", file];
        args.extend_from_slice(extra);
        ok_json(&args)
    };
    let root = fx("certs/kitab-test-root-ca.pem");
    let a = status(&fx("08a-signed-valid.pdf"), &[]);
    assert_eq!(a["count"], 1);
    assert_eq!(a["signatures"][0]["status"], "signer-unknown");
    assert_eq!(a["signatures"][0]["intact"], true);
    assert_eq!(a["signatures"][0]["covers_whole_document"], true);
    let trusted = status(&fx("08a-signed-valid.pdf"), &["--trust", &root]);
    assert_eq!(trusted["status"], "valid");
    assert!(
        trusted["signatures"][0]["signer"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(trusted["signatures"][0]["time"].is_string());
    let two = status(&fx("08b-signed-two-signers.pdf"), &["--trust", &root]);
    assert_eq!(two["count"], 2);
    assert_eq!(two["status"], "valid");
    assert_eq!(two["signatures"][0]["changed_after"], "form-filling");
    let changed = status(&fx("08c-signed-then-modified.pdf"), &["--trust", &root]);
    assert_eq!(changed["status"], "changed-after-signing");
    let broken = status(&fx("08d-signed-tampered.pdf"), &["--trust", &root]);
    assert_eq!(broken["status"], "invalid");
    let none = status(&fx("01-text-outline.pdf"), &[]);
    assert_eq!(none["count"], 0);
}

#[test]
fn sign_then_verify() {
    let dir = temp("sign");
    let out = dir.join("signed.pdf");
    let r = run_with(
        &[
            "sign",
            &fx("08-lease-unsigned.pdf"),
            "--key",
            &fx("certs/landlord.p12"),
            "--key-password-env",
            "KITAB_KEY_PW",
            "--page",
            "1",
            "--rect",
            "72,700,272,760",
            "--reason",
            "Agreed",
            "-o",
            &s(&out),
            "--json",
        ],
        &[("KITAB_KEY_PW", "kitab")],
        None,
    );
    assert_eq!(r.code, 0, "{} {}", r.stdout, r.stderr);
    assert_eq!(r.json()["field"], "Signature1");
    let v = ok_json(&[
        "verify",
        &s(&out),
        "--trust",
        &fx("certs/kitab-test-root-ca.pem"),
    ]);
    assert_eq!(v["status"], "valid");
    assert_eq!(v["signatures"][0]["reason"], "Agreed");
    // A wrong key password: exit 4.
    let bad = run_with(
        &[
            "sign",
            &fx("08-lease-unsigned.pdf"),
            "--key",
            &fx("certs/landlord.p12"),
            "--key-password-env",
            "KITAB_KEY_PW",
            "--page",
            "1",
            "--rect",
            "72,700,272,760",
            "-o",
            &s(&dir.join("bad.pdf")),
        ],
        &[("KITAB_KEY_PW", "wrong")],
        None,
    );
    assert_eq!(bad.code, 4, "{}", bad.stderr);
}

#[test]
fn text_and_markdown() {
    let dir = temp("export");
    let src = fx("01-text-outline.pdf");
    let r = run(&["text", &src, "--pages", "17"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("kitab-needle-17"));
    assert!(!r.stdout.contains("kitab-needle-18"));
    let v = ok_json(&["text", &src, "--pages", "1-2"]);
    assert_eq!(v["pages"], 2);
    assert!(
        v["text"]
            .as_str()
            .is_some_and(|t| t.contains("kitab-needle-01"))
    );
    let txt = dir.join("all.txt");
    ok_json(&["text", &src, "-o", &s(&txt)]);
    let all = std::fs::read_to_string(&txt).expect("text file");
    assert!(all.contains("kitab-needle-40"));

    let r = run(&["markdown", &src, "--pages", "1", "--page-markers"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.stdout.contains("# "), "a heading: {}", r.stdout);
    assert!(r.stdout.contains("page 1"), "a page marker");
    let md = dir.join("doc.md");
    let v = ok_json(&["markdown", &fx("10-scanned-and-images.pdf"), "-o", &s(&md)]);
    assert!(md.is_file());
    if v["images"].as_u64() > Some(0) {
        assert!(dir.join("doc_images").is_dir());
        let text = std::fs::read_to_string(&md).expect("md");
        assert!(text.contains("doc_images/"), "relative links: {text}");
    }
    let v = ok_json(&["markdown", &src, "--pages", "3", "--no-images"]);
    assert!(
        v["markdown"]
            .as_str()
            .is_some_and(|m| m.contains("kitab-needle-03"))
    );
}

#[test]
fn exit_codes() {
    let dir = temp("exits");
    // 2: usage.
    let r = run(&["mrege", "a.pdf"]);
    assert_eq!(r.code, 2);
    assert!(r.stderr.starts_with("kitab: "), "{}", r.stderr);
    assert!(r.stderr.contains("merge"), "suggests merge: {}", r.stderr);
    assert_eq!(
        run(&["info", &fx("01-text-outline.pdf"), "--bogus"]).code,
        2
    );
    assert_eq!(run(&["info"]).code, 2);
    let j = run(&["search", &fx("01-text-outline.pdf"), "--json"]);
    assert_eq!(j.code, 2);
    assert_eq!(j.json()["ok"], false);
    assert_eq!(j.json()["code"], "usage");
    // 3: not found or unreadable.
    let r = run(&["info", &s(&dir.join("missing.pdf")), "--json"]);
    assert_eq!(r.code, 3);
    assert_eq!(r.json()["code"], "not_found");
    assert_eq!(run(&["info", &fx("05c-not-a-pdf.pdf")]).code, 3);
    // 4: password.
    let r = run(&["text", &fx("04a-encrypted-password-kitab.pdf"), "--json"]);
    assert_eq!(r.code, 4);
    assert_eq!(r.json()["code"], "needs_password");
    assert_eq!(
        run(&[
            "pages",
            "keep",
            &fx("04a-encrypted-password-kitab.pdf"),
            "1",
            "--password",
            "wrong",
            "-o",
            &s(&dir.join("x.pdf")),
        ])
        .code,
        4
    );
    let ok = run_with(
        &[
            "pages",
            "keep",
            &fx("04a-encrypted-password-kitab.pdf"),
            "1",
            "--password-stdin",
            "-o",
            &s(&dir.join("x.pdf")),
        ],
        &[],
        Some("kitab\n"),
    );
    assert_eq!(ok.code, 0, "{}", ok.stderr);
    // 5: output exists.
    let r = run(&[
        "pages",
        "keep",
        &fx("01-text-outline.pdf"),
        "1",
        "-o",
        &s(&dir.join("x.pdf")),
        "--json",
    ]);
    assert_eq!(r.code, 5);
    assert_eq!(r.json()["code"], "output_exists");
    let r = run(&[
        "pages",
        "keep",
        &fx("01-text-outline.pdf"),
        "1",
        "-o",
        &s(&dir.join("x.pdf")),
        "--force",
    ]);
    assert_eq!(r.code, 0);
    // A write command on a book: exit 1.
    let epub = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub/01-epub3-basic.epub");
    assert_eq!(
        run(&[
            "pages",
            "keep",
            &s(&epub),
            "1",
            "-o",
            &s(&dir.join("e.pdf"))
        ])
        .code,
        1
    );
}

#[test]
fn never_overwrites_an_input() {
    let dir = temp("inputs");
    let copy = dir.join("mine.pdf");
    std::fs::copy(fixture("01-text-outline.pdf"), &copy).expect("copy");
    let before = std::fs::read(&copy).expect("read");
    for args in [
        vec!["pages", "keep", "IN", "1", "-o", "IN", "--force"],
        vec!["compress", "IN", "-o", "IN", "--force"],
        vec!["watermark", "IN", "X", "-o", "IN", "--force"],
        vec!["merge", "IN", "IN", "-o", "IN", "--force"],
        vec!["text", "IN", "-o", "IN", "--force"],
        vec![
            "annotate", "IN", "note", "--page", "1", "--at", "9,9", "--text", "x", "-o", "IN",
            "--force",
        ],
    ] {
        let p = s(&copy);
        let args: Vec<&str> = args
            .iter()
            .map(|a| if *a == "IN" { p.as_str() } else { a })
            .collect();
        let r = run(&args);
        assert_eq!(r.code, 2, "{args:?}: {}", r.stderr);
        assert!(r.stderr.contains("input"), "{}", r.stderr);
    }
    assert_eq!(std::fs::read(&copy).expect("read"), before);
}

#[test]
fn agent_skill_prints_and_installs() {
    let r = run(&["agent-skill"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.starts_with("---\nname: kitab\n"), "{}", r.stdout);
    let dir = temp("skill");
    let v = ok_json(&["agent-skill", "--install", &s(&dir)]);
    assert_eq!(v["changed"], true);
    let file = dir.join("kitab").join("SKILL.md");
    assert_eq!(std::fs::read_to_string(&file).expect("skill"), r.stdout);
    assert_eq!(
        ok_json(&["agent-skill", "--install", &s(&dir)])["changed"],
        false
    );
    std::fs::write(&file, "mine").expect("write");
    assert_eq!(run(&["agent-skill", "--install", &s(&dir)]).code, 5);
    assert_eq!(
        ok_json(&["agent-skill", "--install", &s(&dir), "--force"])["changed"],
        true
    );
    // `claude` goes under the home folder.
    let home = temp("home");
    let r = run_with(
        &["agent-skill", "--install", "claude", "--json"],
        &[("HOME", &s(&home)), ("USERPROFILE", &s(&home))],
        None,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(home.join(".claude/skills/kitab/SKILL.md").is_file());
}

#[test]
fn files_without_a_command_start_the_app() {
    // The test binary has no app one folder up, so it says how to open Kitab.
    let r = run(&[&fx("01-text-outline.pdf")]);
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("Kitab"), "{}", r.stderr);
}
