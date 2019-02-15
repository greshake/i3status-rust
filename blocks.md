# List of Available Blocks

- [Backlight](#backlight)
- [Battery](#battery)
- [CPU Utilization](#cpu-utilization)
- [Custom](#custom)
- [Disk Space](#disk-space)
- [Focused Window](#focused-window)
- [Load](#load)
- [Maildir](#maildir)
- [Memory](#memory)
- [Music](#music)
- [Net](#net)
- [Nvidia Gpu](#nvidia-gpu)
- [Pacman](#pacman)
- [Sound](#sound)
- [Speed Test](#speed-test)
- [Temperature](#temperature)
- [Time](#time)
- [Toggle](#toggle)
- [Weather](#weather)
- [Xrandr](#xrandr)

## Backlight

Creates a block to display screen brightness. This is a simplified version of the [Xrandr](#xrandr) block that reads brightness information directly from the filesystem, so it works under Wayland. The block uses `inotify` to listen for changes in the device's brightness directly, so there is no need to set an update interval.

When there is no `device` specified, this block will display information from the first device found in the `/sys/class/backlight` directory. If you only have one display, this approach should find it correctly.

It is possible to set the brightness using this block as well -- [see below](#setting-brightness-with-the-mouse-wheel) for details.

### Examples

Show brightness for a specific device:

```toml
[[block]]
block = "backlight"
device = "intel_backlight"
```

Show brightness for the default device:

```toml
[[block]]
block = "backlight"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The `/sys/class/backlight` device to read brightness information from. | No | Default device
`step_width` | The brightness increment to use when scrolling, in percent. | No | `5`

### Setting Brightness with the Mouse Wheel

The block allows for setting brightness with the mouse wheel. However, depending on how you installed i3status-rust, it may not have the appropriate permissions to modify these files, and will fail silently. To remedy this you can write a `udev` rule for your system (if you are comfortable doing so).

First, check that your user is a member of the "video" group using the `groups` command. Then add a rule in the `/etc/udev/rules.d/` directory containing the following, for example in `backlight.rules`:

```
ACTION=="add", SUBSYSTEM=="backlight", KERNEL=="acpi_video0", RUN+="/bin/chgrp video /sys/class/backlight/%k/brightness"
ACTION=="add", SUBSYSTEM=="backlight", KERNEL=="acpi_video0", RUN+="/bin/chmod g+w /sys/class/backlight/%k/brightness"
```

You will need to ensure that the value of the `KERNEL` parameter here is the same as the `device` used to configure the block. (You will also need to restart for this rule to take effect.)

## Battery

Creates a block which displays the current battery state (Full, Charging or Discharging), percentage charged and estimate time until (dis)charged.

The battery block collapses when the battery is fully charged -- or, in the case of some Thinkpad batteries, when it reports "Not charging".

The battery block supports reading charging and status information from `sysfs`, or optionally through the [Upower](https://upower.freedesktop.org/) D-Bus interface on systems where that is available.

### Examples

Update the battery state every ten seconds, and show the time remaining until (dis)charging is complete:

```toml
[[block]]
block = "battery"
interval = 10
format = "{percentage}% {time}"
```

Rely on Upower for battery updates and information:

```toml
[[block]]
block = "battery"
upower = true
format = "{percentage}% {time}"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The device in `/sys/class/power_supply/` to read from. | No | `"BAT0"`
`interval` | Update interval, in seconds. | No | `10`
`format` | A format string. See below for available placeholders. | No | `"{percentage}%"`
`show` | Deprecated in favour of `format`. Show remaining `"time"`, `"percentage"` or `"both"` | No | `"percentage"`
`upower` | When `true`, use the Upower D-Bus interface for battery updates. | No | `false`

The `show` option is deprecated, and will be removed in future versions. In the meantime, it will override the `format` option when present.

### Format string

Placeholder | Description
------------|-------------
`{percentage}` | Battery level, in percent.
`{time}` | Time remaining until (dis)charge is complete.
`{power}` | Power consumption (in watts) by the battery or from the power supply when charging.

## CPU Utilization

Creates a block which displays the overall CPU utilization, calculated from `/proc/stat`.

### Examples

Update CPU usage every second:

```toml
[[block]]
block = "cpu"
interval = 1
frequency = true
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`info` | Minimum usage, where state is set to info. | No | `30`
`warning` | Minimum usage, where state is set to warning. | No | `60`
`critical` | Minimum usage, where state is set to critical. | No | `90`
`interval` | Update interval, in seconds. | No | `1`
`frequency` | Shows avg cpu frequency in GHz | No | `false`

## Custom

Creates a block that display the output of custom shell commands.

### Examples

```toml
[[block]]
block = "custom"
command = "uname"
interval = 100
```

```toml
[[block]]
block = "custom"
cycle = ["echo ON", "echo OFF"]
on_click = "<command>"
interval = 1
```

### Options

Note that `command` and `cycle` are mutually exclusive.

Key | Values | Required | Default
----|--------|----------|--------
`command` | Shell command to execute & display. | No | None
`on_click` | Command to execute when the button is clicked. The command will be passed to whatever is specified in your `$SHELL` variable and - if not set - fallback to `sh`. | No | None
`cycle` | Commands to execute and change when the button is clicked. | No | None
`interval` | Update interval, in seconds. | No | `10`

## Disk Space

Creates a block which displays disk space information.

### Examples

```toml
[[block]]
block = "disk_space"
path = "/"
alias = "/"
info_type = "available"
unit = "GB"
interval = 20
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`path` | Path to collect information from | No | `"/"`
`alias` | Alias that is displayed for path | No | `"/"`
`info_type` | Currently supported options are `available` and `free` | No | `"available"`
`unit` | Unit that is used to display disk space. Options are MB, MiB, GB and GiB | No | `"GB"`
`interval` | Update interval, in seconds. | No | `20`
`show_percentage` | Show percentage of used/available disk space depending on info_type. | No | `false`

## Focused Window

Creates a block which displays the title of the currently focused window. Uses push updates from i3 IPC, so no need to worry about resource usage. The block only updates when the focused window changes title or the focus changes.

### Examples

```toml
[[block]]
block = "focused_window"
max_width = 21
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`max_width` | Truncates titles to this length. | No | `21`

## Load

Creates a block which displays the system load average.

### Examples

Display the 1-minute and 5-minute load averages, updated once per second:

```toml
[[block]]
block = "load"
format = "{1m} {5m}"
interval = 1
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | Format string. You can use the placeholders 1m 5m and 15m, e.g. `"1min avg: {1m}"`. | No | `"{1m}"`
`interval` | Update interval, in seconds. | No | `3`

## Maildir

Creates a block which shows unread mails. Only supports maildir format.

### Examples

```toml
[[block]]
block = "maildir"
interval = 60
inboxes = ["/home/user/mail/local", "/home/user/mail/gmail/Inbox"]
threshold_warning = 1
threshold_critical = 10
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`inboxes` | List of maildir inboxes to look for mails in | Yes | None
`threshold_warning` | Number of unread mails where state is set to warning | No | `1`
`threshold_critical` | Number of unread mails where state is set to critical | No | `10`
`interval` | Update interval, in seconds. | No | `5`

## Memory

Creates a block displaying memory and swap usage.

By default, the format of this module is "<Icon>: {MFm}MB/{MTm}MB({Mp}%)" (Swap values accordingly). That behaviour can be changed within your config.

This module keeps track of both Swap and Memory. By default, a click switches between them.

### Examples

```toml
[[block]]
block = "memory"
format_mem = "{Mum}MB/{MTm}MB({Mup}%)"
format_swap = "{SUm}MB/{STm}MB({SUp}%)"
display_type = "memory"
icons = true
clickable = true
interval = 5
warning_mem = 80
warning_swap = 80
critical_mem = 95
critical_swap = 95
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`format_mem` | Format string for Memory view. All format values are described below. | No | `"{MFm}MB/{MTm}MB({Mp}%)"`
`format_swap` | Format string for Swap view. | No | `"{SFm}MB/{STm}MB({Sp}%)"`
`display_type` | Default view displayed on startup. Options are memory, swap | No | `"memory"`
`icons` | Whether the format string should be prepended with Icons. | No | `true`
`clickable` | Whether the view should switch between memory and swap on click. | No | `true`
`warning_mem` | Percentage of memory usage, where state is set to warning. | No | `80.0`
`warning_swap` | Percentage of swap usage, where state is set to warning. | No | `80.0`
`critical_mem` | Percentage of memory usage, where state is set to critical. | No | `95.0`
`critical_swap` | Percentage of swap usage, where state is set to critical. | No | `95.0`
`interval` | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | `5`

### Format string specification

  Key    | Value
---------|-------
`{MTg}`  | Memory total (GiB).
`{MTm}`  | Memory total (MiB).
`{MAg}`  | Available memory, including cached memory and buffers (GiB).
`{MAm}`  | Available memory, including cached memory and buffers (MiB).
`{MAp}`  | Available memory, including cached memory and buffers (%).
`{MApi}` | Available memory, including cached memory and buffers (%) as integer.
`{MFg}`  | Memory free (GiB).
`{MFm}`  | Memory free (MiB).
`{MFp}`  | Memory free (%).
`{MFpi}` | Memory free (%) as integer.
`{Mug}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (GiB).
`{Mum}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (MiB).
`{Mup}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (%).
`{Mupi}` | Memory used, excluding cached memory and buffers; similar to htop's green bar (%) as integer.
`{MUg}`  | Total memory used (GiB).
`{MUm}`  | Total memory used (MiB).
`{MUp}`  | Total memory used (%).
`{MUpi}` | Total memory used (%) as integer.
`{Cg}`   | Cached memory, similar to htop's yellow bar (GiB).
`{Cm}`   | Cached memory, similar to htop's yellow bar (MiB).
`{Cp}`   | Cached memory, similar to htop's yellow bar (%).
`{Cpi}`  | Cached memory, similar to htop's yellow bar (%) as integer.
`{Bg}`   | Buffers, similar to htop's blue bar (GiB).
`{Bm}`   | Buffers, similar to htop's blue bar (MiB).
`{Bp}`   | Buffers, similar to htop's blue bar (%).
`{Bpi}`  | Buffers, similar to htop's blue bar (%) as integer.
`{STg}`  | Swap total (GiB).
`{STm}`  | Swap total (MiB).
`{SFg}`  | Swap free (GiB).
`{SFm}`  | Swap free (MiB).
`{SFp}`  | Swap free (%).
`{SFpi}` | Swap free (%) as integer.
`{SUg}`  | Swap used (GiB).
`{SUm}`  | Swap used (MiB).
`{SUp}`  | Swap used (%).
`{SUpi}` | Swap used (%) as integer.


## Music

Creates a block which can display the current song title and artist, in a fixed width marquee fashion. Also provides buttons for play/pause, previous and next title.

Supports all music players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes spotify, vlc and many more.

It can be configured to drive a specific (running) player or to automatically discover the currently active one. Most often, you only run one player at a time.

### Examples

Show the currently playing song on Spotify only, with play & next buttons:

```toml
[[block]]
block = "music"
player = "spotify"
buttons = ["play", "next"]
```

Same thing for any compatible player, takes the first active on the bus:

```toml
[[block]]
block = "music"
buttons = ["play", "next"]
```

Start Spotify if the block is clicked whilst it's collapsed:

```toml
[[block]]
block = "music"
on_collapsed_click = "spotify"
```


### Options

Key | Values | Required | Default
----|--------|----------|--------
`player` | Name of the music player. Must be the same name the player is registered with the MediaPlayer2 Interface.  If unset, it will automatically discover the active player.  | Yes | None
`max_width` | Max width of the block in characters, not including the buttons | No | `21`
`marquee` | Bool to specify if a marquee style rotation should be used if the title + artist is longer than max-width | No | `true`
`marquee_interval` | Marquee interval in seconds. This is the delay between each rotation. | No | `10`
`marquee_speed` | Marquee speed in seconds. This is the scrolling time used per character. | No | `0.5`
`buttons` | Array of control buttons to be displayed. Options are prev (previous title), play (play/pause) and next (next title) | No | `[]`
`on_collapsed_click` | Shell command to run when the music block is clicked while collapsed. | No | None

## Net

Creates a block which displays the upload and download throughput for a network interface. Units are in bytes per second (kB/s, MB/s, etc).

### Examples

```toml
[[block]]
block = "net"
device = "wlp2s0"
ssid = true
ip = true
speed_up = false
graph_up = true
interval = 5
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | Network interface to moniter (name from /sys/class/net) | Yes | `lo` (loopback interface)
`ssid` | Display network SSID (wireless only). | No | `false`
`bitrate` | Display connection bitrate. | No | `false`
`ip` | Display connection IP address. | No | `false`
`speed_up` | Display upload speed. | No | `true`
`speed_down` | Display download speed. | No | `true`
`graph_up` | Display a bar graph for upload speed. | No | `false`
`graph_down` | Display a bar graph for download speed. | No | `false`
`interval` | Update interval, in seconds. | No | `1`

## Nvidia Gpu

Proprietary nvidia driver required.

Creates a block which displays the Nvidia GPU utilization, temperature, used and total memory, fan speed, gpu clocks. You can set gpu label, that displayed by default.

Clicking the left button on the icon changes the output of the label to the output of the gpu name. Same with memory: used/total.

Clicking the left button on the fans turns on the mode of changing the speed of the fans using the wheel. Press again to turn off the mode. For this opportunity you need nvidia-settings!

### Examples

```toml
[[block]]
block = "nvidia_gpu"
label = "GT 1030"
show_memory = false
show_clocks = true
interval = 1
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`gpu_id` | GPU id in system | No | 0
`label` | Display custom gpu label | No | ""
`interval` | Update interval, in seconds. | No | `1`
`show_utilization` | Display gpu utilization. In percents. | No | `true`
`show_memory` | Display memory information. | No | `true`
`show_temperature` | Display gpu temperature. | No | `true`
`show_fan_speed` | Display fan speed. | No | `false`
`show_clocks` | Display gpu clocks. | No | `false`

## Pacman

Creates a block which displays the pending updates available on pacman.

### Examples

Update the list of pending updates every ten seconds:

```toml
[[block]]
block = "pacman"
interval = 10
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `600` (10min)

## Sound

Creates a block which displays the volume level (according to PulseAudio or ALSA). Right click to toggle mute, scroll to adjust volume.

Requires a PulseAudio installation or `alsa-utils` for ALSA.

PulseAudio support is a feature and can be turned on (`--features "pulseaudio"`) / off (`--no-default-features`) during build with `cargo`.
If PulseAudio support is enabled the `"auto"` driver will first try to connect to PulseAudio and then fallback to ALSA on error.


Note that if you are using PulseAudio commands (such as `pactl`) to control your volume, you should select the `"pulseaudio"` (or `"auto"`) driver to see volume changes that exceed 100%.

### Examples

Change the default scrolling step width to 3 percent:

```toml
[[block]]
block = "sound"
step_width = 3
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | `"auto"`, `"pulseaudio"`, `"alsa"` | No | `"auto"` (Pulseaudio with ALSA fallback)
`name` | PulseAudio / ALSA device name | No | Default Device (`@DEFAULT_SINK@` / `Master`)
`step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | No | `5`
`on_click` | Shell command to run when the sound block is clicked. | No | None

## Speed Test

Creates a block which uses [`speedtest-cli`](https://github.com/sivel/speedtest-cli) to measure your ping, download, and upload speeds.

### Examples

```toml
[[block]]
block = "speedtest"
bytes = true
interval = 1800
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`bytes` | Whether to use bytes or bits in the display (true for bytes, false for bits). | No | `false`
`interval` | Update interval, in seconds. | No | `1800`

## Temperature

Creates a block which displays the system temperature, based on lm_sensors' `sensors` output. The block is collapsed by default, and can be expanded by clicking, showing max and avg temperature. When collapsed, the color of the temperature block gives a quick indication as to the temperature (Critical when maxtemp > 80°, Warning when > 60°). **Depends on lm_sensors being installed and configured!**

Requires `lm_sensors` and appropriate kernel modules for your hardware.

### Examples

```toml
[[block]]
block = "temperature"
collapsed = false
interval = 10
format = "{min}° min, {max}° max, {average}° avg"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval, in seconds. | No | 5
collapsed | Collapsed by default? | No | true
`good` | Maximum temperature to set state to good. | No | `20`
`idle` | Maximum temperature to set state to idle. | No | `45`
`info` | Maximum temperature to set state to info. | No | `60`
`warning` | Maximum temperature to set state to warning. Beyond this temperature, state is set to critical | No | `80`

## Time

Creates a block which display the current time.

### Examples

```toml
[[block]]
block = "time"
format = "%a %d/%m %R"
timezone = "US/Pacific"
interval = 60
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | Format string. See the [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | No | `"%a %d/%m %R"`
`on_click` | Shell command to run when the time block is clicked. | No | None
`interval` | Update interval, in seconds. | No | 5
`timezone` | A timezone specifier (e.g. "Europe/Lisbon") | No | Local timezone

## Toggle

Creates a toggle block. You can add commands to be executed to disable the toggle (`command_off`), and to enable it (`command_on`).
You also need to specify a command to determine the (initial) state of the toggle (`command_state`). When the command outputs nothing, the toggle is disabled, otherwise enabled.
By specifying the `interval` property you can let the `command_state` be executed continuously.

### Examples

This is what I use to toggle my external monitor configuration:

```toml
[[block]]
block = "toggle"

text = "4k"
command_state = "xrandr | grep DP1\\ connected\\ 38 | grep -v eDP1"
command_on = "~/.screenlayout/4kmon_default.sh"
command_off = "~/.screenlayout/builtin.sh"
interval = 5
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`text` | Label to include next to the toggle icon. | No | ""
`command_on` | Shell Command to enable the toggle | Yes | None
`command_off` | Shell Command to disable the toggle | Yes | None
`command_state` | Shell Command to determine toggle state. Empty output => off. Any output => on.| Yes | None
`icon_on` | Icon override for the toggle button while on. | No | "toggle_on"
`icon_off` | Icon override for the toggle button while off. | No | "toggle_off"
`interval` | Update interval, in seconds. | No | None

## Weather

Creates a block which displays local weather and temperature information. In order to use this block, you will need access to a supported weather API service. At the time of writing, OpenWeatherMap is the only supported service.

Configuring the Weather block requires configuring a weather service, which may require API keys and other parameters.

### Examples

Show detailed weather in San Francisco through the OpenWeatherMap service:

```toml
[[block]]
block = "weather"
format = "{weather} ({location}) {temp}°, {wind} m/s {direction}"
service = { name = "openweathermap", api_key = "XXX", city_id = "5398563", units = "metric" }
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | The text format of the weather display. | No | `"{weather} {temp}°"`
`service` | The configuration of a weather service (see below). | Yes | None
`interval` | Update interval, in seconds. | No | `600`

### OpenWeatherMap Options

To use the service you will need a (free) API key.

Key | Values | Required | Default
----|--------|----------|--------
`name` | `openweathermap` | Yes | None
`api_key` | Your OpenWeatherMap API key. | Yes | None
`city_id` | OpenWeatherMap's ID for the city. | Yes | None
`units` | One of `metric` or `imperial`. | Yes | None

### Available Format Keys

Key | Value
----|-------
`{location}` | Location name (exact format depends on the service).
`{temp}` | Temperature.
`{weather}` | Textual description of the weather, e.g. "Raining".
`{wind}` | Wind speed.
`{direction}` | Wind direction, e.g. "NE".

## Uptime
Creates a block which displays system uptime. The block will always display the 2 biggest units, so minutes and seconds, or hours and minutes or days and hours or weeks and days.

### Examples

```toml
[[block]]
block = "uptime"
```

### Options

None

## Xrandr

Creates a block which shows screen information (name, brightness, resolution). With a click you can toggle through your active screens and with wheel up and down you can adjust the selected screens brightness.

### Examples

```toml
[[block]]
block = "xrandr"
icons = true
resolution = true
interval = 2
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`icons` | Show icons for brightness and resolution (needs awesome fonts support) | No | `true`
`resolution` | Shows the screens resolution | No | `false`
`step_width` | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50) | No | `5`
`interval` | Update interval, in seconds. | No | `5`
