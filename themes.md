The bar can be themed either by specifying a pre-complied theme or overriding defaults in the configuration.  
We differentiate between themes and icon sets.

## Choosing your theme and icon set
To use a theme or icon set other than the default, add them to your configuration file like so:
```toml
theme = "solarized-dark"
icons = "awesome"
```
# Available themes:
* `plain` (default)
* `solarized-dark`
* `solarized-light`
* `slick`
* `modern`
* `bad-wolf`
* `gruvbox-light`
* `gruvbox-dark`

# Available icon sets:
* `none` (default)
* `awesome`
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
* `idle_bg`
* `idle_fg`
* `info_bg`
* `info_fg`
* `good_bg`
* `good_fg`
* `warning_bg`
* `warning_fg`
* `critical_bg`
* `critical_fg`
* `separator`
* `separator_bg`
* `separator_fg`
* `alternating_tint_bg`
* `alternating_tint_fg`

# Available icon overrides
* `time`
* `music`
* `music_play`
* `music_pause`
* `music_next`
* `music_prev`
* `cogs`
* `memory_mem`
* `memory_swap`
* `cpu`
* `bat`
* `bat_full`
* `bat_charging`
* `bat_discharging`
* `update`
* `toggle_off`
* `toggle_on`
* `volume_full`
* `volume_half`
* `volume_empty`
* `volume_muted`
* `thermometer`
* `xrandr`
* `net_up`
* `net_down`
* `net_wireless`
* `net_wired`
* `ping`
* `backlight_empty`
* `backlight_partial1`
* `backlight_partial2`
* `backlight_partial3`
* `backlight_full`
* `weather_sun`
* `weather_snow`
* `weather_thunder`
* `weather_clouds`
* `weather_rain`
* `weather_default`
* `uptime`
* `gpu`
* `mail`
