# List of Available Blocks

- [Apt](#apt)
- [Backlight](#backlight)
- [Battery](#battery)
- [Bluetooth](#bluetooth)
- [CPU Utilization](#cpu-utilization)
- [Custom](#custom)
- [Custom DBus](#custom-dbus)
- [Disk Space](#disk-space)
- [Docker](#docker)
- [Focused Window](#focused-window)
- [GitHub](#github)
- [Hueshift](#hueshift)
- [IBus](#ibus)
- [KDEConnect](#kdeconnect)
- [Keyboard Layout](#keyboard-layout)
- [Load](#load)
- [Maildir](#maildir)
- [Memory](#memory)
- [Music](#music)
- [Net](#net)
- [NetworkManager](#networkmanager)
- [Notify](#notify)
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

## Apt 

Creates a block which displays the pending updates available for your Debian/Ubuntu based system.

Behind the scenes this uses `apt`, and in order to run it without root priveleges i3status-rust will create its own package database in `/tmp/i3rs-apt/` which may take up several MB or more. If you have a custom apt config then this block may not work as expected - in that case please open an issue.

#### Examples

Update the list of pending updates every thirty minutes (1800 seconds):

```toml
[[block]]
block = "apt"
interval = 1800
format = "{count} updates available"
format_singular = "{count} update available"
format_up_to_date = "system up to date"
critical_updates_regex = "(linux |linux-lts|linux-zen)"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval in seconds. | No | `600`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{count}"`
`format_singular` | Same as `format`, but for when exactly one update is available. | No | `"{count}"`
`format_up_to_date` | Same as `format`, but for when no updates are available. | No | `"{count}"`
`warning_updates_regex` | Display block as warning if updates matching regex are available. | No | `None`
`critical_updates_regex` | Display block as critical if updates matching regex are available. | No | `None`

#### Available Format Keys

Key | Value
----|-------
`{count}` | Number of updates available

###### [↥ back to top](#list-of-available-blocks)

## Backlight

Creates a block to display screen brightness. This is a simplified version of the [Xrandr](#xrandr) block that reads brightness information directly from the filesystem, so it works under Wayland. The block uses `inotify` to listen for changes in the device's brightness directly, so there is no need to set an update interval.

When there is no `device` specified, this block will display information from the first device found in the `/sys/class/backlight` directory. If you only have one display, this approach should find it correctly.

It is possible to set the brightness using this block as well -- [see below](#setting-brightness-with-the-mouse-wheel) for details.

#### Examples

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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The `/sys/class/backlight` device to read brightness information from. | No | Default device
`step_width` | The brightness increment to use when scrolling, in percent. | No | `5`
`root_scaling` | Scaling exponent reciprocal (ie. root). | No | `1.0`

Some devices expose raw values that are best handled with nonlinear scaling. The human perception of lightness is close to the cube root of relative luminance, so settings for `root_scaling` between 2.4 and 3.0 are worth trying. For devices with few discrete steps this should be 1.0 (linear). More information: <https://en.wikipedia.org/wiki/Lightness>

#### Setting Brightness with the Mouse Wheel

The block allows for setting brightness with the mouse wheel. However, depending on how you installed i3status-rust, it may not have the appropriate permissions to modify these files, and will fail silently. To remedy this you can write a `udev` rule for your system (if you are comfortable doing so).

First, check that your user is a member of the "video" group using the `groups` command. Then add a rule in the `/etc/udev/rules.d/` directory containing the following, for example in `backlight.rules`:

```
ACTION=="add", SUBSYSTEM=="backlight", GROUP="video", MODE="0664"
```

This will allow the video group to modify all backlight devices. You will also need to restart for this rule to take effect.

###### [↥ back to top](#list-of-available-blocks)

## Battery

Creates a block which displays the current battery state (Full, Charging or Discharging), percentage charged and estimate time until (dis)charged.

The battery block collapses when the battery is fully charged -- or, in the case of some Thinkpad batteries, when it reports "Not charging".

The battery block supports reading charging and status information from either `sysfs` or the [UPower](https://upower.freedesktop.org/) D-Bus interface. These "drivers" have largely identical features, but UPower does include support for `device = "DisplayDevice"`, which treats all physical power sources as a single logical battery. This is particularly useful if your system has multiple batteries.

#### Examples

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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The device in `/sys/class/power_supply/` to read from. When using UPower, this can also be `"DisplayDevice"`. | No | `"BAT0"`
`driver` | One of `"sysfs"` or `"upower"`. | No | `"sysfs"`
`interval` | Update interval, in seconds. Only relevant for `driver = "sysfs"`. | No | `10`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{percentage}%"`
`full_format` | Same as `format` but for when the battery is full. | No | `"{percentage}%"`
`missing_format` | Same as `format` but for when the specified battery is missing. | No | `"{percentage}%"`
`allow_missing` | Don't display errors when the battery cannot be found. Only works with the `sysfs` driver. | No | `false`
`hide_missing` | Completely hide this block if the battery cannot be found. Only works in combination with `allow_missing`. | No | `false`
`info` | Minimum battery level, where state is set to info. | No | `60`
`good` | Minimum battery level, where state is set to good. | No | `60`
`warning` | Minimum battery level, where state is set to warning. | No | `30`
`critical` | Minimum battery level, where state is set to critical. | No | `15`

#### Deprecated Options

Key | Values | Required | Default
----|--------|----------|--------
`show` | Deprecated in favour of `format`. Show remaining `"time"`, `"percentage"` or `"both"`. | No | `"percentage"`
`upower` | Deprecated in favour of `device`. When `true`, use the Upower D-Bus driver. | No | `false`

The `show` option is deprecated, and will be removed in future versions. In the meantime, it will override the `format` option when present.

#### Available Format Keys

Placeholder | Description
------------|-------------
`{percentage}` | Battery level, in percent
`{bar}` | The current battery level in a bar chart
`{time}` | Time remaining until (dis)charge is complete
`{power}` | Power consumption (in watts) by the battery or from the power supply when charging

###### [↥ back to top](#list-of-available-blocks)

## Bluetooth

Creates a block which displays the connectivity of a given Bluetooth device, or the battery level if this is supported. Relies on the Bluez D-Bus API, and is therefore asynchronous.

When the device can be identified as an audio headset, a keyboard, joystick, or mouse, use the relevant icon. Otherwise, fall back on the generic Bluetooth symbol.

Right-clicking the block will attempt to connect (or disconnect) the device.

#### Examples

A block for a Bluetooth device with the given MAC address:

```toml
[[block]]
block = "bluetooth"
mac = "A0:8A:F5:B8:01:FD"
label = " Rowkin"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`mac` | MAC address of the Bluetooth device. | Yes | None
`label` | Text label to display next to the icon. | No | None
`hide_disconnected` | Hides the block when the device is disconnected. | No | `false`
`format_unavailable` | A string to customise the output of this block when the bluetooth controller is unavailable. See below for placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{label} x"`

#### Available Format Keys

Key | Value
----|-------
`{label}` | Device label as set in the block config

###### [↥ back to top](#list-of-available-blocks)

## CPU Utilization

Creates a block which displays the overall CPU utilization, calculated from `/proc/stat`.

#### Examples

Update CPU usage every second:

```toml
[[block]]
block = "cpu"
interval = 1
format = "{barchart} {utilization} {frequency}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`info` | Minimum usage, where state is set to info. | No | `30`
`warning` | Minimum usage, where state is set to warning. | No | `60`
`critical` | Minimum usage, where state is set to critical. | No | `90`
`interval` | Update interval, in seconds. | No | `1`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{utilization}"`
`per_core` | Display CPU frequencies and utilization per core. | No | `false`
`on_click` | Command to execute when the button is clicked. The command will be passed to whatever is specified in your `$SHELL` variable and - if not set - fallback to `sh`. | No | None

#### Deprecated Options

Key | Values | Required | Default
----|--------|----------|--------
`frequency` | Deprecated in favour of `format`. Sets format to `{utilization} {frequency}`. | No | `false`

#### Available Format Keys

Placeholder | Description
------------|-------------
`{barchart}` | Bar chart of each CPU's core utilization
`{utilization}` | Average CPU utilization in percent
`{frequency}` | CPU frequency in GHz

###### [↥ back to top](#list-of-available-blocks)

## Custom

Creates a block that display the output of custom shell commands.

For further customisation, use the `json` option and have the shell command output valid JSON in the schema below:  
`{"icon": "ICON", "state": "STATE", "text": "YOURTEXT"}`  
`icon` is optional, it may be an icon name from `icons.rs` (default "")  
`state` is optional, it may be Idle, Info, Good, Warning, Critical (default Idle)  

#### Examples

Display temperature, update every 10 seconds:

```toml
[[block]]
block = "custom"
command = ''' cat /sys/class/thermal/thermal_zone0/temp | awk '{printf("%.1f\n",$1/1000)}' '''
```

Cycle between "ON" and "OFF", update every 1 second, run `<command>` when block is clicked:

```toml
[[block]]
block = "custom"
cycle = ["echo ON", "echo OFF"]
on_click = "<command>"
interval = 1
```

Use JSON output:

```toml
[[block]]
block = "custom"
command = "echo '{\"icon\":\"weather_thunder\",\"state\":\"Critical\", \"text\": \"Danger!\"}'"
json = true
```

Display kernel, update the block only once:

```toml
[[block]]
block = "custom"
command = "uname -r"
interval = "once"
```

Display the screen brightness on an intel machine and update this only when `pkill -SIGRTMIN+4 i3status-rs` is called:

```toml
[[block]]
block = "custom"
command = ''' cat /sys/class/backlight/intel_backlight/brightness | awk '{print $1}' '''
signal = 4
interval = "once"
```

#### Options

Note that `command` and `cycle` are mutually exclusive.

Key | Values | Required | Default
----|--------|----------|--------
`command` | Shell command to execute & display. Shell command output may need to be escaped, refer to [Escaping Text](#escaping-text). | No | None
`on_click` | Command to execute when the button is clicked. | No | None
`cycle` | Commands to execute and change when the button is clicked. | No | None
`interval` | Update interval, in seconds (or `"once"` to update only once). | No | `10`
`json` | Use JSON from command output to format the block. If the JSON is not valid, the block will error out. | No | `false`
`signal` | Signal value that causes an update for this block with 0 corresponding to `-SIGRTMIN+0` and the largest value being `-SIGRTMAX`. | No | None
`hide_when_empty` | Hides the block when the command output (or json text field) is empty. | No | false
`shell` | Specify the shell to use when running commands. | No | `$SHELL` if set, otherwise fallback to `sh`

###### [↥ back to top](#list-of-available-blocks)

## Custom DBus

Creates a block that can be updated asynchronously using DBus.

For example, updating the block using command line tools:  
`busctl --user call i3.status.rs /CurrentSoundDevice i3.status.rs SetStatus sss Headphones music Good`  
`busctl --user call i3.status.rs /test i3.status.rs SetStatus s just-some-text`  
or
`qdbus i3.status.rs /CurrentSoundDevice i3.status.rs.SetStatus Headphones music Good`.  

The first argument is the text content of the block, the second (optional) argument is the icon to use (as found in `icons.rs`; default `""`), and the third (optional) argument is the state (one of Idle, Info, Good, Warning, or Critical; default Idle).

Note that the text you set may need to be escaped, refer to [Escaping Text](#escaping-text).

#### Examples

```toml
[[block]]
block = "custom_dbus"
name = "CurrentSoundDevice"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`name` | Name of the DBus object that i3status-rs will create. Must be unique. | Yes | None

###### [↥ back to top](#list-of-available-blocks)

## Disk Space

Creates a block which displays disk space information.

#### Examples

```toml
[[block]]
block = "disk_space"
path = "/"
alias = "/"
info_type = "used"
unit = "GiB"
format = "{icon}{used}/{total} {unit} ({available}{unit} free)"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`alert` | Available disk space critical level as a percentage or Unit. | No | `10.0`
`alias` | Alias that is displayed for path. | No | `"/"`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{alias} {available} {unit}"`
`info_type` | Currently supported options are `"available"`, `"free"`, and `"used"` (sets value for alert and percentage calculation). | No | `"available"`
`interval` | Update interval, in seconds. | No | `20`
`path` | Path to collect information from. | No | `"/"`
`unit` | Unit that is used to display disk space. Options are `"MB"`, `"MiB"`, `"GB"`, `"GiB"`, `"TB"`, `"TiB"` and `"Percent"`. | No | `"GB"`
`warning` | Available disk space warning level as a percentage or Unit. | No | `20.0`
`alert_absolute` | Use Unit values for warning and alert instead of percentages. | No | `false`

#### Available Format Keys

Key | Value
----|-------
`{alias}` | Alias for disk path
`{available}` | Available disk space (free disk space minus reserved system space)
`{bar}` | Display bar representing percentage
`{free}` | Free disk space
`{icon}` | Disk drive icon
`{path}` | Path used for capacity check
`{percentage}` | Percentage of disk used or free (depends on info_type setting)
`{total}` | Total disk space
`{unit}` | Unit used for disk space (see above)
`{used}` | Used disk space

###### [↥ back to top](#list-of-available-blocks)

## Docker

Creates a block which shows the local docker daemon status (containers running, paused, stopped, total and image count).

#### Examples

```toml
[[block]]
block = "docker"
interval = 2
format = "{running}/{total}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `5`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{running}"`

#### Available Format Keys

Key | Value
----|-------
`{total}` | Total containers on the host
`{running}` | Containers running on the host
`{stopped}` | Containers stopped on the host
`{paused}` | Containers paused on the host
`{images}` | Total images on the host

###### [↥ back to top](#list-of-available-blocks)

## Focused Window

Creates a block which displays the title or the active marks of the currently focused window. Uses push updates from i3 IPC, so no need to worry about resource usage. The block only updates when the focused window changes title or the focus changes. Also works with sway, due to it having compatibility with i3's IPC.

#### Examples

```toml
[[block]]
block = "focused_window"
max_width = 50
show_marks = "visible"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`max_width` | Truncates titles to this length. | No | `21`
`show_marks` | Display marks instead of the title, if there are some. Options are `"none"`, `"all"` or `"visible"`, the latter of which ignores marks that start with an underscore. | No | `"none"`

###### [↥ back to top](#list-of-available-blocks)

## GitHub

Creates a block which shows the unread notification count for a GitHub account. A GitHub [personal access token](https://github.com/settings/tokens/new) with the "notifications" scope is requried, and must be passed using the `I3RS_GITHUB_TOKEN` environment variable.

#### Examples

```toml
[[block]]
block = "github"
format = "{total}|{author}|{comment}|{mention}|{review_requested}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `30`
`format` | AA string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{total}"`
`api_server`| API Server URL to use to fetch notifications. | No | `https://api.github.com`
`hide_if_total_is_zero` | Hide this block if the total count of notifications is zero | No | `false`

#### Available Format Keys

 Key | Value
-----|-------
`{total}` | Total number of notifications
`{assign}` | Total number of notifications related to issues you're assigned on
`{author}` | Total number of notifications related to threads you are the author of
`{comment}` | Total number of notifications related to threads you commented on
`{invitation}` | Total number of notifications related to invitations
`{manual}` | Total number of notifications related to threads you manually subscribed on
`{mention}` | Total number of notifications related to content you were specifically mentioned on
`{review_requested}` | Total number of notifications related to PR you were requested to review
`{security_alert}` | Total number of notifications related to security vulnerabilities found on your repositories
`{state_change}` | Total number of notifications related to thread state change
`{subscribed}` | Total number of notifications related to repositories you're watching
`{team_mention}` | Total number of notification related to thread where your team was mentioned

For more information about notifications, refer to the [GitHub API documentation](https://developer.github.com/v3/activity/notifications/#notification-reasons).

###### [↥ back to top](#list-of-available-blocks)

## Hueshift

Creates a block which display the current color temperature in Kelvin. When scrolling upon the block the color temperature is changed.
A left click on the block sets the color temperature to `click_temp` that is by default to `6500K`.
A right click completely resets the color temperature to its default value (`6500K`).

#### Examples

```toml
[[block]]
block = "hueshift"
hue_shifter = "redshift"
step = 50
click_temp = 3500
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`step`        | The step color temperature is in/decreased in Kelvin. | No | `100`
`hue_shifter` | Program used to control screen color. | No | Detect automatically. |
`max_temp`    | Max color temperature in Kelvin. | No | `10000`
`min_temp`    | Min color temperature in Kelvin. | No | `1000`
`click_temp`  | Left click color temperature in Kelvin. | No | `6500`

#### Available Hue Shifters

Name | Supports
-----|---------
`"redshift"`  | X11
`"sct"`       | X11
`"gammastep"` | X11 and Wayland


A hard limit is set for the `max_temp` to `10000K` and the same for the `min_temp` which is `1000K`.
The `step` has a hard limit as well, defined to `500K` to avoid too brutal changes.

###### [↥ back to top](#list-of-available-blocks)

## IBus

Creates a block which displays the current global engine set in [IBus](https://wiki.archlinux.org/index.php/IBus). Updates are instant as D-Bus signalling is used.

#### Examples

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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{engine}"`

#### Available Format Keys

Placeholder | Description
------------|-------------
`{engine}` | Engine name as provided by IBus

###### [↥ back to top](#list-of-available-blocks)

## KDEConnect

Display info from the currently connected device in KDEConnect, updated asynchronously.

Block colours are updated based on the battery level, unless all bat_* thresholds are set to 0, in which case the block colours will depend on the notification count instead.

```toml
[[block]]
block = "kdeconnect"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device_id` | Device ID as per the output of `kdeconnect --list-devices`. | No | Chooses the first found device, if any.
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{name} {bat_icon}{bat_charge}% {notif_icon}{notif_count}"`
`format_disconnected` | Same as `format` but for when the phone is disconnected/unreachable. Same placeholders can be used as above, however they will be fixed at the last known value until the phone comes back online. | No | `"{name}"`
`bat_info` | Min battery level below which state is set to info. | No | `60`
`bat_good` | Min battery level below which state is set to good. | No | `60`
`bat_warning` | Min battery level below which state is set to warning. | No | `30`
`bat_critical` | Min battery level below which state is set to critical. | No | `15`

#### Available Format Keys

Placeholder | Description
------------|-------------
`{bat_icon}` | Battery icon which will automatically change between the various battery icons depending on the current charge state
`{bat_charge}` | Battery charge level in percent
`{bat_state}` | Battery charging state, "true" or "false"
`{notif_icon}` | Will display an icon when you have a notification, otherwise an empty string
`{notif_count}` | Number of unread notifications on your phone
`{name}` | Name of your device as reported by KDEConnect
`{id}` | KDEConnect device ID

###### [↥ back to top](#list-of-available-blocks)

## Keyboard Layout

Creates a block to display the current keyboard layout.

Four drivers are available:
- `setxkbmap` which polls setxkbmap to get the current layout
- `localebus` which can read asynchronous updates from the systemd `org.freedesktop.locale1` D-Bus path
- `kbdd` which uses [kbdd](https://github.com/qnikst/kbdd) to monitor per-window layout changes via DBus
- `sway` which can read asynchronous updates from the sway IPC

Which of these methods is appropriate will depend on your system setup.

#### Examples

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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | One of `"setxkbmap"`, `"localebus"`, `"kbddbus"` or `"sway"`, depending on your system. | No | `"setxkbmap"`
`interval` | Update interval, in seconds. Only used by the `"setxkbmap"` driver. | No | `60`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{layout}"`
`sway_kb_identifier` | Identifier of the device you want to monitor, as found in the output of `swaymsg -t get_inputs`. | No | Defaults to first input found

#### Available Format Keys

  Key    | Value
---------|-------
`{layout}` | Keyboard layout name
`{variant}` | Keyboard variant (only `localebus` and `sway` are supported so far)

###### [↥ back to top](#list-of-available-blocks)

## Load

Creates a block which displays the system load average.

#### Examples

Display the 1-minute and 5-minute load averages, updated once per second:

```toml
[[block]]
block = "load"
format = "1min avg: {1m}"
interval = 1
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`info` | Minimum load, where state is set to info. | No | `0.3`
`warning` | Minimum load, where state is set to warning. | No | `0.6`
`critical` | Minimum load, where state is set to critical. | No | `0.9`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{1m}"`
`interval` | Update interval in seconds. | No | `3`

#### Available Format Keys

Placeholder | Description
------------|-------------
`{1m}` | 1 minute load average
`{5m}` | 5minute load average
`{15m}` | 15minute load average

###### [↥ back to top](#list-of-available-blocks)

## Maildir

Creates a block which shows unread mails. Only supports maildir format.

#### Examples

```toml
[[block]]
block = "maildir"
interval = 60
inboxes = ["/home/user/mail/local", "/home/user/mail/gmail/Inbox"]
threshold_warning = 1
threshold_critical = 10
display_type = "new"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`inboxes` | List of maildir inboxes to look for mails in. | Yes | None
`threshold_warning` | Number of unread mails where state is set to warning. | No | `1`
`threshold_critical` | Number of unread mails where state is set to critical. | No | `10`
`interval` | Update interval, in seconds. | No | `5`
`display_type` | Which part of the maildir to count: `"new"`, `"cur"`, or `"all"`. | No | `"new"`
`icon` | Whether or not to prepend the output with the mail icon. | No | `true`

###### [↥ back to top](#list-of-available-blocks)

## Memory

Creates a block displaying memory and swap usage.

This module keeps track of both Swap and Memory. By default, a click switches between them.

#### Examples

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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format_mem` | A string to customise the output of this block when in "Memory" view. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{MFm}MB/{MTm}MB({Mp}%)"`
`format_swap` | A string to customise the output of this block when in "Swap" view. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{SFm}MB/{STm}MB({Sp}%)"`
`display_type` | Default view displayed on startup: "`memory`" or "`swap`". | No | `"memory"`
`icons` | Whether the format string should be prepended with icons. | No | `true`
`clickable` | Whether the view should switch between memory and swap on click. | No | `true`
`warning_mem` | Percentage of memory usage, where state is set to warning. | No | `80.0`
`warning_swap` | Percentage of swap usage, where state is set to warning. | No | `80.0`
`critical_mem` | Percentage of memory usage, where state is set to critical. | No | `95.0`
`critical_swap` | Percentage of swap usage, where state is set to critical. | No | `95.0`
`interval` | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | `5`

#### Available Format Keys

  Key    | Value
---------|-------
`{MTg}`  | Memory total (GiB)
`{MTm}`  | Memory total (MiB)
`{MAg}`  | Available memory, including cached memory and buffers (GiB)
`{MAm}`  | Available memory, including cached memory and buffers (MiB)
`{MAp}`  | Available memory, including cached memory and buffers (%)
`{MApi}` | Available memory, including cached memory and buffers (%) as integer
`{MFg}`  | Memory free (GiB)
`{MFm}`  | Memory free (MiB)
`{MFp}`  | Memory free (%)
`{MFpi}` | Memory free (%) as integer
`{Mug}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (GiB)
`{Mum}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (MiB)
`{Mup}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (%)
`{Mupi}` | Memory used, excluding cached memory and buffers; similar to htop's green bar (%) as integer
`{MUg}`  | Total memory used (GiB)
`{MUm}`  | Total memory used (MiB)
`{MUp}`  | Total memory used (%)
`{MUpi}` | Total memory used (%) as integer
`{Cg}`   | Cached memory, similar to htop's yellow bar (GiB)
`{Cm}`   | Cached memory, similar to htop's yellow bar (MiB)
`{Cp}`   | Cached memory, similar to htop's yellow bar (%)
`{Cpi}`  | Cached memory, similar to htop's yellow bar (%) as integer
`{Bg}`   | Buffers, similar to htop's blue bar (GiB)
`{Bm}`   | Buffers, similar to htop's blue bar (MiB)
`{Bp}`   | Buffers, similar to htop's blue bar (%)
`{Bpi}`  | Buffers, similar to htop's blue bar (%) as integer
`{STg}`  | Swap total (GiB)
`{STm}`  | Swap total (MiB)
`{SFg}`  | Swap free (GiB)
`{SFm}`  | Swap free (MiB)
`{SFp}`  | Swap free (%)
`{SFpi}` | Swap free (%) as integer
`{SUg}`  | Swap used (GiB)
`{SUm}`  | Swap used (MiB)
`{SUp}`  | Swap used (%)
`{SUpi}` | Swap used (%) as integer

###### [↥ back to top](#list-of-available-blocks)

## Music

Creates a block to display the current song title and artist in a fixed-width marquee. Also provides buttons for play/pause, previous and next.
When there is no song playing the block collapses to show just the icon and any configured buttons.

Supports all music players that implement the [MediaPlayer2 Interface](https://specifications.freedesktop.org/mpris-spec/latest/Player_Interface.html). This includes:

- Spotify
- VLC
- mpd (via [mpDris2](https://github.com/eonpatapon/mpDris2))

and many others.

By default the block tracks all players available on the MPRIS bus. Right clicking on the block will cycle it to the next player (if the next player has no song playing then the block will collapse, however you can continue to right click to the next player.).  You can pin the widget to a given player via the "player" setting.

#### Examples

Show the currently playing song on Spotify only, with play & next buttons:

```toml
[[block]]
block = "music"
player = "spotify"
buttons = ["play", "next"]
```

Same thing for any compatible player, takes the first active on the bus, but ignores "mpd" or anything with "kdeconnect" in the name:

```toml
[[block]]
block = "music"
buttons = ["play", "next"]
interface_name_exclude = [".*kdeconnect.*", "mpd"]
```

Start Spotify if the block is clicked whilst it's collapsed:

```toml
[[block]]
block = "music"
on_collapsed_click = "spotify"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`player` | Name of the music player MPRIS interface. Run `busctl --user list \| grep "org.mpris.MediaPlayer2." \| cut -d' ' -f1` and the name is the part after "org.mpris.MediaPlayer2". If unset, you can cycle through different players by right clicking on the widget. | No | None
`interface_name_exclude` | A list of regex patterns for player MPRIS interface names to ignore. | No | ""
`max_width` | Max width of the block in characters, not including the buttons. | No | `21`
`dynamic_width` | Bool to specify whether the block will change width depending on the text content or remain static always (= `max_width`). | No | `false`
`marquee` | Bool to specify if a marquee style rotation should be used if the title + artist is longer than max-width. | No | `true`
`marquee_interval` | Marquee interval in seconds. This is the delay between each rotation. | No | `10`
`marquee_speed` | Marquee speed in seconds. This is the scrolling time used per character. | No | `0.5`
`smart_trim` | If title + artist is longer than max-width, trim from both the artist and the title in proportion to their lengths to try and show the most information possible. | No | `false`
`separator` | String to insert between artist and title. | No | `" - "`
`buttons` | Array of control buttons to be displayed. Options are prev (previous title), play (play/pause) and next (next title). | No | `[]`
`on_collapsed_click` | Command to run when the block is clicked while collapsed. | No | None
`on_click` | Command to run when the block is clicked while not collapsed. | No | None
`seek_step` | Number of microseconds to seek forward/backward when scrolling on the bar. | No | `1000`
`hide_when_empty` | Hides the block when there is no player available. | No | `false`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{combo}"`

#### Available Format Keys

  Key    | Value
---------|-------
`{artist}`  | Current artist (may be an empty string)
`{title}`  | Current title (may be an empty string)
`{combo}`  | Resolves to "`{artist}[sep]{title}"`, `"{artist}"`, or `"{title}"` depending on what information is available. `[sep]` is set by `separator` option. The `smart_trim` option affects the output.
`{player}`  | Name of the current player (taken from the last part of its MPRIS bus name)
`{avail}`  | Total number of players available to switch between

###### [↥ back to top](#list-of-available-blocks)

## Net

Creates a block which displays the upload and download throughput for a network interface. Units are by default in bytes per second (kB/s, MB/s, etc), 
but the 'use_bits' flag can be set to `true` to convert the units to bps (little b).

`bitrate` requires either `ethtool` for wired devices or `iw` for wireless devices.  
`ip` and `ipv6` require `ip`.  
`ssid` requires one of `iw`, `wpa_cli`, `nm-cli` or `iwctl`.  
`signal_strength` requires `iw`.

#### Examples

```toml
[[block]]
block = "net"
device = "wlp2s0"
format = "{ssid} {signal_strength} {ip} {speed_down} {graph_down}"
interval = 5
use_bits = false
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | Network interface to monitor (name from /sys/class/net). | No | Automatically chosen from the output of `ip route show default`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | "{speed_up} {speed_down}" 
`speed_digits` | Number of digits to use when displaying speeds. | No | `3`
`speed_min_unit` | Smallest unit to use when displaying speeds. Possible choices: `"B"`, `"K"`, `"M"`, `"G"`, `"T"`. | No | `"K"`
`use_bits` | Display speeds in bits instead of bytes. | No | `false`
`interval` | Update interval, in seconds. Note: the update interval for SSID and IP address is fixed at 30 seconds, and bitrate fixed at 10 seconds. | No | `1`
`hide_missing` | Whether to hide interfaces that don't exist on the system. | No | `false`
`hide_inactive` | Whether to hide interfaces that are not connected (or missing). | No | `false`

#### Available Format Keys

Placeholder | Description
------------|------------
`ssid` | Display network SSID (wireless only)
`signal_strength` | Display WiFi signal strength (wireless only)
`bitrate` | Display connection bitrate
`ip` | Display connection IP address
`ipv6` | Display connection IPv6 address
`speed_up` | Display upload speed
`speed_down` | Display download speed
`graph_up` | Display a bar graph for upload speed
`graph_down` | Display a bar graph for download speed

#### Deprecated Options

Key | Values | Required | Default
----|--------|----------|--------
`ssid` | Deprecated in favor of `format`. Display network SSID (wireless only). | No | `false`
`signal_strength` | Deprecated in favor of `format`. Display WiFi signal strength (wireless only). | No | `false`
`bitrate` | Deprecated in favor of `format`. Display connection bitrate. | No | `false`
`ip` | Deprecated in favor of `format`. Display connection IP address. | No | `false`
`ipv6` | Deprecated in favor of `format`. Display connection IPv6 address. | No | `false`
`speed_up` | Deprecated in favor of `format`. Display upload speed. | No | `true`
`speed_down` | Deprecated in favor of `format`. Display download speed. | No | `true`
`graph_up` | Deprecated in favor of `format`. Display a bar graph for upload speed. | No | `false`
`graph_down` | Deprecated in favor of `format`. Display a bar graph for download speed. | No | `false`

###### [↥ back to top](#list-of-available-blocks)

## NetworkManager

Creates a block which displays network connection information from NetworkManager.

#### Examples

```toml
[[block]]
block = "networkmanager"
on_click = "alacritty -e nmtui"
interface_name_exclude = ["br\\-[0-9a-f]{12}", "docker\\d+"]
interface_name_include = []
```

#### Options

Key | Values | Required | Default
----|--------|----------|---------
`primary_only` | Whether to show only the primary active connection or all active connections. | No | `false`
`max_ssid_width` | Truncation length for SSID. | No | `21`
`ap_format` | Acces point string formatter. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{ssid}"`
`device_format` | Device string formatter. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{icon}{ap} {ips}"`
`connection_format` | Connection string formatter. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{devices}"`
`on_click` | On-click handler. Commands are executed in a shell. | No | `""`
`interface_name_exclude` | A list of regex patterns for device interface names to ignore. | No | ""
`interface_name_include` | A list of regex patterns for device interface names to include (only interfaces that match at least one are shown). | No | ""

#### AP format string

Placeholder | Description
------------|-------------
`{ssid}` | The SSID for this AP
`{strength}` | The signal strength in percent for this AP
`{freq}` | The frequency of this AP in MHz

#### Device format string

Placeholder | Description
------------|-------------
`{icon}` | The icon matching the device type
`{typename}` | The name of the device type
`{name}` | The name of the device interface
`{ap}` | The connected AP if available, formatted with the AP format string
`{ips}` | The list of IPs for this device

#### Connection format string

Placeholder | Description
------------|-------------
`{devices}` | The list of devices, each formatted with the device format string

###### [↥ back to top](#list-of-available-blocks)

## Notify

Displays the current state of your notification daemon.

Note: For `dunst` this block uses DBus to get instantaneous updates. For now this requires building `dunst` from source (`dunst-git` from the AUR if you are on Arch Linux) until the next release of `dunst` comes out containing this feature: https://github.com/dunst-project/dunst/pull/766.

TODO: support `mako`

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | Notification daemon to monitor. | No | `"dunst"`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{state}"`

#### Available Format Keys

Key | Value
----|-------
`{state}` | Current state of the notification daemon in icon form

###### [↥ back to top](#list-of-available-blocks)

## Notmuch

Creates a block which queries a notmuch database and displays the count of messages.

The simplest configuration will return the total count of messages in the notmuch database stored at $HOME/.mail

NOTE: This block can only be used if you build with `cargo build --features=notmuch`

#### Examples

```toml
[[block]]
block = "notmuch"
query = "tag:alert and not tag:trash"
threshold_warning = 1
threshold_critical = 10
name = "A"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`maildir` | Path to the directory containing the notmuch database. | No | `$HOME/.mail`
`query` | Query to run on the database. | No | `""`
`threshold_critical` | Mail count that triggers `critical` state. | No | `99999`
`threshold_warning` | Mail count that triggers `warning` state. | No | `99999`
`threshold_good` | Mail count that triggers `good` state. | No | `99999`
`threshold_info` | Mail count that triggers `info` state. | No | `99999`
`name` | Label to show before the mail count. | No | `""`
`no_icon` | Disable the mail icon. | No | `false`
`interval` | Update interval in seconds. | No | `10`

###### [↥ back to top](#list-of-available-blocks)

## Nvidia Gpu

Creates a block which can display the name, utilization, temperature, memory usage, fan speed and clock speed of your NVidia GPU.

By default the name provided by `nvidia-smi` will be shown. If `label` is set then clicking the left mouse button on the "name" part of the block will alternate it between showing the default name or `label`.

By default `show_temperature` shows the used memory. Clicking the left mouse on the "temperature" part of the block will alternate it between showing used or total available memory.

When using `show_fan_speed`, clicking the left mouse button on the "fan speed" part of the block will cause it to enter into a fan speed setting mode. In this mode you can scroll the mouse wheel over the block to change the fan speeds, and left click to exit the mode.

Requires `nvidia-smi` for displaying info and `nvidia_settings` for setting fan speed.

#### Examples

```toml
[[block]]
block = "nvidia_gpu"
label = "GT 1030"
show_memory = false
show_clocks = true
interval = 1
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`gpu_id` | GPU id in system. | No | `0`
`label` | Display custom GPU label. | No | `""`
`interval` | Update interval in seconds. | No | `1`
`show_utilization` | Display GPU utilization percentage. | No | `true`
`show_memory` | Display memory information. | No | `true`
`show_temperature` | Display GPU temperature. | No | `true`
`show_fan_speed` | Display fan speed. | No | `false`
`show_clocks` | Display gpu clocks. | No | `false`

###### [↥ back to top](#list-of-available-blocks)

## Pacman

Creates a block which displays the pending updates available on pacman or an AUR helper.

Requires fakeroot to be installed (only required for pacman).

#### Examples

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

pacman only config using warnings with ZFS modules:

```toml
[[block]]
block = "pacman"
interval = 600
format = "{pacman} updates available"
format_singular = "{pacman} update available"
format_up_to_date = "system up to date"
# If a linux update is availble, but no ZFS package, it won't be possible to
# actually perform a system upgrade, so we show a warning.
warning_updates_regex = "(linux |linux-lts|linux-zen)"
# If ZFS is available, we know that we can and should do an upgrade, so we show 
# the status as critical.
critical_updates_regex = "(zfs |zfs-lts)"
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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `600`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{pacman}"`
`format_singular` | Same as `format` but for when exactly one update is available. | No | `"{pacman}"`
`format_up_to_date` | Same as `format` but for when no updates are available. | No | `"{pacman}"`
`warning_updates_regex` | Display block as warning if updates matching regex are available. | No | `None`
`critical_updates_regex` | Display block as critical if updates matching regex are available. | No | `None`
`aur_command` | AUR command to check available updates, which outputs in the same format as pacman. e.g. `pikaur -Qua` | if `{both}` or `{aur}` are used. | `None`
`hide_when_uptodate` | Hides the block when there are no updates available | `false`

### Available Format Keys

Key | Value
----|-------
`{count}` | Number of pacman updates available (**deprecated**: use `{pacman}` instead)
`{pacman}`| Number of updates available according to `pacman`
`{aur}` | Number of updates available according to `<aur_command>`
`{both}` | Cumulative number of updates available according to `pacman` and `<aur_command>` 

###### [↥ back to top](#list-of-available-blocks)

## Pomodoro

Creates a block which runs a [pomodoro timer](https://en.wikipedia.org/wiki/Pomodoro_Technique).

You can face problems showing the nagbar if i3 is configured to hide the status bar. See
[#701](https://github.com/greshake/i3status-rust/pull/701) to fix this.

#### Examples

```toml
[[block]]
block = "pomodoro"
length = 25
break_length = 5
message = "Take a break!"
break_message = "Back to work!"
use_nag = true
nag_path = "i3-nagbar"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`length` | Timer duration in minutes. | No | `25`
`break_length` | Break duration in minutes. | No | `5`
`use_nag` | i3-nagbar enabled. | No | `false`
`message` | i3-nagbar message when timer expires. | No | `Pomodoro over! Take a break!`
`break_message` | i3-nagbar message when break is over. | No | `Break over! Time to work!`
`nag_path` | i3-nagbar binary path. | No | `i3-nagbar`

###### [↥ back to top](#list-of-available-blocks)

## Sound

Creates a block which displays the volume level (according to PulseAudio or ALSA). Right click to toggle mute, scroll to adjust volume.

Requires a PulseAudio installation or `alsa-utils` for ALSA.

PulseAudio support is a feature and can be turned on (`--features "pulseaudio"`) / off (`--no-default-features`) during build with `cargo`.
If PulseAudio support is enabled the `"auto"` driver will first try to connect to PulseAudio and then fallback to ALSA on error.

Note that if you are using PulseAudio commands (such as `pactl`) to control your volume, you should select the `"pulseaudio"` (or `"auto"`) driver to see volume changes that exceed 100%.

#### Examples

Change the default scrolling step width to 3 percent:

```toml
[[block]]
block = "sound"
step_width = 3
```

```toml
[[block]]
block = "sound"
format = "{output_name} {volume}"
[block.mappings]
"alsa_output.usb-Harman_Multimedia_JBL_Pebbles_1.0.0-00.analog-stereo" = "🔈"
"alsa_output.pci-0000_00_1b.0.analog-stereo" = "🎧"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | `"auto"`, `"pulseaudio"`, `"alsa"`. | No | `"auto"` (Pulseaudio with ALSA fallback)
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `{volume}%`
`name` | PulseAudio device name, or the ALSA control name as found in the output of `amixer -D yourdevice scontrols`. | No | PulseAudio: `@DEFAULT_SINK@` / ALSA: `Master`
`device` | ALSA device name, usually in the form "hw:X" or "hw:X,Y" where `X` is the card number and `Y` is the device number as found in the output of `aplay -l`. | No | `default`
`device_kind` | PulseAudio device kind: `source` or `sink`. | No | `sink`
`natural_mapping` | When using the ALSA driver, display the "mapped volume" as given by `alsamixer`/`amixer -M`, which represents the volume level more naturally with respect for the human ear. | No | `false`
`step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | No | `5`
`max_vol` | Max volume in percent that can be set via scrolling. Note it can still be set above this value if changed by another application. | No | `None`
`on_click` | Shell command to run when the sound block is clicked. | No | None
`show_volume_when_muted` | Show the volume even if it is currently muted. | No | `false`

#### Available Format Keys

  Key    | Value
---------|-------
`{volume}` | Current volume in percent
`{output_name}` | PulseAudio or ALSA device name

###### [↥ back to top](#list-of-available-blocks)

## Speed Test

Creates a block which uses [`speedtest-cli`](https://github.com/sivel/speedtest-cli) to measure your ping, download, and upload speeds.

#### Examples

```toml
[[block]]
block = "speedtest"
bytes = true
interval = 1800
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`bytes` | Whether to use bytes or bits in the display (true for bytes, false for bits). | No | `false`
`interval` | Update interval in seconds. | No | `1800`
`speed_digits` | Number of digits to use when displaying speeds and latencies. | No | `3`
`speed_min_unit` | Smallest unit to use when displaying speeds. Possible choices: `"B"`, `"K"`, `"M"`, `"G"`, `"T"`. | No | `"K"`

###### [↥ back to top](#list-of-available-blocks)

## Taskwarrior

Creates a block which displays the number of tasks matching user-defined filters from the current user's taskwarrior list.

Clicking the left mouse button on the icon updates the number of tasks immediately.

Clicking the right mouse button on the icon cycles the view of the block through the user's filters.


#### Examples

```toml
[[block]]
block = "taskwarrior"
interval = 60
format = "{count} open tasks ({filter_name})"
format_singular = "{count} open task ({filter_name})"
format_everything_done = "nothing to do!"
warning_threshold = 10
critical_threshold = 20
[[block.filters]]
name = "today"
filter = "+PENDING +OVERDUE or +DUETODAY"
[[block.filters]]
name = "some-project"
filter = "project:some-project +PENDING"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `600` (10min)
`warning_threshold` | The threshold of pending (or started) tasks when the block turns into a warning state. | No | `10`
`critical_threshold` | The threshold of pending (or started) tasks when the block turns into a critical state. | No | `20`
`filter_tags` | Deprecated in favour of `filters`. A list of tags a task has to have before its counted as a pending task. The list of tags will be appended to the base filter `-COMPLETED -DELETED`. | No | ```<empty>```
`filters` | A list of tables with the keys `name` and `filter`. `filter` specifies the criteria that must be met for a task to be counted towards this filter. | No | ```[{name = "pending", filter = "-COMPLETED -DELETED"}]```
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{count}"`
`format_singular` | Same as `format` but for when exactly one task is pending. | No | `"{count}"`
`format_everything_done` | Same as `format` but for when all tasks are completed. | No | `"{count}"`

#### Available Format Keys

Key | Value
----|-------
`{count}` | The number of pending tasks
`{filter_name}` | The name of the current filter

###### [↥ back to top](#list-of-available-blocks)

## Temperature

Creates a block which displays the system temperature, based on lm_sensors' `sensors -j` output. The block has two modes: "collapsed", which uses only colour as an indicator, and "expanded", which shows the content of a `format` string.

Requires `lm_sensors` and appropriate kernel modules for your hardware.

The average, minimum, and maximum temperatures are computed using all sensors displayed by `sensors -j`, or optionally filtered by `chip` and `inputs`.

Note that the colour of the block is always determined by the maximum temperature across all sensors, not the average. You may need to keep this in mind if you have a misbehaving sensor.

#### Examples

```toml
[[block]]
block = "temperature"
collapsed = false
interval = 10
format = "{min}° min, {max}° max, {average}° avg"
chip = "*-isa-*"
inputs = ["CPUTIN", "SYSTIN"]
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval in seconds. | No | `5`
`collapsed` | Whether the block will be collapsed by default. | No | `true`
`scale` | Either `celsius` or `fahrenheit`. | No | `celsius`
`good` | Maximum temperature to set state to good. | No | `20` °C (`68` °F)
`idle` | Maximum temperature to set state to idle. | No | `45` °C (`113` °F)
`info` | Maximum temperature to set state to info. | No | `60` °C (`140` °F)
`warning` | Maximum temperature to set state to warning. Beyond this temperature, state is set to critical. | No | `80` °C (`176` °F)
`chip` | Narrows the results to a given chip name. `*` may be used as a wildcard. | No | None
`inputs` | Narrows the results to individual inputs reported by each chip. | No | None
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{average}° avg, {max}° max"`

#### Available Format Keys

Key | Value
----|-------
`{min}` | Minimum temperature among all sensors
`{average}` | Average temperature among all sensors
`{max}` | Maximum temperature among all sensors

###### [↥ back to top](#list-of-available-blocks)

## Time

Creates a block which display the current time.

#### Examples

```toml
[[block]]
block = "time"
format = "%a %d/%m %R"
timezone = "US/Pacific"
interval = 60
locale = "fr_BE"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See the [chrono docs](https://docs.rs/chrono/0.4/chrono/format/strftime/index.html#specifiers) for all options. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"%a %d/%m %R"`
`on_click` | Shell command to run when the time block is clicked. | No | None
`interval` | Update interval, in seconds. | No | `5`
`timezone` | A timezone specifier (e.g. "Europe/Lisbon"). | No | Local timezone
`locale` | Locale to apply when formatting the time. | No | System locale

###### [↥ back to top](#list-of-available-blocks)

## Toggle

Creates a toggle block. You can add commands to be executed to disable the toggle (`command_off`), and to enable it (`command_on`). If these command exit with a non-zero status, the block will not be toggled and the block state will be changed to give a visual warning of the failure.
You also need to specify a command to determine the initial state of the toggle (`command_state`). When the command outputs nothing, the toggle is disabled, otherwise enabled.
By specifying the `interval` property you can let the `command_state` be executed continuously.

#### Examples

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

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`text` | Label to include next to the toggle icon. | No | `""`
`command_on` | Shell Command to enable the toggle. | Yes | None
`command_off` | Shell Command to disable the toggle. | Yes | None
`command_state` | Shell Command to determine toggle state. Empty output => off. Any output => on. | Yes | None
`icon_on` | Icon override for the toggle button while on. | No | `"toggle_on"`
`icon_off` | Icon override for the toggle button while off. | No | `"toggle_off"`
`interval` | Update interval, in seconds. | No | None

###### [↥ back to top](#list-of-available-blocks)

## Uptime
Creates a block which displays system uptime. The block will always display the 2 biggest units, so minutes and seconds, or hours and minutes or days and hours or weeks and days.

#### Examples

```toml
[[block]]
block = "uptime"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval in seconds. | No | `60`

###### [↥ back to top](#list-of-available-blocks)

## Watson

[Watson](http://tailordev.github.io/Watson/) is a simple CLI time tracking application. This block will show the name of your current active project, tags and optionally recorded time. Clicking the widget will toggle the `show_time` variable dynamically.

#### Examples

```toml
[[block]]
block = "watson"
show_time = true
state_path = "/home/user/.config/watson/state"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`show_time` | Whether to show recorded time. | No | `false`
`state_path` | Path to the Watson state file. | No | `$XDG_CONFIG_HOME/watson/state`
`interval` | Update interval, in seconds. | No | `60`

## Weather

Creates a block which displays local weather and temperature information. In order to use this block, you will need access to a supported weather API service. At the time of writing, OpenWeatherMap is the only supported service.

Configuring the Weather block requires configuring a weather service, which may require API keys and other parameters.

If using the `autolocate` feature, set the block update interval such that you do not exceed ipapi.co's free daily limit of 1000 hits.

#### Examples

Show detailed weather in San Francisco through the OpenWeatherMap service:

```toml
[[block]]
block = "weather"
format = "{weather} ({location}) {temp}°, {wind} m/s {direction}"
service = { name = "openweathermap", api_key = "XXX", city_id = "5398563", units = "metric" }
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{weather} {temp}°"`
`service` | The configuration of a weather service (see below). | Yes | None
`interval` | Update interval, in seconds. | No | `600`
`autolocate` | Gets your location using the ipapi.co IP location service (no API key required). If the API call fails then the block will fallback to `city_id` or `place`. | No | false

#### OpenWeatherMap Options

To use the service you will need a (free) API key.

Key | Values | Required | Default
----|--------|----------|--------
`name` | `openweathermap`. | Yes | None
`api_key` | Your OpenWeatherMap API key. | Yes | None
`city_id` | OpenWeatherMap's ID for the city. | Yes* | None
`place` | OpenWeatherMap 'By city name' search query. See [here](https://openweathermap.org/current) | Yes* | None
`coordinates` | GPS latitude longitude coordinates as a tuple, example: `["39.236229089090216","9.331730718685696"]`
`units` | Either `metric` or `imperial`. | Yes | `metric`

One of `city_id`, `place` or `coordinates` is required. If more than one are supplied, `city_id` takes precedence over `place` which takes place over `coordinates`.

The options `api_key`, `city_id`, `place` can be omitted from configuration,
in which case they must be provided in the environment variables
`OPENWEATHERMAP_API_KEY`, `OPENWEATHERMAP_CITY_ID`, `OPENWEATHERMAP_PLACE`.

#### Available Format Keys

Key | Value
----|-------
`{location}` | Location name (exact format depends on the service)
`{temp}` | Temperature
`{apparent}` | Australian Apparent Temperature
`{humidity}` | Humidity
`{weather}` | Textual description of the weather, e.g. "Raining"
`{wind}` | Wind speed
`{wind_kmh}` | Wind speed. The wind speed in km/h.
`{direction}` | Wind direction, e.g. "NE"

###### [↥ back to top](#list-of-available-blocks)

## Xrandr

Creates a block which shows screen information (name, brightness, resolution). With a click you can toggle through your active screens and with wheel up and down you can adjust the selected screens brightness. Regarding brightness control, xrandr changes the brightness of the display using gamma rather than changing the brightness in hardware, so if that is not desirable then consider using the `backlight` block instead.

NOTE: Some users report issues (e.g. [here](https://github.com/greshake/i3status-rust/issues/274) and [here](https://github.com/greshake/i3status-rust/issues/668) when using this block. The cause is currently unknown, however setting a higher update interval may help.

#### Examples

```toml
[[block]]
block = "xrandr"
icons = true
resolution = true
interval = 2
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`icons` | Show icons for brightness and resolution. | No | `true`
`resolution` | Shows the screens resolution. | No | `false`
`step_width` | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50). | No | `5`
`interval` | Update interval in seconds. | No | `5`

###### [↥ back to top](#list-of-available-blocks)

## Escaping text
For blocks where the `format` string or `command` output can be configured by the user, you may need to escape any Pango characters otherwise the block may fail to render (i3) and/or throw errors to stderr (sway).

### List of characters that require escaping

Char | Escaped
-----|--------
  `<`  | `&lt;`
  `>`  | `&gt;`
  `&`  | `&amp;`
  `'`  | `&#39;`

e.g.
```toml
[[block]]
block = "custom"
# need to escape ampersand
#command = "echo '<b>1 &</b>'"
# escaped ampersand
command = "echo '<b>1 &amp;</b>'"
```

###### [↥ back to top](#list-of-available-blocks)
