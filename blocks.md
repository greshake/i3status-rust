# List of Available Blocks

- [Backlight](#backlight)
- [Battery](#battery)
- [Bluetooth](#bluetooth)
- [CPU Utilization](#cpu-utilization)
- [Custom](#custom)
- [Custom DBus](#custom-dbus)
- [Disk Space](#disk-space)
- [Focused Window](#focused-window)
- [IBus](#ibus)
- [Keyboard Layout](#keyboard-layout)
- [Load](#load)
- [Maildir](#maildir)
- [Memory](#memory)
- [Music](#music)
- [Net](#net)
- [NetworkManager](#networkmanager)
- [Notmuch](#notmuch)
- [Nvidia Gpu](#nvidia-gpu)
- [Pacman](#pacman)
- [Pomodoro](#pomodoro)
- [Sound](#sound)
- [Speed Test](#speed-test)
- [Taskwarrior](#taskwarrior)
- [Temperature](#temperature)
- [Time](#time)
- [Toggle](#toggle)
- [Uptime](#uptime)
- [Watson](#watson)
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
ACTION=="add", SUBSYSTEM=="backlight", GROUP="video", MODE="0664"
```

This will allow the video group to modify all backlight devices. You will also need to restart for this rule to take effect.

## Battery

Creates a block which displays the current battery state (Full, Charging or Discharging), percentage charged and estimate time until (dis)charged.

The battery block collapses when the battery is fully charged -- or, in the case of some Thinkpad batteries, when it reports "Not charging".

The battery block supports reading charging and status information from either `sysfs` or the [UPower](https://upower.freedesktop.org/) D-Bus interface. These "drivers" have largely identical features, but UPower does include support for `device = "DisplayDevice"`, which treats all physical power sources as a single logical battery. This is particularly useful if your system has multiple batteries.

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
driver = "upower"
format = "{percentage}% {time}"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The device in `/sys/class/power_supply/` to read from. When using UPower, this can also be `"DisplayDevice"`. | No | `"BAT0"`
`driver` | One of `"sysfs"` or `"upower"`. | No | `"sysfs"`
`interval` | Update interval, in seconds. Only relevant for `driver = "sysfs"`. | No | `10`
`format` | A format string. See below for available placeholders. | No | `"{percentage}%"`
`show` | Deprecated in favour of `format`. Show remaining `"time"`, `"percentage"` or `"both"` | No | `"percentage"`
`upower` | Deprecated in favour of `device`. When `true`, use the Upower D-Bus driver. | No | `false`

The `show` option is deprecated, and will be removed in future versions. In the meantime, it will override the `format` option when present.

### Format string

Placeholder | Description
------------|-------------
`{percentage}` | Battery level, in percent.
`{time}` | Time remaining until (dis)charge is complete.
`{power}` | Power consumption (in watts) by the battery or from the power supply when charging.

## Bluetooth

Creates a block which displays the connectivity of a given Bluetooth device, or the battery level if this is supported. Relies on the Bluez D-Bus API, and is therefore asynchronous.

When the device can be identified as an audio headset, a keyboard, joystick, or mouse, use the relevant icon. Otherwise, fall back on the generic Bluetooth symbol.

Right-clicking the block will attempt to connect (or disconnect) the device.

### Examples

A block for a Bluetooth device with the given MAC address:

```toml
[[block]]
block = "bluetooth"
mac = "A0:8A:F5:B8:01:FD"
label = " Rowkin"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`mac` | MAC address of the Bluetooth device. | Yes | None
`label` | Text label to display next to the icon. | No | None


## CPU Utilization

Creates a block which displays the overall CPU utilization, calculated from `/proc/stat`.

### Examples

Update CPU usage every second:

```toml
[[block]]
block = "cpu"
interval = 1
format = "{barchart} {utilization}% {frequency}GHz"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`info` | Minimum usage, where state is set to info. | No | `30`
`warning` | Minimum usage, where state is set to warning. | No | `60`
`critical` | Minimum usage, where state is set to critical. | No | `90`
`interval` | Update interval, in seconds. | No | `1`
`format` | A format string. Possible placeholders: `{barchart}` (barchart of each CPU's core utilization), `{utilization}` (average CPU utilization in percent) and `{frequency}` (CPU frequency). | No | `"{utilization}%"`
`frequency` | Deprecated in favour of `format`. Sets format to `{utilization}% {frequency}GHz` | No | `false`
`per_core` | Display CPU frequencies and utilization per core. | No | `false`


## Custom

Creates a block that display the output of custom shell commands.

For further customisation, use the `json` option and have the shell command output valid JSON in the schema below:  
`{"icon": "ICON", "state": "STATE", "text": "YOURTEXT"}`  
`icon` is optional, it may be an icon name from `icons.rs` (default "")  
`state` is optional, it may be Idle, Info, Good, Warning, Critical (default Idle)  

### Examples

```toml
[[block]]
block = "custom"
command = ''' cat /sys/class/thermal/thermal_zone0/temp | awk '{printf("%.1f\n",$1/1000)}' '''
```

```toml
[[block]]
block = "custom"
cycle = ["echo ON", "echo OFF"]
on_click = "<command>"
interval = 1
```

```toml
[[block]]
block = "custom"
command = "echo '{\"icon\":\"weather_thunder\",\"state\":\"Critical\", \"text\": \"Danger!\"}'"
json = true
```

### Options

Note that `command` and `cycle` are mutually exclusive.

Key | Values | Required | Default
----|--------|----------|--------
`command` | Shell command to execute & display. | No | None
`on_click` | Command to execute when the button is clicked. The command will be passed to whatever is specified in your `$SHELL` variable and - if not set - fallback to `sh`. | No | None
`cycle` | Commands to execute and change when the button is clicked. | No | None
`interval` | Update interval, in seconds. | No | `10`
`json` | Use JSON from command output to format the block. If the JSON is not valid, the block will error out. | No | `false`



## Custom DBus

Creates a block that can be updated asynchronously using DBus.

For example, updating the block using the command line tool `qdbus`: `qdbus i3.status.rs /CurrentSoundDevice i3.status.rs.SetStatus headphones`

### Examples

```toml
[[block]]
block = "custom_dbus"
name = "CurrentSoundDevice"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`name` | Name of the DBus object that i3status-rs will create. Must be unique. | Yes | None

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
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`path` | Path to collect information from | No | `"/"`
`alias` | Alias that is displayed for path | No | `"/"`
`info_type` | Currently supported options are `"available"`, `"free"`, `"total"` and `"used"` | No | `"available"`
`unit` | Unit that is used to display disk space. Options are `"MB"`, `"MiB"`, `"GB"`, `"GiB"`, `"TB"`, `"TiB"` and `"Percent"` | No | `"GB"`
`warning` | Available disk space warning level in GiB. | No | `20.0`
`alert` | Available disk space critical level in GiB. | No | `10.0`
`interval` | Update interval, in seconds. | No | `20`
`show_percentage` | Show percentage of used/available disk space depending on info_type. | No | `false`

## Focused Window

Creates a block which displays the title or the active marks of the currently focused window. Uses push updates from i3 IPC, so no need to worry about resource usage. The block only updates when the focused window changes title or the focus changes. Also works with sway, due to it having compatibility with i3's IPC.

### Examples

```toml
[[block]]
block = "focused_window"
max_width = 50
show_marks = "visible"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`max_width` | Truncates titles to this length. | No | `21`
`show_marks` | Display marks instead of the title, if there are some. Options are `"none"`, `"all"` or `"visible"`, the latter of which ignores marks that start with an underscore. | No | `"none"`

## IBus

Creates a block which displays the current global engine set in [IBus](https://wiki.archlinux.org/index.php/IBus). Updates are instant as D-Bus signalling is used.

### Examples

```toml
[[block]]
block = "ibus"
```

With optional mappings:

```toml
[[block]]
block = "ibus"
[block.mappings]
"mozc-jp" = "JP"
"xkb:us::eng" = "EN"
```

## Keyboard Layout

Creates a block to display the current keyboard layout.

Four drivers are available:
- `setxkbmap` which polls setxkbmap to get the current layout
- `localebus` which can read asynchronous updates from the systemd `org.freedesktop.locale1` D-Bus path
- `kbdd` which uses [kbdd](https://github.com/qnikst/kbdd) to monitor per-window layout changes via DBus
- `sway` which can read asynchronous updates from the sway IPC
 Which of these methods is appropriate will depend on your system setup.

### Examples

Check `setxkbmap` every 15 seconds:

```toml
[[block]]
block = "keyboard_layout"
driver = "setxkbmap"
interval = 15
```

Listen to D-Bus for changes:

```toml
[[block]]
block = "keyboard_layout"
driver = "localebus"
```

Listen to kbdd for changes:

```toml
[[block]]
block = "keyboard_layout"
driver = "kbddbus"
```

Listen to sway for changes:

```toml
[[block]]
block = "keyboard_layout"
driver = "sway"
sway_kb_identifier = "1133:49706:Gaming_Keyboard_G110"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | One of `"setxkbmap"`, `"localebus"`, `"kbddbus"` or `"sway"`, depending on your system. | No | `"setxkbmap"`
`interval` | Update interval, in seconds. Only used by the `"setxkbmap"` driver. | No | `60`
`format` | Format string, e.g. " {layout}" | No | `"{layout}"`
`sway_kb_identifier` | Identifier of the device you want to monitor, as found in the output of `swaymsg -t get_inputs` | No | ""

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
`info` | Minimum load, where state is set to info. | No | `0.3`
`warning` | Minimum load, where state is set to warning. | No | `0.6`
`critical` | Minimum load, where state is set to critical. | No | `0.9`
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
display_type = "new"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`inboxes` | List of maildir inboxes to look for mails in | Yes | None
`threshold_warning` | Number of unread mails where state is set to warning | No | `1`
`threshold_critical` | Number of unread mails where state is set to critical | No | `10`
`interval` | Update interval, in seconds. | No | `5`
`display_type` | Which part of the maildir to count. One of "new", "cur", or "all" | No | `"new"`
`icon` | Whether or not to prepend the output with the mail icon | No | `true`

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

Creates a block to display the current song title and artist in a fixed-width marquee. Also provides buttons for play/pause, previous and next.

Supports all music players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes:

- Spotify
- VLC
- mpd (via [mpDris2](https://github.com/eonpatapon/mpDris2))

and many others.

The block can be configured to drive a specific music player by name or automatically discover the currently active one.

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
`smart_trim` | When marquee rotation is disabled and the title + artist is longer than max-width, trim from both the artist and the title in proportion to their lengths, to try and show the most information possible. | No | `false`
`separator` | String to insert between artist and title | No | `" - "`
`buttons` | Array of control buttons to be displayed. Options are prev (previous title), play (play/pause) and next (next title) | No | `[]`
`on_collapsed_click` | Shell command to run when the music block is clicked while collapsed. | No | None

## Net

Creates a block which displays the upload and download throughput for a network interface. Units are by default in bytes per second (kB/s, MB/s, etc), 
but the 'use_bits' flag can be set to `true` to convert the units to bps (little b).

Note: `bitrate` for wired devices requires `ethtool` to be installed

### Examples

```toml
[[block]]
block = "net"
device = "wlp2s0"
ssid = true
signal_strength = true
ip = true
speed_up = false
graph_up = true
interval = 5
use_bits = false
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | Network interface to monitor (name from /sys/class/net) | Yes | `lo` (loopback interface)
`ssid` | Display network SSID (wireless only). | No | `false`
`signal_strength` | Display WiFi signal strength (wireless only). | No | `false`
`bitrate` | Display connection bitrate. | No | `false`
`ip` | Display connection IP address. | No | `false`
`ipv6` | Display connection IPv6 address. | No | `false`
`speed_up` | Display upload speed. | No | `true`
`speed_down` | Display download speed. | No | `true`
`graph_up` | Display a bar graph for upload speed. | No | `false`
`graph_down` | Display a bar graph for download speed. | No | `false`
`use_bits` | Display speeds in bits instead of bytes. | No | `false`
`interval` | Update interval, in seconds. | No | `1`
`hide_missing` | Whether to hide networks that are down/inactive completely. | No | `false`
`hide_inactive` | Whether to hide networks that are missing. | No | `false`


## NetworkManager

Creates a block which displays network connection information from NetworkManager.

### Examples

```toml
[[block]]
block = "networkmanager"
on_click = "alacritty -e nmtui"
```

### Options

Key | Values | Required | Default
----|--------|----------|---------
`primary_only` | Whether to show only the primary active connection or all active connections | No | `false`
`max_ssid_width` | Truncation length for SSID | No | `21`
`device_format` | Device string formatter. See below for available placeholders. | No | `"{icon}{ssid}"`
`connection_format` | Connection string formatter. See below for available placeholders. | No | `"{devices} {ips}"`
`on_click` | On-click handler | No | `""`

### AP format string

Placeholder | Description
------------|-------------
`{ssid}` | The SSID for this AP.
`{strength}` | The signal strength in percent for this AP.
`{frequency}` | The frequency of this AP in MHz.

### Device format string

Placeholder | Description
------------|-------------
`{icon}` | The icon matching the device type.
`{typename}` | The name of the device type.
`{ap}` | The connected AP if available, formatted with the AP format string.
`{ips}` | The list of IPs for this device.

### Connection format string

Placeholder | Description
------------|-------------
`{devices}` | The list of devices, each formatted with the device format string.


## Notmuch

Creates a block which queries a notmuch database and displays the count of messages.

The simplest configuration will return the total count of messages in the notmuch database stored at $HOME/.mail

NOTE: This block can only be used if you build with `cargo build --features=notmuch`

### Examples

```toml
[[block]]
block = "notmuch"
query = "tag:alert and not tag:trash"
threshold_warning = 1
threshold_critical = 10
name = "A"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`maildir` | Path to the directory containing the notmuch database | No | `$HOME/.mail`
`query` | Query to run on the database | No | `""`
`threshold_critical` | Mail count that triggers `critical` state | No | `99999`
`threshold_warning` | Mail count that triggers `warning` state | No | `99999`
`threshold_good` | Mail count that triggers `good` state | No | `99999`
`threshold_info` | Mail count that triggers `info` state | No | `99999`
`name` | Label to show before the mail count | No | `""`
`no_icon` | Disable the mail icon | No | `false`
`interval` | Update interval, in seconds. | No | `10`

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
`gpu_id` | GPU id in system | No | `0`
`label` | Display custom gpu label | No | `""`
`interval` | Update interval, in seconds. | No | `1`
`show_utilization` | Display gpu utilization. In percents. | No | `true`
`show_memory` | Display memory information. | No | `true`
`show_temperature` | Display gpu temperature. | No | `true`
`show_fan_speed` | Display fan speed. | No | `false`
`show_clocks` | Display gpu clocks. | No | `false`

## Pacman

Creates a block which displays the pending updates available on pacman or an AUR helper.

Requires fakeroot to be installed (only required for pacman).

### Examples

Update the list of pending updates every ten minutes (600 seconds):

Update interval should be set appropriately as to not exceed the AUR's daily rate limit.

pacman only config:

```toml
[[block]]
block = "pacman"
interval = 600
format = "{pacman} updates available"
format_singular = "{pacman} update available"
format_up_to_date = "system up to date"
critical_updates_regex = "(linux |linux-lts|linux-zen)"
```

pacman and AUR helper config:

```toml
[[block]]
block = "pacman"
interval = 600
format = "{pacman} + {aur} = {both} updates available"
format_singular = "{both} update available"
format_up_to_date = "system up to date"
critical_updates_regex = "(linux |linux-lts|linux-zen)"
# aur_command should output available updates to stdout (ie behave as echo -ne "update\n")
aur_command = "pikaur -Qua"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `600` (10min)
`format` | Format override | No | `"{pacman}"`
`format_singular` | Format override if exactly one update is available | No | `"{pacman}"`
`format_up_to_date` | Format override if no updates are available | No | `"{pacman}"`
`critical_updates_regex` | Display block as critical if updates matching regex are available | No | `None`
`aur_command` | AUR command to check available updates, which outputs in the same format as pacman. e.g. `pikaur -Qua` | if `{both}` or `{aur}` are used | `None`

### Available Format Keys

Key | Value
----|-------
`{count}` | Number of pacman updates available (**deprecated**: use `{pacman}` instead)
`{pacman}`| Number of updates available according to `pacman`
`{aur}` | Number of updates available according to `<aur_command>`
`{both}` | Cumulative number of updates available according to `pacman` and `<aur_commad>` 


## Pomodoro

Creates a block which runs a [pomodoro timer](https://en.wikipedia.org/wiki/Pomodoro_Technique).

### Examples

```toml
[[block]]
block = "pomodoro"
length = 25
break_length = 5
message = "Take a break!"
break_message = "Back to work!"
use_nag = false
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`length` | Timer duration in minutes. | No | `25`
`break_length` | Break duration in minutes. | No | `5`
`use_nag` | i3-nagbar enabled | No | `false`
`message` | i3-nagbar message when timer expires. | No | `Pomodoro over! Take a break!`
`break_message` | i3-nagbar message when break is over. | No | `Break over! Time to work!`


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
`format` | Any string to use next to the icon | No | `{volume}%`
`name` | PulseAudio device name, or the ALSA control name as found in the output of `amixer -D yourdevice scontrols` | No | PulseAudio: `@DEFAULT_SINK@` / ALSA: `Master`
`device` | ALSA device name, usually in the form "hw:X" or "hw:X,Y" where `X` is the card number and `Y` is the device number as found in the output of `aplay -l` | No | `default`
`natural_mapping` | Use the mapped volume for evaluating the percentage representation like `alsamixer`/`amixer -M`, to be more natural for human ear | No | `false`
`step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | No | `5`
`on_click` | Shell command to run when the sound block is clicked. | No | None
`show_volume_when_muted` | Show the volume even if it is currently muted. | No | `false`

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

## Taskwarrior

Creates a block which displays number of pending and started tasks of the current users taskwarrior list.

Clicking the left mouse button on the icon updates the number of pending tasks immediately.

Clicking the right mouse button on the icon toggles the view of the block between filtered (default) and non-filtered
tasks. If there are no filters configured, the number of tasks stays the same and both modes are behaving
equally.  

### Examples

```toml
[[block]]
block = "taskwarrior"
interval = 60
format = "{count} open tasks"
format_singular = "{count} open task"
format_everything_done = "nothing to do!"
warning_threshold = 10
critical_threshold = 20
filter_tags = ["work", "important"]
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `600` (10min)
`warning_threshold` | The threshold of pending (or started) tasks when the block turns into a warning state. | No | `10`
`critical_threshold` | The threshold of pending (or started) tasks when the block turns into a critical state. | No | `20`
`filter_tags` | A list of tags a task has to have before its counted as a pending task. | No | ```<empty>```
`format` | Format override | No | `"{count}"`
`format_singular` | Format override if exactly one task is pending | No | `"{count}"`
`format_everything_done` | Format override if all tasks are completed | No | `"{count}"`

### Available Format Keys

Key | Value
----|-------
`{count}` | The number of pending tasks.

## Temperature

Creates a block which displays the system temperature, based on lm_sensors' `sensors -u` output. The block has two modes: "collapsed", which uses only colour as an indicator, and "expanded", which shows the content of a `format` string.

Requires `lm_sensors` and appropriate kernel modules for your hardware.

The average, minimum, and maximum temperatures are computed using all sensors displayed by `sensors -u`, or the subset matching the chip name, if `chip` is specified.

Note that the colour of the block is always determined by the maximum temperature across all sensors, not the average. You may need to keep this in mind if you have a misbehaving sensor.

### Examples

```toml
[[block]]
block = "temperature"
collapsed = false
interval = 10
format = "{min}° min, {max}° max, {average}° avg"
chip = "*-isa-*"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `5`
`collapsed` | Whether the block will be collapsed by default. | No | `true`
`good` | Maximum temperature to set state to good. | No | `20`
`idle` | Maximum temperature to set state to idle. | No | `45`
`info` | Maximum temperature to set state to info. | No | `60`
`warning` | Maximum temperature to set state to warning. Beyond this temperature, state is set to critical. | No | `80`
`chip` | Narrows the results to a given chip name. `*` may be used as a wildcard. | No | None

### Available Format Keys

Key | Value
----|-------
`{min}` | Minimum temperature among all sensors.
`{average}` | Average temperature among all sensors.
`{max}` | Maximum temperature among all sensors.

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
`interval` | Update interval, in seconds. | No | `5`
`timezone` | A timezone specifier (e.g. "Europe/Lisbon") | No | Local timezone

## Toggle

Creates a toggle block. You can add commands to be executed to disable the toggle (`command_off`), and to enable it (`command_on`). If these command exit with a non-zero status, the block will not be toggled and the block state will be changed to give a visual warning of the failure.
You also need to specify a command to determine the initial state of the toggle (`command_state`). When the command outputs nothing, the toggle is disabled, otherwise enabled.
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
`text` | Label to include next to the toggle icon. | No | `""`
`command_on` | Shell Command to enable the toggle | Yes | None
`command_off` | Shell Command to disable the toggle | Yes | None
`command_state` | Shell Command to determine toggle state. Empty output => off. Any output => on.| Yes | None
`icon_on` | Icon override for the toggle button while on. | No | `"toggle_on"`
`icon_off` | Icon override for the toggle button while off. | No | `"toggle_off"`
`interval` | Update interval, in seconds. | No | None


## Uptime
Creates a block which displays system uptime. The block will always display the 2 biggest units, so minutes and seconds, or hours and minutes or days and hours or weeks and days.

### Examples

```toml
[[block]]
block = "uptime"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `60`


## Watson

[Watson](http://tailordev.github.io/Watson/) is a simple CLI time tracking application. This block will show the name of your current active project, tags and optionally recorded time. Clicking the widget will toggle the `show_time` variable dynamically.

### Examples

```toml
[[block]]
block = "watson"
show_time = true
state_path = "/home/user/.config/watson/state"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`show_time` | Whether to show recorded time | No | `false`
`state_path` | Path to the Watson state file | No | `$XDG_CONFIG_HOME/watson/state`

## Weather

Creates a block which displays local weather and temperature information. In order to use this block, you will need access to a supported weather API service. At the time of writing, OpenWeatherMap is the only supported service.

Configuring the Weather block requires configuring a weather service, which may require API keys and other parameters.

If using the `autolocate` feature, set the block update interval such that you do not exceed ipapi.co's free daily limit of 1000 hits.

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
`autolocate` | Gets your location using the ipapi.co IP location service (no API key required). If the API call fails then the block will fallback to `city_id` or `place`. | No | false

### OpenWeatherMap Options

To use the service you will need a (free) API key.

Key | Values | Required | Default
----|--------|----------|--------
`name` | `openweathermap` | Yes | None
`api_key` | Your OpenWeatherMap API key. | Yes | None
`city_id` | OpenWeatherMap's ID for the city. | Yes* | None
`place` | OpenWeatherMap 'By city name' search query. See [here](https://openweathermap.org/current) | Yes* | None
`units` | One of `metric` or `imperial`. | Yes | None

Either one of `city_id` or `place` is required. If both are supplied, `city_id` takes precedence.

The options `api_key`, `city_id`, `place` can be omitted from configuration,
in which case they must be provided in the environment variables
`OPENWEATHERMAP_API_KEY`, `OPENWEATHERMAP_CITY_ID`, `OPENWEATHERMAP_PLACE`.

### Available Format Keys

Key | Value
----|-------
`{location}` | Location name (exact format depends on the service).
`{temp}` | Temperature.
`{apparent}` | Australian Apparent Temperature.
`{humidity}` | Humidity.
`{weather}` | Textual description of the weather, e.g. "Raining".
`{wind}` | Wind speed.
`{direction}` | Wind direction, e.g. "NE".


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

## Docker

Creates a block which shows the local docker daemon status (containers running, paused, stopped, total and image count).

### Examples

```toml
[[block]]
block = "docker"
interval = 2
format = "{running}/{total}"
```

### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `5`
`format` | A format string. See below for available placeholders. | No | `"{running}"`

### Available Format Keys

Key | Value
----|-------
`{total}` | Total containers on the host.
`{running}` | Containers running on the host.
`{stopped}` | Containers stopped on the host.
`{paused}` | Containers paused on the host.
`{images}` | Total images on the host.
