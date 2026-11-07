// SPDX-License-Identifier: AGPL-3.0-or-later

//! `kitab_shell.dll`: Kitab's Windows Explorer thumbnail provider (PLAN.md
//! §3.9 "Thumbnails").
//!
//! The DLL holds no document code. Explorer hands it the file as a stream; it
//! pipes the bytes to `kitab.exe --thumbnail --stdin EXT --bgra` (the kitab.exe
//! next to the DLL), waits at most [`TIMEOUT`], and turns the BGRA it gets back
//! into a bitmap. So MuPDF ships once (no second copy against the installer
//! budget), and a hostile file can only take a short-lived kitab.exe down,
//! never Explorer or its thumbnail host.
//!
//! Kitab registers the provider under its own ProgIDs only (`kitab-os::assoc`),
//! so it never replaces another reader's thumbnails.

#[cfg(windows)]
mod provider;

#[cfg(windows)]
pub use provider::*;
