#!/bin/bash

set -eo pipefail

if ! pandoc --version | grep -q "pandoc 3" -; then
    echo "Please make sure you are have pandoc version 3"
    exit 1
fi

if [ -z $1 ]; then
    OUT=man/i3status-rs.1
else
    OUT=$1
fi

(cd gen-manpage && cargo run -- ../src ../man/blocks.md)

pandoc -o man/blocks.1 -t man man/blocks.md
# TODO: fix deprecation warning
pandoc -o man/themes.1 -t man --base-header-level=2 doc/themes.md

# Add appropriate section headers.
sed -i '1i .SH BLOCKS\n' man/blocks.1
sed -i '1i .SH THEMES\n' man/themes.1

# Stitch together the final manpage.
cat man/_preface.1 man/blocks.1 man/themes.1 man/_postface.1 > $OUT

rm man/blocks.md man/blocks.1 man/themes.1
