// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `kitab` command (GitHub: Kitab CLI): read, convert and change
//! documents from a terminal, a script or an AI agent. Every command the
//! app's own tools offer, with `--json` output and stable exit codes.
//!
//! On Windows this is `bin\kitab.exe` beside the app; on macOS and Linux the
//! app's own binary calls [`is_command`] and [`run`]. Agents are the main
//! audience, so the output is a contract: see `out.rs` and the help.

mod args;
mod cx;
mod help;
mod launch;
mod marks;
mod out;
mod read;
mod skill;
mod write;

use args::{Flag, switch, valued};
use std::ffi::{OsStr, OsString};
use std::path::Path;

/// One command: its name, the flags it takes besides the global ones, and
/// what runs it.
struct Command {
    name: &'static str,
    flags: &'static [Flag],
    run: fn(&mut cx::Cx) -> out::Result<()>,
}

const PAGES_FLAG: Flag = valued("pages");

const COMMANDS: &[Command] = &[
    Command {
        name: "info",
        flags: &[],
        run: read::info,
    },
    Command {
        name: "text",
        flags: &[PAGES_FLAG],
        run: read::text,
    },
    Command {
        name: "markdown",
        flags: &[
            PAGES_FLAG,
            valued("images"),
            switch("no-images"),
            switch("page-markers"),
        ],
        run: read::markdown,
    },
    Command {
        name: "search",
        flags: &[switch("case"), switch("whole-word"), valued("limit")],
        run: read::search,
    },
    Command {
        name: "render",
        flags: &[valued("page"), valued("width"), valued("dpi")],
        run: read::render,
    },
    Command {
        name: "annotations",
        flags: &[switch("all"), valued("page"), valued("index")],
        run: marks::annotations,
    },
    Command {
        name: "verify",
        flags: &[valued("trust")],
        run: read::verify,
    },
    Command {
        name: "pages",
        flags: &[
            valued("by"),
            valued("before"),
            valued("from"),
            valued("from-pages"),
        ],
        run: write::pages,
    },
    Command {
        name: "merge",
        flags: &[],
        run: write::merge,
    },
    Command {
        name: "split",
        flags: &[valued("every"), valued("ranges")],
        run: write::split,
    },
    Command {
        name: "compress",
        flags: &[valued("quality")],
        run: write::compress,
    },
    Command {
        name: "password",
        flags: &[
            valued("new-password-env"),
            switch("new-password-stdin"),
            valued("new-password"),
        ],
        run: write::password,
    },
    Command {
        name: "images-to-pdf",
        flags: &[],
        run: write::images_to_pdf,
    },
    Command {
        name: "images",
        flags: &[],
        run: write::images,
    },
    Command {
        name: "watermark",
        flags: &[],
        run: write::watermark,
    },
    Command {
        name: "number",
        flags: &[valued("start")],
        run: write::number,
    },
    Command {
        name: "annotate",
        flags: &[
            valued("page"),
            valued("text"),
            switch("all"),
            valued("at"),
            valued("rect"),
            valued("size"),
            switch("bold"),
            switch("italic"),
            switch("underline"),
            switch("strike"),
            valued("align"),
            valued("color"),
            valued("colour"),
            valued("opacity"),
        ],
        run: marks::annotate,
    },
    Command {
        name: "sign",
        flags: &[
            valued("key"),
            valued("key-password-env"),
            switch("key-password-stdin"),
            valued("key-password"),
            valued("page"),
            valued("rect"),
            valued("reason"),
            valued("location"),
        ],
        run: marks::sign,
    },
    Command {
        name: "agent-skill",
        flags: &[valued("install")],
        run: skill::agent_skill,
    },
];

fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// Whether `first` (the first argument) names a command, so the caller runs
/// the CLI instead of opening the app.
///
/// True exactly for the command names and `help`, `--help`, `-h`. A path is
/// never a command: anything with a separator or an extension, and a file
/// that exists under a command's name (`kitab merge` in a folder holding a
/// file called "merge" opens that file). The app's own flags (`--version`,
/// `--print`, `--thumbnail`...) are not commands.
pub fn is_command(first: &OsStr) -> bool {
    let Some(s) = first.to_str() else {
        return false;
    };
    if matches!(s, "--help" | "-h") {
        return true;
    }
    let named = s == "help" || find(s).is_some();
    named && !Path::new(s).is_file()
}

