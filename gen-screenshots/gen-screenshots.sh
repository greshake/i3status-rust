#!/bin/bash

cd "$(dirname "$0")"

SOCK_FILE=sway_i3rs.sock

set_theme() {
     theme_name=$1
     sed -i -r "s/theme = .*/theme = \"$theme_name\"/" screenshot_config.toml
}

cleanup() {
     # Reset theme so that config is not changed
     set_theme srcery
     # Remove the socket file
     rm -f $SOCK_FILE
}

trap cleanup EXIT

# Screenshot area depends on the current monitor's position and size, and we only want the relevant part of the bar
read x y w <<< $(swaymsg -t get_outputs | jq --raw-output ".. | objects | select(.focused == true) | .rect | \"\(.x) \(.y) \(.width)\"")
BAR_COORDS="$(($w/2 + $x)),$y $(($w/2))x16"

swaymsg fullscreen off
SWAYSOCK=$SOCK_FILE I3RS_PWD=$PWD sway --config swayconfig_i3rs &
sleep 1
swaymsg fullscreen toggle

for theme in ../files/themes/*; do
     theme_name=$(basename $theme .toml)
     if [ -f ../img/themes/"$theme_name".png ]; then
          echo Image for theme $theme_name already exists, skipping
          continue
     fi
     set_theme $theme_name
     pkill -SIGUSR2 i3status-rs
     sleep 1
     grim -g "$BAR_COORDS" ../img/themes/"$theme_name".png
done

SWAYSOCK=$SOCK_FILE swaymsg exit

