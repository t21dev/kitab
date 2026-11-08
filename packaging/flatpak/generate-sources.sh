#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# cargo-sources.json: every crate in Cargo.lock, so flatpak-builder can build
# offline (as Flathub requires). Needs python3 with aiohttp and tomlkit.
set -e
here=$(cd "$(dirname "$0")" && pwd)
gen="$here/flatpak-cargo-generator.py"
# flatpak-builder-tools, pinned.
rev=41c20aa10819cdb2a4f3ca171758a96d1955c018
if [ ! -f "$gen" ]; then
  curl -fsSL -o "$gen" "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$rev/cargo/flatpak-cargo-generator.py"
fi
python3 "$gen" "$here/../../Cargo.lock" -o "$here/cargo-sources.json"
echo "wrote $here/cargo-sources.json"
