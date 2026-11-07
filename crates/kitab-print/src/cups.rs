// SPDX-License-Identifier: AGPL-3.0-or-later

//! Printing through CUPS, the print system of macOS and Linux: the printers
//! come from `lpstat`, and a PDF of the chosen pages goes to `lp` with the
//! sheet's choices. The parsing and the arguments are plain functions, tested
//! on every OS; running the commands is only compiled where CUPS is.

use crate::{PrinterInfo, Scale};

/// `lpstat -e`: one printer name per line.
pub fn parse_printers(lpstat_e: &str, lpstat_d: &str) -> Vec<PrinterInfo> {
    let default = parse_default(lpstat_d);
    lpstat_e
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains(char::is_whitespace))
        .map(|name| PrinterInfo {
            name: name.to_string(),
            is_default: default.as_deref() == Some(name),
        })
        .collect()
}

/// `lpstat -d`: "system default destination: NAME", or none.
pub fn parse_default(lpstat_d: &str) -> Option<String> {
    lpstat_d
        .lines()
        .find_map(|l| l.split_once("default destination:"))
        .map(|(_, name)| name.trim().to_string())
        .filter(|n| !n.is_empty() && !n.contains(char::is_whitespace))
}

/// The `lp` arguments for one job. Printer names come from `lpstat`; titles
/// are passed as single arguments (no shell), so nothing in them is run.
pub fn lp_args(
    printer: &str,
    pdf: &std::path::Path,
    title: &str,
    copies: u32,
    scale: Scale,
    auto_rotate: bool,
) -> Vec<String> {
    let mut args = vec![
        "-d".to_string(),
        printer.to_string(),
        "-n".to_string(),
        copies.clamp(1, 99).to_string(),
        "-t".to_string(),
        title
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect(),
        "-o".to_string(),
        match scale {
            Scale::Fit => "print-scaling=fit",
            Scale::Actual => "print-scaling=none",
            Scale::Shrink => "print-scaling=auto-fit",
        }
        .to_string(),
    ];
    if !auto_rotate {
        // Keep pages the way they are: no turning to match the paper.
        args.push("-o".into());
        args.push("nofit-to-page".into());
    }
    args.push("--".into());
    args.push(pdf.to_string_lossy().to_string());
    args
}

#[cfg(not(windows))]
pub(crate) mod run {
    use super::*;
    use std::process::Command;

    pub fn printers() -> Result<Vec<PrinterInfo>, String> {
        let names = Command::new("lpstat").arg("-e").output().map_err(|_| {
            "CUPS isn't available (no lpstat), so Kitab can't find printers".to_string()
        })?;
        let default = Command::new("lpstat").arg("-d").output().ok();
        let list = parse_printers(
            &String::from_utf8_lossy(&names.stdout),
            &default
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default(),
        );
        if list.is_empty() {
            return Err("No printers are set up on this computer.".into());
        }
        Ok(list)
    }

    pub fn print_pdf(
        printer: &str,
        pdf: &std::path::Path,
        title: &str,
        copies: u32,
        scale: Scale,
        auto_rotate: bool,
    ) -> Result<(), String> {
        let out = Command::new("lp")
            .args(lp_args(printer, pdf, title, copies, scale, auto_rotate))
            .output()
            .map_err(|_| "CUPS isn't available (no lp)".to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            let why = String::from_utf8_lossy(&out.stderr);
            Err(format!("the printer refused the job: {}", why.trim()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printers_and_the_default_are_read_from_lpstat() {
        let list = parse_printers(
            "Office_Laser\nHome_Inkjet\n\n",
            "system default destination: Home_Inkjet\n",
        );
        assert_eq!(list.len(), 2);
        assert!(list[1].is_default && !list[0].is_default);
        assert_eq!(parse_default("no system default destination\n"), None);
        // Some systems say it differently.
        assert_eq!(
            parse_default("default destination: Office_Laser"),
            Some("Office_Laser".into())
        );
    }

    #[test]
    fn lp_gets_one_argument_per_value() {
        let a = lp_args(
            "Office_Laser",
            std::path::Path::new("/tmp/kitab print.pdf"),
            "Report; rm -rf ~\n",
            0,
            Scale::Shrink,
            false,
        );
        assert_eq!(&a[..4], ["-d", "Office_Laser", "-n", "1"]);
        // The title is one argument, control characters dropped.
        assert_eq!(a[5], "Report; rm -rf ~");
        assert!(a.contains(&"print-scaling=auto-fit".to_string()));
        assert!(a.contains(&"nofit-to-page".to_string()));
        // The file comes last, after "--", as one argument with its space.
        assert_eq!(a[a.len() - 2], "--");
        assert!(a.last().is_some_and(|p| p.ends_with("kitab print.pdf")));
    }
}
