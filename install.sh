#!/bin/sh
# Use this script when installing via `cargo` in order to be able to use the default icons/themes
mkdir -p ~/.local/share/i3status-rust
cp -r files/* ~/.local/share/i3status-rust/
