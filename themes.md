The bar can be themed either by specifying a pre-complied theme or overwriting defaults in the configuration.  
We differentiate between themes and icon sets.

In order to change either, add them to your configuration:

```toml
theme = "solarized-dark"
icons = "awesome"
```

Available themes are: `plain`, `solarized-dark` and `slick`.  
Available icon sets are: `none`, `awesome`, `material`.

> **Note**: In order to use the material icon set, you need a patched material icons font which can be found [here](https://gist.github.com/draoncc/3c20d8d4262892ccd2e227eefeafa8ef/raw/3e6e12c213fba1ec28aaa26430c3606874754c30/MaterialIcons-Regular-for-inline.ttf). Make sure to pass it in your i3 configuration bar block.

## Overwriting themes and icon sets
Create a block in the configuration called theme or icons like so:

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
Here's a full list of available overrides:

| Theme        		| Icons
| -----    		| -----
| idle_bg		| time
| idle_fg		| music
| info_bg		| music_play
| info_fg		| music_pause
| good_bg		| music_next
| good_fg		| music_prev
| warning_bg		| cogs
| warning_fg		| memory_mem
| critical_bg		| memory_swap
| critical_fg		| cpu
| separator		| bat
| separator_bg		| bat_full
| separator_fg		| bat_charging
| alternating_tint_bg	| bat_discharging
| alternating_tint_fg	| update
|              		| toggle_off
|              		| toggle_on
|              		| volume_full
|            	 	| volume_half
|			| volume_empty
|            		| volume_muted
|             		| thermometer
|             		| xrandr
|             		| net_up
|             		| net_down
|             		| netw_wireless
|             		| net_wired
|             		| ping
|            		| backlight_empty
|             		| backlight_partial1
|             		| backlight_partial2
|            		| backlight_partial3
|           		| backlight_full
|          		| weather_sun
|            		| weather_snow
|           		| weather_thunder
|             		| weather_clouds
|            		| weather_rain
|            		| weather_default
|            		| uptime
|             		| gpu
|              		| mail
