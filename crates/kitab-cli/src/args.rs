// SPDX-License-Identifier: AGPL-3.0-or-later

//! A small argument parser: each command lists the flags it takes and
//! whether they take a value; the global flags are accepted anywhere. Kept
//! by hand so the command stays small and its errors read like the help.

use crate::out::CliError;
use std::collections::HashMap;
use std::ffi::OsString;

/// A flag a command takes: its long name (without dashes), an optional
/// one-letter short name, and whether a value follows it.
#[derive(Debug, Clone, Copy)]
pub struct Flag {
    pub long: &'static str,
    pub short: Option<char>,
    pub value: bool,
}

pub const fn switch(long: &'static str) -> Flag {
    Flag {
        long,
        short: None,
        value: false,
    }
}

pub const fn valued(long: &'static str) -> Flag {
    Flag {
        long,
        short: None,
        value: true,
    }
}

/// Flags every command accepts, anywhere on the line.
pub const GLOBAL: &[Flag] = &[
    switch("json"),
    Flag {
        long: "quiet",
        short: Some('q'),
        value: false,
    },
    switch("force"),
    Flag {
        long: "out",
        short: Some('o'),
        value: true,
    },
    valued("password"),
    valued("password-env"),
    switch("password-stdin"),
    Flag {
        long: "help",
        short: Some('h'),
        value: false,
    },
];

/// A parsed command line: flags by long name (every value given, in order)
/// and the positional arguments.
#[derive(Debug, Default)]
pub struct Parsed {
    flags: HashMap<&'static str, Vec<String>>,
    pub positional: Vec<String>,
}

impl Parsed {
    pub fn has(&self, long: &str) -> bool {
        self.flags.contains_key(long)
    }

    /// The last value given for `long`.
    pub fn value(&self, long: &str) -> Option<&str> {
        self.flags
            .get(long)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    pub fn values(&self, long: &str) -> &[String] {
        self.flags.get(long).map_or(&[], Vec::as_slice)
    }
}

/// Parse `args` against the command's `flags` plus [`GLOBAL`]. `--` ends
/// the flags; `--name=value` and `-o value` both work. A lone `-` is a
/// positional argument.
pub fn parse(args: &[OsString], flags: &[Flag], command: &str) -> Result<Parsed, CliError> {
    let all: Vec<Flag> = flags.iter().chain(GLOBAL).copied().collect();
    let mut parsed = Parsed::default();
    let mut rest_positional = false;
    let mut it = args.iter();
    while let Some(raw) = it.next() {
        let arg = raw.to_str().ok_or_else(|| {
            CliError::usage(format!(
                "an argument isn't valid Unicode: {}",
                raw.to_string_lossy()
            ))
        })?;
        if rest_positional || arg == "-" || !arg.starts_with('-') || is_number(arg) {
            parsed.positional.push(arg.to_string());
            continue;
        }
        if arg == "--" {
            rest_positional = true;
            continue;
        }
        let (name, inline) = match arg.strip_prefix("--") {
            Some(long) => match long.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (long.to_string(), None),
            },
            None => {
                let short = &arg[1..];
                let mut chars = short.chars();
                let (Some(c), None) = (chars.next(), chars.next()) else {
                    return Err(unknown(arg, command));
                };
                match all.iter().find(|f| f.short == Some(c)) {
                    Some(f) => (f.long.to_string(), None),
                    None => return Err(unknown(arg, command)),
                }
            }
        };
        let Some(flag) = all.iter().find(|f| f.long == name) else {
            return Err(unknown(arg, command));
        };
        let value = if flag.value {
            match inline {
                Some(v) => v,
                None => {
                    let v = it
                        .next()
                        .ok_or_else(|| CliError::usage(format!("--{} needs a value", flag.long)))?;
                    v.to_str()
                        .ok_or_else(|| {
                            CliError::usage(format!("the value of --{} isn't valid Unicode", name))
                        })?
                        .to_string()
                }
            }
        } else {
            if inline.is_some() {
                return Err(CliError::usage(format!("--{} takes no value", flag.long)));
            }
            String::new()
        };
        parsed.flags.entry(flag.long).or_default().push(value);
    }
    Ok(parsed)
}

/// "-90" or "-1.5" is a value, not a flag.
fn is_number(arg: &str) -> bool {
    arg.len() > 1 && arg[1..].chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn unknown(arg: &str, command: &str) -> CliError {
    CliError::usage(format!(
        "unknown option {arg} for '{command}' (see: kitab help {command})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn flags_anywhere_values_and_positionals() {
        let p = parse(
            &os(&["a.pdf", "--by", "90", "-o", "x.pdf", "--json", "1-3"]),
            &[valued("by")],
            "pages",
        )
        .expect("parses");
        assert_eq!(p.positional, vec!["a.pdf", "1-3"]);
        assert_eq!(p.value("by"), Some("90"));
        assert_eq!(p.value("out"), Some("x.pdf"));
        assert!(p.has("json"));
        let p = parse(&os(&["--out=y.pdf", "--", "--odd"]), &[], "x").expect("parses");
        assert_eq!(p.value("out"), Some("y.pdf"));
        assert_eq!(p.positional, vec!["--odd"]);
    }

    #[test]
    fn mistakes_are_usage_errors() {
        assert!(parse(&os(&["--nope"]), &[], "x").is_err());
        assert!(parse(&os(&["-o"]), &[], "x").is_err());
        assert!(parse(&os(&["--json=1"]), &[], "x").is_err());
        assert!(parse(&os(&["-xyz"]), &[], "x").is_err());
    }
}
