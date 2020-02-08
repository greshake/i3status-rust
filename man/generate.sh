#!/bin/bash

set -eo pipefail

pandoc -o man/blocks.1 -t man blocks.md
pandoc -o man/themes.1 -t man --base-header-level=2 themes.md

# Delete the table of contents from the block documentation.
sed -i '0,/Xrandr/d' man/blocks.1

# Add appropriate section headers.
sed -i '1i .SH BLOCKS\n' man/blocks.1
sed -i '1i .SH THEMES\n' man/themes.1

# Stich together the final manpage.
cat man/_preface.1 man/blocks.1 man/themes.1 man/_postface.1 > man/i3status-rs.1

rm man/blocks.1 man/themes.1
