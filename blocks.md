## Time
Creates a block which display the current time.

**Example**
```toml
[[block]]
block = "time"

interval = 60
format = "%a %d/%m %R"
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
format | Format string.<br/> See [chrono docs](https://docs.rs/chrono/0.3.0/chrono/format/strftime/index.html#specifiers) for all options. | No | %a %d/%m %R
interval | Update interval in seconds | No | 5


## Memory

Creates a block displaying memory and swap usage.

By default, the format of this module is "<Icon>: {MFm}MB/{MTm}MB({Mp}%)" (Swap values
accordingly). That behaviour can be changed within your config.

This module keeps track of both Swap and Memory. By default, a click switches between them.


**Example**
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

**Options**

Key | Values | Required | Default
----|--------|----------|--------
format_mem | Format string for Memory view. All format values are described below. | No | {MFm}MB/{MTm}MB({Mp}%)
format_swap | Format string for Swap view. | No | {SFm}MB/{STm}MB({Sp}%)
display_type | Default view displayed on startup. Options are <br/> memory, swap | No | memory
icons | Whether the format string should be prepended with Icons. Options are <br/> true, false | No | true
clickable | Whether the view should switch between memory and swap on click. Options are <br/> true, false | No | true
interval | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | 5
warning_mem | Percentage of memory usage, where state is set to warning | No | 80.0
warning_swap | Percentage of swap usage, where state is set to warning | No | 80.0
critical_mem | Percentage of memory usage, where state is set to critical | No | 95.0
critical_swap | Percentage of swap usage, where state is set to critical | No | 95.0

### Format string specification

Key | Value
----|-------
{MTg} | Memory total (GiB)
{MTm} | Memory total (MiB)
{MAg} | Available memory, including cached memory and buffers (GiB)
{MAm} | Available memory, including cached memory and buffers (MiB)
{MAp} | Available memory, including cached memory and buffers (%)
{MFg} | Memory free (GiB)
{MFm} | Memory free (MiB)
{MFp} | Memory free (%)
{Mug} | Memory used, excluding cached memory and buffers; similar to htop's green bar (GiB)
{Mum} | Memory used, excluding cached memory and buffers; similar to htop's green bar (MiB)
{Mup} | Memory used, excluding cached memory and buffers; similar to htop's green bar (%)
{MUg} | Total memory used (GiB)
{MUm} | Total memory used (MiB)
{MUp} | Total memory used (%)
{Cg}  | Cached memory, similar to htop's yellow bar (GiB)
{Cm}  | Cached memory, similar to htop's yellow bar (MiB)
{Cp}  | Cached memory, similar to htop's yellow bar (%)
{Bg}  | Buffers, similar to htop's blue bar (GiB)
{Bm}  | Buffers, similar to htop's blue bar (MiB)
{Bp}  | Buffers, similar to htop's blue bar (%)
{STg} | Swap total (GiB)
{STm} | Swap total (MiB)
{SFg} | Swap free (GiB)
{SFm} | Swap free (MiB)
{SFp} | Swap free (%)
{SUg} | Swap used (GiB)
{SUm} | Swap used (MiB)
{SUp} | Swap used (%)


## Music
Creates a block which can display the current song title and artist, in a fixed width marquee fashion. It uses dbus signaling to fetch new tracks, so no periodic updates are needed. It supports all Players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes spotify, vlc and many more. Also provides buttons for play/pause, previous and next title.

**Example**
```toml
[[block]]
block = "music"

player = "spotify"
buttons = ["play", "next"]
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
player | Name of the music player.Must be the same name the player<br/> is registered with the MediaPlayer2 Interface.  | Yes | -
max_width | Max width of the block in characters, not including the buttons | No | 21
marquee | Bool to specify if a marquee style rotation should be used every<br/>10s if the title + artist is longer than max_width | No | true
buttons | Array of control buttons to be displayed. Options are<br/>prev (previous title), play (play/pause) and next (next title) | No | []

## Load
Creates a block which displays the system load average.

**Example**
```toml
[[block]]
block = "load"

format = "{1m} {5m}"
interval = 1
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
format | Format string.<br/> You can use the placeholders 1m 5m and 15m, e.g. "1min avg: {1m}" | No | {1m}
interval | Update interval in seconds | No | 3

## Cpu utilization
Creates a block which displays the overall CPU utilization, calculated from /proc/stat.

**Example**
```toml
[[block]]
block = "cpu"

interval = 1
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 1
info | Minimum usage, where state is set to info | No | 30
warning | Minimum usage, where state is set to warning | No | 60
critical | Minimum usage, where state is set to critical | No | 90

## Battery
Creates a block which displays the current battery state (Full, Charging or Discharging) and percentage charged.

