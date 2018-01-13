# List of Available Blocks

- [Backlight](#backlight)
- [Battery](#battery)
- [CPU Utilization](#cpu-utilization)
- [Custom](#custom)
- [Disk Space](#disk-space)
- [Focused Window](#focused-window)
- [Load](#load)
- [Memory](#memory)
- [Music](#music)
- [Net](#net)
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

## Battery

Creates a block which displays the current battery state (Full, Charging or Discharging), percentage charged and estimate time until (dis)charged.

### Examples

Update the battery state every ten seconds:

```toml
[[block]]
block = "battery"
interval = 10
show = "percentage"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The device in `/sys/class/power_supply/` to read from. | No | `"BAT0"`
`interval` | Update interval, in seconds. | No | `10`
`show` | Show remaining 'time', 'percentage' or 'both' | No | `both`

## CPU Utilization

Creates a block which displays the overall CPU utilization, calculated from `/proc/stat`.

### Examples

Update CPU usage every second:

```toml
[[block]]
block = "cpu"
interval = 1
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`info` | Minimum usage, where state is set to info. | No | `30`
`warning` | Minimum usage, where state is set to warning. | No | `60`
`critical` | Minimum usage, where state is set to critical. | No | `90`
`interval` | Update interval, in seconds. | No | `1`

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
`on_click` | Command to execute when the button is clicked. | No | None
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
`info_type` | Currently supported options are available and free | No | `"available"`
`unit` | Unit that is used to display disk space. Options are MB, MiB, GB and GiB | No | `"GB"`
`interval` | Update interval, in seconds. | No | `20`

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

Key | Value
----|-------
`{MTg}` | Memory total (GiB).
`{MTm}` | Memory total (MiB).
`{MAg}` | Available memory, including cached memory and buffers (GiB).
`{MAm}` | Available memory, including cached memory and buffers (MiB).
`{MAp}` | Available memory, including cached memory and buffers (%).
`{MFg}` | Memory free (GiB).
`{MFm}` | Memory free (MiB).
`{MFp}` | Memory free (%).
`{Mug}` | Memory used, excluding cached memory and buffers; similar to htop's green bar (GiB).
`{Mum}` | Memory used, excluding cached memory and buffers; similar to htop's green bar (MiB).
`{Mup}` | Memory used, excluding cached memory and buffers; similar to htop's green bar (%).
`{MUg}` | Total memory used (GiB).
`{MUm}` | Total memory used (MiB).
`{MUp}` | Total memory used (%).
`{Cg}`  | Cached memory, similar to htop's yellow bar (GiB).
`{Cm}`  | Cached memory, similar to htop's yellow bar (MiB).
`{Cp}`  | Cached memory, similar to htop's yellow bar (%).
`{Bg}`  | Buffers, similar to htop's blue bar (GiB).
`{Bm}`  | Buffers, similar to htop's blue bar (MiB).
`{Bp}`  | Buffers, similar to htop's blue bar (%).
`{STg}` | Swap total (GiB).
`{STm}` | Swap total (MiB).
`{SFg}` | Swap free (GiB).
`{SFm}` | Swap free (MiB).
`{SFp}` | Swap free (%).
`{SUg}` | Swap used (GiB).
`{SUm}` | Swap used (MiB).
`{SUp}` | Swap used (%).

## Music

Creates a block which can display the current song title and artist, in a fixed width marquee fashion. It uses dbus signaling to fetch new tracks, so no periodic updates are needed. It supports all Players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes spotify, vlc and many more. Also provides buttons for play/pause, previous and next title.

### Examples

Show the currently playing song on Spotify, with play & next buttons:

```toml
[[block]]
block = "music"
player = "spotify"
buttons = ["play", "next"]
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`player` | Name of the music player. Must be the same name the player is registered with the MediaPlayer2 Interface.  | Yes | None
`max_width` | Max width of the block in characters, not including the buttons | No | `21`
`marquee` | Bool to specify if a marquee style rotation should be used if the title + artist is longer than max-width | No | `true`
`marquee_interval` | Marquee interval in seconds. This is the delay between each rotation. | No | `10`
`marquee_speed` | Marquee speed in seconds. This is the scrolling time used per character. | No | `0.5`
`buttons` | Array of control buttons to be displayed. Options are prev (previous title), play (play/pause) and next (next title) | No | `[]`

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

Creates a block which displays the volume level (according to ALSA). Right click to toggle mute, scroll to adjust volume.

The display is updated when ALSA detects changes, so there is no need to set an update interval.

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

Creates a block which displays the system temperature, based on lm_sensors' `sensors` output. The block is collapsed by default, and can be expanded by clicking, showing max and avg temperature. When collapsed, the color of the temperature block gives a quick indication as to the temperature (Critical when maxtemp > 80°, Warning when > 60°). Currently, you can only adjust these thresholds in source code. **Depends on lm_sensors being installed and configured!**

### Examples

```toml
[[block]]
block = "temperature"
collapsed = false
interval = 10
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval, in seconds. | No | 5
collapsed | Collapsed by default? | No | true

## Time

Creates a block which display the current time.

### Examples

```toml
[[block]]
block = "time"
format = "%a %d/%m %R"
interval = 60
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | Format string. See the [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | No | `"%a %d/%m %R"`
`on_click` | Shell command to run when the sound block is clicked. | No | None
`interval` | Update interval, in seconds. | No | 5

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
`command_on` | Shell Command to enable the toggle | Yes | None
`command_off` | Shell Command to disable the toggle | Yes | None
`command_state` | Shell Command to determine toggle state. Empty output => off. Any output => on.| Yes | None
`interval` | Update interval, in seconds. | No | None

## Weather

Creates a block which displays local weather and temperature information. In order to use this block, you will need access to a supported weather API service. At the time of writing, OpenWeatherMap is the only supported service.

Configuring the Weather block requires configuring a weather service, which may require API keys and other parameters.

### Examples

Show detailed weather in San Francisco through the OpenWeatherMap service:

```toml
[[block]]
block = "weather"
format = "{weather} ({location}) {temp}°, {wind} km/s"
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
