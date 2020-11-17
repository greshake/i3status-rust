The bar can be themed either by specifying a pre-complied theme or overriding defaults in the configuration.  
We differentiate between themes and icon sets.

## Choosing your theme and icon set
To use a theme or icon set other than the default, add them to your configuration file like so:
```toml
theme = "solarized-dark"
icons = "awesome"
```
NOTE: If you plan on overriding parts of the theme/icon set then you will need to change your config file like so:
```toml
[theme]
name = "solarized-dark"
[icons]
name = "awesome"
```

You can also use your own custom theme:

```toml
[theme]
file = "<file>"
```

where `<file>` can be either a filename or a full path and will be checked in this order:

1. If full path given, then use it as is: `/home/foo/custom_theme.toml`
2. If filename given, e.g. "custom_theme.toml", then first check `XDG_CONFIG_HOME/i3status-rust/themes`
3. Otherwise look for it in `/usr/share/i3status-rust/themes`

Example theme file can be found in `example/theme/solarized-dark.toml`.

# Available themes

* `plain` (default)
![plain](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/plain.png)
* `solarized-dark`
![solarized-dark](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/solarized_dark.png)
* `solarized-light`
![solarized-light](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/solarized_light.png)
* `slick`
![slick](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/slick.png)
* `modern`
![modern](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/modern.png)
* `bad-wolf`
![bad-wolf](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/bad_wolf.png)
* `gruvbox-light`
![gruvbox-light](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/gruvbox_light.png)
* `gruvbox-dark`
![gruvbox-dark](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/gruvbox_dark.png)
* `space-villain`
![space-villain](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/space_villain.png)
* `native` (like plain with no background and native separators)
![native](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/native.png)

# Available icon sets

* `none` (default. Uses text labels instead of icons)
* `awesome` (Font Awesome 4.x)
* `awesome5` (Font Awesome 5.x)
* `material`

> **Note**: In order to use the material icon set, you need a patched material icons font which can be found [here](https://gist.github.com/draoncc/3c20d8d4262892ccd2e227eefeafa8ef/raw/3e6e12c213fba1ec28aaa26430c3606874754c30/MaterialIcons-Regular-for-inline.ttf). Make sure to pass it in your i3 configuration bar block.

## Overriding themes and icon sets

Create a block in the configuration called `theme` or `icons` like so:

```toml
[theme]
name = "solarized-dark"
[theme.overrides]
idle_bg = "#123456"
idle_fg = "#abcdef"

[icons]
name = "awesome"
[icons.overrides]
bat = " | | "
bat_full = " |X| "
bat_charging = " |^| "
bat_discharging = " |v| "
```

Example configurations can be found as `example_theme.toml` and `example_icon.toml`.

# Available theme overrides

* `alternating_tint_bg`
* `alternating_tint_fg`
* `critical_bg`
* `critical_fg`
* `good_bg`
* `good_fg`
* `idle_bg`
* `idle_fg`
* `info_bg`
* `info_fg`
* `separator_bg`
* `separator_fg`
* `separator`
* `warning_bg`
* `warning_fg`

# Available icon overrides

* `backlight_empty`
* `backlight_full`
* `backlight_partial1`
* `backlight_partial2`
* `backlight_partial3`
* `bat_charging`
* `bat_discharging`
* `bat_full`
* `bat`
* `cogs`
* `cpu`
* `gpu`
* `mail`
* `memory_mem`
* `memory_swap`
* `music_next`
* `music_pause`
* `music_play`
* `music_prev`
* `music`
* `net_down`
* `net_up`
* `net_wired`
* `net_wireless`
* `ping`
* `thermometer`
* `time`
* `toggle_off`
* `toggle_on`
* `update`
* `uptime`
* `volume_empty`
* `volume_full`
* `volume_half`
* `volume_muted`
* `weather_clouds`
* `weather_default`
* `weather_rain`
* `weather_snow`
* `weather_sun`
* `weather_thunder`
* `xrandr`
