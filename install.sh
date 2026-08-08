#!/bin/sh
# Use this script when installing via `cargo` in order to be able to use the
# default icons/themes. If installed via a package manager you do not need to
# run this script.

set -x

XDG_DATA_HOME=${XDG_DATA_HOME:-$HOME/.local/share}

# Themes
mkdir -p $XDG_DATA_HOME/i3status-rust
cp -r files/* $XDG_DATA_HOME/i3status-rust/

# Icon font. This must not go under i3status-rust/: fontconfig only looks in
# /usr/share/fonts, /usr/local/share/fonts, $XDG_DATA_HOME/fonts and ~/.fonts,
# so a font installed alongside the themes would never be found.
mkdir -p $XDG_DATA_HOME/fonts
cp fonts/i3status-icons/i3status-icons.ttf $XDG_DATA_HOME/fonts/
if command -v fc-cache >/dev/null 2>&1; then
	fc-cache -f $XDG_DATA_HOME/fonts
fi

# Manpage
cargo xtask generate-manpage
mkdir -p $XDG_DATA_HOME/man/man1/
cp man/i3status-rs.1 $XDG_DATA_HOME/man/man1/i3status-rs.1
