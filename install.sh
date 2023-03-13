#!/bin/sh
# Use this script when installing via `cargo` in order to be able to use the
# default icons/themes. If installed via a package manager you do not need to
# run this script.

# Themes
mkdir -p ~/.local/share/i3status-rust
cp -r files/* ~/.local/share/i3status-rust/

# Manpage
cargo xtask generate-manpage
mkdir -p ~/.local/share/man/man1/
cp man/i3status-rs.1 ~/.local/share/man/man1/i3status-rs.1
