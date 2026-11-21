// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kitab` in a terminal (Windows: bin\kitab.exe).

fn main() {
    std::process::exit(kitab_cli::run(std::env::args_os().skip(1).collect()));
}
