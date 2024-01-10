#!/bin/sh
# Use this script when installing via `cargo` in order to be able to use the
# default icons/themes. If installed via a package manager you do not need to
# run this script.

set -x

XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}

# Themes
mkdir -p $XDG_DATA_HOME/i3status-rust
cp -r files/* $XDG_DATA_HOME/i3status-rust/

# Manpage
cargo xtask generate-manpage
mkdir -p $XDG_DATA_HOME/man/man1/
cp man/i3status-rs.1 $XDG_DATA_HOME/man/man1/i3status-rs.1