**Example**
```toml
[[block]]
block = "battery"

interval = 10
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 10
device | Which device in /sys/class/power_supply/ to read from. | No | BAT0

## Custom
Creates a block that display the output of custom commands

**Example**
```toml
[[block]]
block = "custom"

interval = 100
command = "uname"
```

```toml
[[block]]
block = "custom"

interval = 1
cycle = ["echo ON", "echo OFF"]
on_click = "<command>"
```

**Options**

Note that `command` and `cycle` are mutually exclusive.

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 10
command | Shell Command to execute & display | No | None
on_click | Command to execute when the button is clicked | No | None
cycle | Commands to execute and change when the button is clicked | No | None

## Toggle
Creates a toggle block. You can add commands to be executed to disable the toggle (`command_off`), and to enable it (`command_on`).
You also need to specify a command to determine the (initial) state of the toggle (`command_state`). When the command outputs nothing, the toggle is disabled, otherwise enabled.
By specifying the `interval` property you can let the `command_state` be executed continuously.

**Example**
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

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | Never
command_on | Shell Command to enable the toggle | Yes | None
command_off | Shell Command to disable the toggle | Yes | None
command_state | Shell Command to determine toggle state. <br/>Empty output => off. Any output => on.| Yes | None


## Pacman
Creates a block which displays the pending updates available on pacman.

**Example**
```toml
[[block]]
block = "pacman"

interval = 10
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 600 (10min)


## Disk Space
Creates a block which displays disk space information.

**Example**
```toml
[[block]]
block = "disk_space"


path = "/"
alias = "/"
info_type = "available"
unit = "GB"
interval = 20
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
path | Path to collect information from | No | /
alias | Alias that is displayed for path | No | /
info_type | Currently supported options are available and free | No | available
unit | Unit that is used to display disk space. Options are MB, MiB, GB and GiB | No | GB
interval | Update interval in seconds | No | 20


## Sound
Creates a block which displays the current Master volume (currently based on amixer output). Right click to toggle mute, scroll to adjust volume.

**Example**
```toml
[[block]]
block = "sound"

interval = 10
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 2
step\_width | The steps volume is in/decreased for the selected audio device (When greater than 50 it gets limited to 50) | No | 5


## Temperature
Creates a block which displays the system temperature, based on lm_sensors' `sensors` output. The block is collapsed by default, and can be expanded by clicking, showing max and avg temperature. When collapsed, the color of the temperature block gives a quick indication as to the temperature (Critical when maxtemp > 80°, Warning when > 60°). Currently, you can only adjust these thresholds in source code. **Depends on lm_sensors being installed and configured!**

**Example**
```toml
[[block]]
block = "temperature"

interval = 10
collapsed = false
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 5
collapsed | Collapsed by default? | No | true

## Focused Window
Creates a block which displays the title of the currently focused window. Uses push updates from i3 IPC, so no need to worry about resource usage. The block only updates when the focused window changes title or the focus changes.

**Example**
```toml
[[block]]
block = "focused_window"

max_width = 21
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
max_width | Truncates titles if longer than max_width | No | 21

## Xrandr
Creates a block which shows screen information (name, brightness, resolution). With a click you can toggle through your active screens and with wheel up and down you can adjust the selected screens brighntess.

Example
```toml
[[block]]
block = "xrandr"

interval = 2
icons = true
resolution = true
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
interval | Update interval in seconds | No | 5
icons | Show icons for brightness and resolution (needs awesome fonts support) | No | true
resolution | Shows the screens resolution | No | false
step\_width | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50) | No | 5

## Net
Creates a block which displays the upload and download throughput for a network interface. Units are in bytes per second (kB/s, MB/s, etc).

**Example**
```toml
[[block]]
block = "net"
device = "wlp2s0"
interval = 5
ssid = true
ip = true
speed_up = false
graph_up = true
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
device | Network interface to moniter (name from /sys/class/net) | Yes | lo (loopback interface)
interval | Update interval in seconds | No | 1
ssid | Display network SSID (wireless only) | No | false
bitrate | Display connection bitrate | No | false
ip | Display connection IP address | No | false
speed_up | Display upload speed | no | true
speed_down | Display download speed | no | true
graph_up | Display a bar graph for upload speed | no | false
graph_down | Display a bar graph for download speed | no | false

## Speed Test
Creates a block which uses [`speedtest-cli`](https://github.com/sivel/speedtest-cli) to measure your ping, download, and upload speeds.

**Example**
```toml
[[block]]
block = "speedtest"

bytes = true
interval = 1800
```

**Options**

Key | Values | Required | Default
----|--------|----------|--------
bytes | whether to use bytes or bits in the display.<br>(true for bytes, false for bits) | No | false
interval | Update interval in seconds | No | 1800
