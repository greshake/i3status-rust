#!/bin/bash

I3CONF="$HOME/.config/i3/config"
DIR="$HOME/.config/i3status-rust"

#notify-send "Switching theme"

MODE=`cat $DIR/mode.txt`

if [[ $MODE == light* ]]
then

    #notify-send "Switching to dark"

    # Set GTK theme
    gsettings set org.gnome.desktop.interface gtk-theme Adapta


    # Reconfigure i3status-rust
    sed -i 's/^name = "solarized-light"$/name = "slick"/' $DIR/config.toml

    # Reconfigure i3
    sed -i 's/background	#DDDDDD/background	#424242/' $I3CONF

    sleep 0.2

    pkill -SIGUSR2 i3status-rs
    i3-msg reload > /dev/null

    echo dark | tee $DIR/mode.txt

elif [[ $MODE == dark* ]]
then

    #notify-send "Switching to light"

    # Set GTK theme to an invalid one, which is usually light
    gsettings set org.gnome.desktop.interface gtk-theme None

    # Reconfigure i3status-rust
    sed -i 's/^name = "slick"$/name = "solarized-light"/' $DIR/config.toml

    # Reconfigure i3
    sed -i 's/background	#424242/background	#DDDDDD/' $I3CONF

    sleep 0.2

    pkill -SIGUSR2 i3status-rs
    i3-msg reload > /dev/null

    echo light | tee $DIR/mode.txt

    # This needs to be done a bit later for some reason
    sleep 1

    # Set GTK theme to an invalid one, which is usually light
    gsettings set org.gnome.desktop.interface gtk-theme None

fi
