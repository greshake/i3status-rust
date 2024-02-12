#!/usr/bin/env fish

# Run from this directory!

# TODO: posix or rustify script?

# Screenshot area depends on the current monitor's position and size, and we only want the relevant part of the bar
swaymsg -t get_outputs | jq --raw-output ".. | objects | select(.focused == true) | .rect | \"\(.x) \(.y) \(.width)\"" | read x y w
set BAR_COORDS (math $w/2 + $x)",$y "(math $w/2)"x16"

SWAYSOCK=sway_i3rs.sock I3RS_PWD=(pwd) sway --config swayconfig_i3rs &
sleep 1;
swaymsg fullscreen toggle

for theme in ../files/themes/*
     set theme_name (string replace '.toml' '' -- (string replace '../files/themes/' '' -- $theme))
     sed -i -r "s/theme = .*/theme = \"$theme_name\"/" screenshot_config.toml
     pkill -SIGUSR2 i3status-rs
     sleep 1
     grim -g $BAR_COORDS ../img/themes/"$theme_name".png
end

swaymsg fullscreen toggle
swaymsg --socket ./sway_i3rs.sock exit