/// Run the command line `args` (without the program name). Returns the exit
/// code.
pub fn run(args: Vec<OsString>) -> i32 {
    // `kitab --json info x.pdf`: global switches may come before the command.
    let lead = args
        .iter()
        .take_while(|a| {
            a.to_str()
                .is_some_and(|s| matches!(s, "--json" | "-q" | "--quiet" | "--force"))
        })
        .count();
    let Some(first) = args.get(lead) else {
        if lead == 0 {
            return launch::open_app(&args);
        }
        eprintln!("kitab: missing command (see: kitab --help)");
        return out::EXIT_USAGE;
    };
    let first_str = first.to_string_lossy().to_string();
    if lead == 0 {
        match first_str.as_str() {
            "--version" | "-V" | "version" => {
                println!("kitab {}", env!("CARGO_PKG_VERSION"));
                return out::EXIT_OK;
            }
            "--help" | "-h" | "help" => return help_command(&args[1..]),
            _ => {}
        }
    }
    let rest: Vec<OsString> = args[..lead]
        .iter()
        .chain(&args[lead + 1..])
        .cloned()
        .collect();
    if let Some(cmd) = find(&first_str).filter(|_| is_command(first)) {
        return run_command(cmd, &rest);
    }
    if lead == 0 && opens_app(&args) {
        return launch::open_app(&args);
    }
    let hint = suggest(&first_str)
        .map(|s| format!(" Did you mean '{s}'?"))
        .unwrap_or_default();
    let e = out::CliError::usage(format!(
        "unknown command '{first_str}'.{hint} Run `kitab --help` for the list."
    ));
    let o = out::Out {
        json: args.iter().any(|a| a == "--json"),
        quiet: false,
    };
    o.error(&first_str, &e);
    e.exit
}

/// Arguments the app takes: files to open and its own flags.
fn opens_app(args: &[OsString]) -> bool {
    const APP_FLAGS: &[&str] = &["--print", "--thumbnail", "--unregister", "--stdin"];
    let first = args.first().map(|a| a.to_string_lossy().to_string());
    if first.as_deref().is_some_and(|f| APP_FLAGS.contains(&f)) {
        return true;
    }
    args.iter().all(|a| {
        let s = a.to_string_lossy();
        if s.starts_with("--") {
            return true;
        }
        looks_like_path(&s)
    })
}

/// A file name rather than a mistyped command: it has a separator or an
/// extension, or it exists.
fn looks_like_path(s: &str) -> bool {
    let p = Path::new(s);
    s.contains(['/', '\\']) || p.extension().is_some() || p.exists()
}

/// The command closest to a mistyped one, if any is close.
fn suggest(typed: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .map(|c| c.name)
        .chain(["help"])
        .map(|name| (distance(typed, name), name))
        .filter(|(d, name)| *d <= 2.max(name.len() / 4))
        .min_by_key(|(d, _)| *d)
        .map(|(_, name)| name)
}

/// Edit distance between two short words.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

fn help_command(args: &[OsString]) -> i32 {
    let topic = args
        .iter()
        .map(|a| a.to_string_lossy().to_string())
        .find(|a| !a.starts_with('-'));
    let asks_about_help = args.iter().any(|a| a == "--help" || a == "-h");
    match topic {
        None if asks_about_help => {
            print!("{}", help::for_command("help").unwrap_or(help::OVERVIEW));
            out::EXIT_OK
        }
        None => {
            print!("{}", help::OVERVIEW);
            out::EXIT_OK
        }
        Some(t) => match help::for_command(&t) {
            Some(text) => {
                print!("{text}");
                out::EXIT_OK
            }
            None => {
                let hint = suggest(&t)
                    .map(|s| format!(" Did you mean '{s}'?"))
                    .unwrap_or_default();
                eprintln!("kitab: no help for '{t}'.{hint} Run `kitab --help` for the list.");
                out::EXIT_USAGE
            }
        },
    }
}

fn run_command(cmd: &Command, args: &[OsString]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let o = out::Out { json, quiet: false };
    let parsed = match args::parse(args, cmd.flags, cmd.name) {
        Ok(p) => p,
        Err(e) => {
            o.error(cmd.name, &e);
            return e.exit;
        }
    };
    if parsed.has("help") {
        print!("{}", help::for_command(cmd.name).unwrap_or(help::OVERVIEW));
        return out::EXIT_OK;
    }
    let mut cx = cx::Cx::new(cmd.name, parsed);
    match (cmd.run)(&mut cx) {
        Ok(()) => out::EXIT_OK,
        Err(e) => {
            cx.out.error(&cx.command, &e);
            e.exit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_help() {
        for c in COMMANDS {
            assert!(help::for_command(c.name).is_some(), "{}", c.name);
            assert!(help::OVERVIEW.contains(c.name), "{}", c.name);
        }
    }

    #[test]
    fn help_lines_fit_100_columns() {
        let all = COMMANDS
            .iter()
            .filter_map(|c| help::for_command(c.name))
            .chain([help::OVERVIEW]);
        for text in all {
            for line in text.lines() {
                assert!(line.chars().count() <= 100, "too long: {line}");
            }
        }
    }

    #[test]
    fn suggestions_for_typos() {
        assert_eq!(suggest("mrege"), Some("merge"));
        assert_eq!(suggest("serach"), Some("search"));
        assert_eq!(suggest("zzzzzzzz"), None);
    }
}
