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

Both theme and an icon set can be loaded from a separate file. 
```toml
[theme]
file = "<file>"
[icons]
file = "<file_2>"
```
where `<file>` can be either a filename or a full path and will be checked in this order:

1. If full path given, then use it as is: `/home/foo/custom_theme.toml`
2. If filename given, e.g. "custom_theme.toml", then first check `XDG_CONFIG_HOME/i3status-rust/themes`
3. Then look for it in `~/.local/share/i3status-rust/themes`
4. Otherwise look for it in `/usr/share/i3status-rust/themes`

Notes:
- In case with icon sets, the file should be in `icons` subdirectory instead of `themes`.
- You can omit the `.toml` extension while specifying `file` parameter.
- `file` parameter is an alias to `name`, they are completely interchangeable.
- All the standard themes are provides in files, so you can take them as examples of how to write your own themes/icon sets.

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
* `semi-native` (like native but with background)
//TODO add an image
* `nord-dark` (polar night)
![nord-dark](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/nord-dark.png)
* `dracula`
![dracula](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/dracula.png)

# Available icon sets

* `none` (default. Uses text labels instead of icons)
* `awesome` (Font Awesome 4.x)
* `awesome5` (Font Awesome 5.x)
* `material`
* `material-nf` (Any font from Nerd Fonts collection)

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

Besides global overrides you may also use per-block overrides using the `theme_overrides`, `icons_overrides` and `icons_format` options available for all blocks.
For example:
```toml
[[block]]
block = "cpu"
icons_format = "{icon}" # Remove spaces aroud icons for this block.
[block.theme_overrides]
idle_bg = "#123456"
idle_fg = "#abcdef"
[block.icons_overrides]
cpu_boost_on = "ON"
cpu_boost_off = "OFF"
```

# Available theme overrides

All `bg` and `fg` overrides are html hex color codes like `#000000` or `#789ABC`. A fourth byte for alpha (like `#acbdef42`) works on some systems. `00` is transparent, `FF` is opaque.

The tints are added to every second block counting from the right. They will therefore always brighten the block and never darken it. The alpha channel, if it works, can also be alternated in the same way.

Feel free to take a look at the provided color schemes for reference.

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

These can be directly set to a string containing the desired unicode codepoint(s) or use a TOML escape sequence like `"\uf0f3"` for up to 4-nibble codepoints and `"\U0001f312"` for up to 8-nibble codepoints.

You can find the codepoints in the documentation of the icon font you're using.

Feel free to take a look at the provided icon mappings for reference.

* `backlight_empty`
* `backlight_full`
* `backlight_1`
* `backlight_2`
* `backlight_3`
* `backlight_4`
* `backlight_5`
* `backlight_6`
* `backlight_7`
* `backlight_8`
* `backlight_9`
* `backlight_10`
* `backlight_11`
* `backlight_12`
* `backlight_13`
* `bat_charging`
* `bat_discharging`
* `bat_full`
* `bat`
* `cogs`
* `cpu`
* `gpu`
* `disk_drive`
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
