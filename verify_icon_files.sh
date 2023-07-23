#!/bin/bash

TMP="$(mktemp)"
trap 'rm -f -- "$TMP"' EXIT

EXITCODE=0

for f in files/icons/*.toml; do
    echo == Verifying $f ==
    comm -3 <(sed -n '/impl Default for Icons {/, /}/p' src/icons.rs | awk -F '"' '/=>/ {print $2}' | sort) <(awk '!/^#/ && /=/ {print $1}' $f | sort) > $TMP
    if [ -s $TMP ]; then
        echo "Found the following conflicts❗"
        cat $TMP
        EXITCODE=1
    else
        echo "No conflicts found ✅"
    fi
done

exit $EXITCODE
