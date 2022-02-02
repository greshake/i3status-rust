Define blocks in your config file in the following format:

```toml
[[block]]
block = "insert-block-name-here"
```

Each block has different required or optional parameters that may be specified.  
In addition, there are some global config directives that can be applied to any block:

Config | Description | Default
-------|-------------|--------
`on_click` | Runs the specified command when the block is left clicked. This will override any default actions the block already has for left click. | None
`if_command` | Only enables the block if the specified command has a return code of 0. | None

Some blocks support format strings - refer to the [formatting section](#formatting) to see how to customize formatting strings' placeholders.

You may find that the block you desire is not in the list below. In that case, first see the [`custom` block examples](https://github.com/greshake/i3status-rust/blob/master/examples/README.md) for inspiration of how you can easily create additional blocks using the `custom` block.

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
- [ExternalIP](#externalip)
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
- [Rofication](#rofication)
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

Behind the scenes this uses `apt`, and in order to run it without root privileges i3status-rust will create its own package database in `/tmp/i3rs-apt/` which may take up several MB or more. If you have a custom apt config then this block may not work as expected - in that case please open an issue.

Tip: You can grab the list of available updates using `APT_CONFIG=/tmp/i3rs-apt/apt.conf apt list --upgradable`

#### Examples

Update the list of pending updates every thirty minutes (1800 seconds):

```toml
[[block]]
block = "apt"
interval = 1800
format = "{count:1} updates available"
format_singular = "{count:1} update available"
format_up_to_date = "system up to date"
critical_updates_regex = "(linux|linux-lts|linux-zen)"
# shows dmenu with cached available updates. Any dmenu alternative should also work.
on_click = "APT_CONFIG=/tmp/i3rs-apt/apt.conf apt list --upgradable | tail -n +2 | rofi -dmenu"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval in seconds. | No | `600`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{count:1}"`
`format_singular` | Same as `format`, but for when exactly one update is available. | No | `"{count:1}"`
`format_up_to_date` | Same as `format`, but for when no updates are available. | No | `"{count:1}"`
`warning_updates_regex` | Display block as warning if updates matching regex are available. | No | `None`
`critical_updates_regex` | Display block as critical if updates matching regex are available. | No | `None`

#### Available Format Keys

Key | Value | Type
----|-------|-----
`{count}` | Number of updates available | Integer

#### Notes

The number one in `{count:1}` sets the minimal width to one character.

#### Icons Used

- `update`

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

Setup bounds and cycle:

```toml
[[block]]
block = "backlight"
minimum = 15
maximum = 100
cycle = [100, 50, 0, 50]
```

Note that `cycle = []` will disable cycling, and `cycle = [n]` will reset brightness to `n` on each click

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | The `/sys/class/backlight` device to read brightness information from. | No | Default device
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{brightness}"`
`step_width` | The brightness increment to use when scrolling, in percent. | No | `5`
`minimum` | The minimum brightness that can be scrolled down to | No | `1`
`maximum` | The maximum brightness that can be scrolled up to | No | `100`
`cycle` | The brightnesses to cycle through on each click | No | `[minimum, maximum]`
`root_scaling` | Scaling exponent reciprocal (ie. root). | No | `1.0`
`invert_icons` | Invert icons' ordering, useful if you have colorful emoji. | No | `false`

Some devices expose raw values that are best handled with nonlinear scaling. The human perception of lightness is close to the cube root of relative luminance, so settings for `root_scaling` between 2.4 and 3.0 are worth trying. For devices with few discrete steps this should be 1.0 (linear). More information: <https://en.wikipedia.org/wiki/Lightness>

Also be aware that some devices turn off when brightness is set to `0`. Be careful when setting `minimum` to 0.

#### Available Format Keys

Placeholder | Description | Type
------------|-------------|-----
`{brightness}` | Device brightness percentage | String or Integer

#### Setting Brightness with the Mouse Wheel

The block allows for setting brightness with the mouse wheel and toggling min/max brightness on click. However, depending on how you installed i3status-rust, it may not have the appropriate permissions to modify these files, and will fail silently. To remedy this you can write a `udev` rule for your system (if you are comfortable doing so).

First, check that your user is a member of the "video" group using the `groups` command. Then add a rule in the `/etc/udev/rules.d/` directory containing the following, for example in `backlight.rules`:

```
ACTION=="add", SUBSYSTEM=="backlight", GROUP="video", MODE="0664"
```

This will allow the video group to modify all backlight devices. You will also need to restart for this rule to take effect.

#### Icons Used

- `backlight_empty` (when brightness between 0 and 6%)
- `backlight_1` (when brightness between 7 and 13%)
- `backlight_2` (when brightness between 14 and 20%)
- `backlight_3` (when brightness between 21 and 26%)
- `backlight_4` (when brightness between 27 and 33%)
- `backlight_5` (when brightness between 34 and 40%)
- `backlight_6` (when brightness between 41 and 46%)
- `backlight_7` (when brightness between 47 and 53%)
- `backlight_8` (when brightness between 54 and 60%)
- `backlight_9` (when brightness between 61 and 67%)
- `backlight_10` (when brightness between 68 and 73%)
- `backlight_11` (when brightness between 74 and 80%)
- `backlight_12` (when brightness between 81 and 87%)
- `backlight_13` (when brightness between 88 and 93%)
- `backlight_full` (when brightness above 94%)

###### [↥ back to top](#list-of-available-blocks)

## Battery

Creates a block which displays the current battery state (Full, Charging or Discharging), percentage charged and estimate time until (dis)charged.

The battery block collapses when the battery is fully charged -- or, in the case of some Thinkpad batteries, when it reports "Not charging".

The battery block supports reading charging and status information from either `sysfs`, [apcaccess](http://www.apcaccess.org/manual/manual.html#nis-server-client-configuration-using-the-net-driver), or the [UPower](https://upower.freedesktop.org/) D-Bus interface. These "drivers" have largely identical features, but UPower does include support for `device = "DisplayDevice"`, which treats all physical power sources as a single logical battery. This is particularly useful if your system has multiple batteries.

#### Examples

Update the battery state every ten seconds, and show the time remaining until (dis)charging is complete:

```toml
[[block]]
block = "battery"
interval = 10
format = "{percentage} {time}"
```

Same as previous, but also prints a six character bar.

```toml
[[block]]
block = "battery"
interval = 10
format = "{percentage:6#100} {percentage} {time}"
```

Rely on Upower for battery updates and information:

```toml
[[block]]
block = "battery"
driver = "upower"
format = "{percentage} {time}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | `sysfs`: The device in `/sys/class/power_supply/` to read from.<br />`apcaccess`: IPv4Address/hostname:port<br/>`UPower`: this can be `"DisplayDevice"` or any of the other paths found by running `upower --enumerate`. | No | `sysfs`: the first battery device found in `/sys/class/power_supply`, with "BATx" or "CMBx" entries taking precedence.<br />`apcaccess`: "localhost:3551"<br />`upower`:  `DisplayDevice``
`driver` | One of `"sysfs"`, `"apcaccess"`, or `"upower"`. | No | `"sysfs"`
`interval` | Update interval, in seconds. Only relevant for `driver = "sysfs" \|\| "apcaccess"`. | No | `10`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{percentage}"`
`full_format` | Same as `format` but for when the battery is full. | No | `"{percentage}"`
`missing_format` | Same as `format` but for when the specified battery is missing. | No | `"{percentage}"`
`allow_missing` | Don't display errors when the battery cannot be found. | No | `false`
`hide_missing` | Completely hide this block if the battery cannot be found. Only works in combination with `allow_missing`. | No | `false`
`full_threshold` | Percentage at which the battery is considered full (`full_format` shown) | No | `100`
`good` | Minimum battery level, where state is set to good. | No | `60`
`info` | Minimum battery level, where state is set to info. | No | `60`
`warning` | Minimum battery level, where state is set to warning. | No | `30`
`critical` | Minimum battery level, where state is set to critical. | No | `15`

#### Available Format Keys

Placeholder | Description | Type
------------|-------------|-----
`{percentage}` | Battery level, in percent | String or Integer
`{time}` | Time remaining until (dis)charge is complete | String
`{power}` | Power consumption by the battery or from the power supply when charging | String or Float

#### Icons Used

- `bat_charging`
- `bat_not_available`
- `bat_empty` (charge between 0 and 5%)
- `bat_quarter` (charge between 6 and 25%)
- `bat_half` (charge between 26 and 50%)
- `bat_three_quarters` (charge between 51 and 75%)
- `bat_full` (charge over 75%)

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
format = "Rowkin {percentage}"
format_unavailable = "Rowkin x"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`mac` | MAC address of the Bluetooth device. | Yes | None
`hide_disconnected` | Hides the block when the device is disconnected. | No | `false`
`format` | A string to customise the output of this block. See below for placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{label} {percentage}"`
`format_unavailable` | A string to customise the output of this block when the bluetooth controller is unavailable. See below for placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{label} x"`

#### Deprecated Options

Key | Values | Required | Default
----|--------|----------|--------
`label` | Text label to display next to the icon. | No | None

#### Available Format Keys

Key | Value | Type
----|-------|------
`{percentage}` | Device's charge in percents | Integer or an empty String

#### Deprecated Format Keys

Key | Value
----|-------
`{label}` | Device label as set in the block config

#### Icons Used

- `headphones` for bluetooth devices identifying as "audio-card"
- `joystick` for bluetooth devices identifying as "input-gaming"
- `keyboard` for bluetooth devices identifying as "input-keyboard"
- `mouse` for bluetooth devices identifying as "input-mouse"
- `bluetooth` for all other devices

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

#### Available Format Keys

Placeholder | Description | Type
------------|-------------|------
`{barchart}` | Bar chart of each CPU's core utilization | String
`{utilization}` | Average CPU utilization in percent | Integer
`{utilization<n>}` | CPU utilization in percent for core `n` | Integer
`{frequency}` | CPU frequency | Float
`{frequency<n>}` | CPU frequency in GHz for core `n` | Float
`{boost}` | CPU turbo boost status | String

#### Icons Used

- `cpu`
- `cpu_boost_on`
- `cpu_boost_off`

###### [↥ back to top](#list-of-available-blocks)

## Custom

Creates a block that display the output of custom shell commands.

For further customisation, use the `json` option and have the shell command output valid JSON in the schema below:  
`{"icon": "ICON", "state": "STATE", "text": "YOURTEXT"}`  
`icon` is optional, it may be an icon name from `icons.rs` (default "")  
`state` is optional, it may be Idle, Info, Good, Warning, Critical (default Idle)  

See [`examples`](https://github.com/greshake/i3status-rust/blob/master/examples/README.md) for a list of how many functionalities can be easily achieved using the `custom` block.

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

Update block when one or more specified files are modified:

```toml
[[block]]
block = "custom"
command = "cat ~/custom_status"
watch_files = ["~/custom_status"]
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
`watch_files` | Watch files to trigger update on file modification | No | None
`hide_when_empty` | Hides the block when the command output (or json text field) is empty. | No | false
`shell` | Specify the shell to use when running commands. | No | `$SHELL` if set, otherwise fallback to `sh`

###### [↥ back to top](#list-of-available-blocks)

## Custom DBus

Creates a block that can be updated asynchronously using DBus.

For example, updating the block using command line tools:  
busctl:  
`busctl --user call i3.status.rs /CurrentSoundDevice i3.status.rs SetStatus sss Headphones music Good`  
or  
`busctl --user call i3.status.rs /CurrentSoundDevice i3.status.rs SetStatus s Headphones`  

qdbus:
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
info_type = "used"
format = "{icon} {used}/{total} ({available} free)"
```

Same as previous, but the block will change it's state to "warning" if more than 40GB of disk space is used and to "alert" if more than 50GB is used.

```toml
[[block]]
block = "disk_space"
path = "/"
info_type = "used"
format = "{icon} {used}/{total} ({available} free)"
alert_absolute = true
unit = "GB"
alert = 50
warning = 40
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`alert` | Available disk space critical level as a percentage or Unit. | No | `10.0`
`warning` | Available disk space warning level as a percentage or Unit. | No | `20.0`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{available}"`
`info_type` | Currently supported options are `"available"`, `"free"`, and `"used"` (sets value for alert and percentage calculation). | No | `"available"`
`interval` | Update interval, in seconds. | No | `20`
`path` | Path to collect information from. | No | `"/"`
`unit` | Unit that is used when `alert_absolute` is set for `warning` and `alert`. Options are `"B"`, `"KB"` `"MB"`, `"GB"`, `"TB"`. | No | `"GB"`
`alert_absolute` | Use Unit values for warning and alert instead of percentages. | No | `false`

#### Deprecated Options
Key | Values | Required | Default
----|--------|----------|--------
`alias` | Sets the value for `{alias}` placeholder | No | `"/"`

#### Available Format Keys

Key | Value | Type
----|-------|-------
`{available}` | Available disk space (free disk space minus reserved system space) | Float
`{free}` | Free disk space | Float
`{icon}` | Disk drive icon | String
`{path}` | Path used for capacity check | String
`{percentage}` | Percentage of disk used or free (depends on info_type setting) | Float
`{total}` | Total disk space | Float
`{used}` | Used disk space | Float

#### Deprecated Format Keys

Key | Value | Type
----|-------|-------
`{alias}` | The value of `alias` option | String

#### Icons Used

- `disk_drive`

###### [↥ back to top](#list-of-available-blocks)

## Dnf

Creates a block which displays the pending updates available for your Fedora system.

#### Examples

Update the list of pending updates every thirty minutes (1800 seconds):

```toml
[[block]]
block = "dnf"
interval = 1800
format = "{count:1} updates available"
format_singular = "{count:1} update available"
format_up_to_date = "system up to date"
critical_updates_regex = "(linux|linux-lts|linux-zen)"
# shows dmenu with cached available updates. Any dmenu alternative should also work.
on_click = "dnf list -q --upgrades | tail -n +2 | rofi -dmenu"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval in seconds. | No | `600`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{count:1}"`
`format_singular` | Same as `format`, but for when exactly one update is available. | No | `"{count:1}"`
`format_up_to_date` | Same as `format`, but for when no updates are available. | No | `"{count:1}"`
`warning_updates_regex` | Display block as warning if updates matching regex are available. | No | `None`
`critical_updates_regex` | Display block as critical if updates matching regex are available. | No | `None`

#### Available Format Keys

Key | Value | Type
----|-------|-----
`{count}` | Number of updates available | Integer

#### Notes

The number one in `{count:1}` sets the minimal width to one character.

#### Icons Used

- `update`

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
`socket_path` | The path to the docker socket. | No | `"/var/run/docker.sock"`

#### Available Format Keys

Key | Value | Type
----|-------|-----
`{total}`   | Total containers on the host | Integer
`{running}` | Containers running on the host | Integer
`{stopped}` | Containers stopped on the host | Integer
`{paused}`  | Containers paused on the host | Integer
`{images}`  | Total images on the host | Integer

#### Icons Used

- `docker`

###### [↥ back to top](#list-of-available-blocks)

## ExternalIP

Creates a block which displays the external IP address and various information about it.

#### Examples

```toml
[[block]]
block = "external_ip"
format = "{ip} {country_code}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{address} {country_flag}"`
`interval` | Interval in seconds for automatic updates when the previous update was successful | No | 300
`error_interval` | Interval in seconds for automatic updates when the previous update failed | No | 15
`with_network_manager` | If 'true', listen for NetworkManager events and update the IP immediately if there was a change | No | "true"

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{ip}` | The external IP address, as seen from a remote server | String
`{version}` | IPv4 or IPv6 | String
`{city}` | City name, such as "San Francisco" | Integer
`{region}` | Region name, such as "California" | String
`{region_code}` | Region code, such as "CA" for California | String
`{country}` | Country code (2 letter, ISO 3166-1 alpha-2) | String
`{country_name}` | Short country name | String
`{country_code}` | Country code (2 letter, ISO 3166-1 alpha-2) | String
`{country_code_iso3}` | Country code (3 letter, ISO 3166-1 alpha-3) | String
`{country_capital}` | Capital of the country | String
`{country_tld}` | Country specific TLD (top-level domain) | String
`{continent_code}` | Continent code | String
`{in_eu}` | Region code, such as "CA" | String
`{postal}` | ZIP / Postal code | String
`{latitude}` | Latitude | Float
`{longitude}` | Longitude | Float
`{timezone}` | City | String
`{utc_offset}` | UTC offset (with daylight saving time) as +HHMM or -HHMM (HH is hours, MM is minutes) | String
`{country_calling_code}` | Country calling code (dial in code, comma separated) | String
`{currency}` | Currency code (ISO 4217) | String
`{currency_name}` | Currency name | String
`{languages}` | Languages spoken (comma separated 2 or 3 letter ISO 639 code with optional hyphen separated country suffix) | String
`{country_area}` | Area of the country (in sq km) | Float
`{country_population}` | Population of the country | Float
`{timezone}` | Time zone | String
`{org}` | Organization | String
`{asn}` | Autonomous system (AS) | String
`{country_flag}` | Flag of the country | String (glyph)

##### Notes
All the information comes from https://ipapi.co/json/ 
Check their documentation here: https://ipapi.co/api/#complete-location5

The IP is queried, 1) When i3status-rs starts, 2) When a signal is received
on D-Bus about a network configuration change, 3) Every 5 minutes. This
periodic refresh exists to catch IP updates that don't trigger a notification,
for example due to a IP refresh at the router.

Flags: They are not icons but unicode glyphs. You will need a font that
includes them. Tested with: https://www.babelstone.co.uk/Fonts/Flags.html

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
`format` | AA string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{combo}"`

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{title}` | Title | String
`{marks}` | Marks | String
`{combo}` | Title _or_ marks depending on whether the title is empty or not and show_marks is enabled or not | String

###### [↥ back to top](#list-of-available-blocks)

## GitHub

Creates a block which shows the unread notification count for a GitHub account. A GitHub [personal access token](https://github.com/settings/tokens/new) with the "notifications" scope is required, and must be passed using the `I3RS_GITHUB_TOKEN` environment variable. Optionally the colour of the block is determined by the highest notification in the following lists from highest to lowest: `critical`,`warning`,`info`,`good`

#### Examples

Display notification counts

```toml
[[block]]
block = "github"
format = "{total}|{author}|{comment}|{mention}|{review_requested}"
```

Display number of total notifications, change to info colour if there are any notifications, and warning colour if there is a mention or review_requested notification

```toml
[[block]]
block = "github"
format = "{total}"
info = ["total"]
warning = ["mention","review_requested"]
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Update interval, in seconds. | No | `30`
`format` | AA string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{total}"`
`api_server`| API Server URL to use to fetch notifications. | No | `https://api.github.com`
`hide_if_total_is_zero` | Hide this block if the total count of notifications is zero | No | `false`
`critical` | List of notification types that change the block to the critical colour | No | None
`warning` | List of notification types that change the block to the warning colour | No | None
`info` | List of notification types that change the block to the info colour | No | None
`good` | List of notification types that change the block to the good colour | No | None

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{total}` | Total number of notifications | Integer
`{assign}` | Total number of notifications related to issues you're assigned on | Integer
`{author}` | Total number of notifications related to threads you are the author of | Integer
`{comment}` | Total number of notifications related to threads you commented on | Integer
`{invitation}` | Total number of notifications related to invitations | Integer
`{manual}` | Total number of notifications related to threads you manually subscribed on | Integer
`{mention}` | Total number of notifications related to content you were specifically mentioned on | Integer
`{review_requested}` | Total number of notifications related to PR you were requested to review | Integer
`{security_alert}` | Total number of notifications related to security vulnerabilities found on your repositories | Integer
`{state_change}` | Total number of notifications related to thread state change | Integer
`{subscribed}` | Total number of notifications related to repositories you're watching | Integer
`{team_mention}` | Total number of notification related to thread where your team was mentioned | Integer

For more information about notifications, refer to the [GitHub API documentation](https://developer.github.com/v3/activity/notifications/#notification-reasons).

#### Icons Used

- `github`

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
`"wlsunset"` | Wayland
`"wl-gammarelay-rs"` | Wayland
`"wl-gammarelay"` | Wayland


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

 Key | Value | Type
-----|-------|-----
`{engine}` | Engine name as provided by IBus | String

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
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{name} {bat_icon}{bat_charge} {notif_icon}{notif_count}"`
`format_disconnected` | Same as `format` but for when the phone is disconnected/unreachable. Same placeholders can be used as above, however they will be fixed at the last known value until the phone comes back online. | No | `"{name}"`
`bat_info` | Min battery level below which state is set to info. | No | `60`
`bat_good` | Min battery level below which state is set to good. | No | `60`
`bat_warning` | Min battery level below which state is set to warning. | No | `30`
`bat_critical` | Min battery level below which state is set to critical. | No | `15`

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{bat_icon}` | Battery icon which will automatically change between the various battery icons depending on the current charge state | String
`{bat_charge}` | Battery charge level in percent | Integer
`{bat_state}` | Battery charging state, "true" or "false" | String
`{notif_icon}` | Will display an icon when you have a notification, otherwise an empty string | String
`{notif_count}` | Number of unread notifications on your phone | Integer
`{name}` | Name of your device as reported by KDEConnect | String
`{id}` | KDEConnect device ID | String

#### Icons Used

- `bat_charging`
- `bat_not_available`
- `bat_empty` (charge between 0 and 5%)
- `bat_quarter` (charge between 6 and 25%)
- `bat_half` (charge between 26 and 50%)
- `bat_three_quarters` (charge between 51 and 75%)
- `bat_full` (charge over 75%)
- `notification`
- `phone`
- `phone_disconnected`

###### [↥ back to top](#list-of-available-blocks)

## Keyboard Layout

Creates a block to display the current keyboard layout.

Four drivers are available:
- `setxkbmap` which polls setxkbmap to get the current layout
- `localebus` which can read asynchronous updates from the systemd `org.freedesktop.locale1` D-Bus path
- `kbddbus` which uses [kbdd](https://github.com/qnikst/kbdd) to monitor per-window layout changes via DBus
- `xkbswitch` which uses [xkb-switch](https://github.com/grwlf/xkb-switch) to show the current layout and variant. This works when `setxkbmap` is used to set a comma separated list of layouts, such as `us,es,fr`.
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

Use the [`xkb-switch`](https://github.com/grwlf/xkb-switch) X11 tool to switch to next `setxkbmap` layout on click:

```toml
[[block]]
block = "keyboard_layout"
driver = "setxkbmap"
on_click = "xkb-switch -n"
interval = 1
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

Poll `xkb-switch` for current layout and variant:

```toml
[[block]]
block = "keyboard_layout"
driver = "xkbswitch"
on_click = "xkb-switch -n"
format = "{layout} {variant}"
interval = 1
```

Listen to sway for changes:

```toml
[[block]]
block = "keyboard_layout"
driver = "sway"
sway_kb_identifier = "1133:49706:Gaming_Keyboard_G110"
```

Listen to sway for changes and override mappings:
```toml
[[block]]
block = "keyboard_layout"
driver = "sway"
format = "{layout}"
[block.mappings]
"English (Workman)" = "EN"
"Russian (N/A)" = "RU"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | One of `"setxkbmap"`, `"localebus"`, `"kbddbus"` or `"sway"`, depending on your system. | No | `"setxkbmap"`
`interval` | Update interval, in seconds. Only used by the `"setxkbmap"` driver. | No | `60`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{layout}"`
`sway_kb_identifier` | Identifier of the device you want to monitor, as found in the output of `swaymsg -t get_inputs`. | No | Defaults to first input found
`mappings` | Map `layout (variant)` to custom short name. | No | None

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{layout}` | Keyboard layout name | String
`{variant}` | Keyboard variant (only `localebus` and `sway` are supported so far) | String

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

 Key | Value | Type
-----|-------|-----
`{1m}` | 1 minute load average | Float
`{5m}` | 5minute load average | Float
`{15m}` | 15minute load average | Float

#### Icons Used

- `cogs`

###### [↥ back to top](#list-of-available-blocks)

## Maildir

Creates a block which shows unread mails. Only supports maildir format.

NOTE: This block can only be used if you build with `cargo build --features=maildir`

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
`icon` | Whether or not to prepend the output with the mail icon. **Deprecated**: set `icons_format=""` to hide the icon. | No | `true`

#### Icons Used

- `mail`

###### [↥ back to top](#list-of-available-blocks)

## Memory

Creates a block displaying memory and swap usage.

This module keeps track of both Swap and Memory. By default, a click switches between them.

#### Examples

```toml
[[block]]
block = "memory"
format_mem = "{mem_used}/{mem_total}({mem_used_percents})"
format_swap = "{swap_used}/{swap_total}({swap_used_percents})"
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
`format_mem` | A string to customise the output of this block when in "Memory" view. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{mem_free;M}/{mem_total;M}({mem_total_used_percents})"`
`format_swap` | A string to customise the output of this block when in "Swap" view. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{swap_free;M}/{swap_total;M}({swap_used_percents})"`
`display_type` | Default view displayed on startup: "`memory`" or "`swap`". | No | `"memory"`
`clickable` | Whether the view should switch between memory and swap on click. | No | `true`
`warning_mem` | Percentage of memory usage, where state is set to warning. | No | `80.0`
`warning_swap` | Percentage of swap usage, where state is set to warning. | No | `80.0`
`critical_mem` | Percentage of memory usage, where state is set to critical. | No | `95.0`
`critical_swap` | Percentage of swap usage, where state is set to critical. | No | `95.0`
`interval` | The delay in seconds between an update. If `clickable`, an update is triggered on click. Integer values only. | No | `5`

#### Deprecated options

Key | Values | Required | Default
----|--------|----------|--------
`icons` | Whether the format string should be prepended with icons. Deprecated - set `icons_format = ""` to disable icons. | No | `true`

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{mem_total}` | Memory total | Float
`{mem_free}` | Memory free | Float
`{mem_free_percents}`| Memory free % | Float
`{mem_total_used}`  | Total memory used | Float
`{mem_total_used_percents}`  | Total memory used % | Float
`{mem_used}` | Memory used, excluding cached memory and buffers; similar to htop's green bar | Float
`{mem_used_percents}`  | Memory used, excluding cached memory and buffers; similar to htop's green bar (in %) | Float
`{mem_avail}` | Available memory, including cached memory and buffers | Float
`{mem_avail_percents}` | Available memory, including cached memory and buffers (in %) | Float
`{swap_total}` | Swap total | Float
`{swap_free}` | Swap free | Float
`{swap_free_percents}` | Swap free % | Float
`{swap_used}` | Swap used | Float
`{swap_used_percents}` | Swap used | Float
`{buffers}` | Buffers, similar to htop's blue bar | Float
`{buffers_percent}` | Buffers, similar to htop's blue bar (in %) | Float
`{cached}` | Cached memory, similar to htop's yellow bar | Float
`{cached_percent}` | Cached memory, similar to htop's yellow bar (in %) | Float

#### Icons Used

- `memory_mem`
- `memory_swap`

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
`player` | Name of the music player MPRIS interface. Run `busctl --user list \| grep "org.mpris.MediaPlayer2." \| cut -d' ' -f1` and the name is the part after "org.mpris.MediaPlayer2.". If unset, you can cycle through different players by right clicking on the widget. | No | None
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

 Key | Value | Type
-----|-------|-----
`{artist}` | Current artist (may be an empty string) | String
`{title}`  | Current title (may be an empty string) | String
`{combo}`  | Resolves to "`{artist}[sep]{title}"`, `"{artist}"`, or `"{title}"` depending on what information is available. `[sep]` is set by `separator` option. The `smart_trim` option affects the output. | String
`{player}` | Name of the current player (taken from the last part of its MPRIS bus name) | String
`{avail}`  | Total number of players available to switch between | String

#### Icons Used

- `music`
- `music_next`
- `music_play`
- `music_prev`

###### [↥ back to top](#list-of-available-blocks)

## Net

Creates a block which displays the upload and download throughput for a network interface.

`bitrate` requires either `ethtool` for wired devices or `iw` for wireless devices.  
`ip` and `ipv6` require `ip`.  

#### Examples

Displays ssid, signal strength, ip, down speed and up speed as bits per second. Minimal prefix is set to `K` in order to prevent the block to change it's size.

```toml
[[block]]
block = "net"
device = "wlp2s0"
format = "{ssid} {signal_strength} {ip} {speed_down;K*b} {graph_down;K*b}"
interval = 5
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`device` | Network interface to monitor (name from /sys/class/net). | No | Automatically chosen from the output of `ip route show default`
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{speed_up;K} {speed_down;K}"`
`format_alt` | If set, block will switch its formatting between `format` and `format_alt` on every click. | No | None
`interval` | Update interval, in seconds. Note: the update interval for SSID and IP address is fixed at 30 seconds, and bitrate fixed at 10 seconds. | No | `1`
`hide_missing` | Whether to hide interfaces that don't exist on the system. | No | `false`
`hide_inactive` | Whether to hide interfaces that are not connected (or missing). | No | `false`

#### Available Format Keys

 Key | Value | Type | Unit
-----|-------|------|------
`ssid` | Network SSID (wireless only) | String | -
`signal_strength` | Display WiFi signal strength (wireless only) | Integer | %
`frequency` | WiFi frequency (wireless only) | Float | Hz
`bitrate` | Connection bitrate | String | -
`ip` | Connection IP address | String | -
`ipv6` | Connection IPv6 address | String | -
`speed_up` | Upload speed | Float | Bytes per second
`speed_down` | Download speed | Float | Bytes per second
`graph_up` | A bar graph for upload speed | String | -
`graph_down` | A bar graph for download speed | String | -

#### Icons Used

- `net_loopback`
- `net_vpn`
- `net_wired`
- `net_wireless`
- `net_up`
- `net_down`

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

Same as previous, but also limits the length of SSID to 10 characters.

```toml
[[block]]
block = "networkmanager"
on_click = "alacritty -e nmtui"
interface_name_exclude = ["br\\-[0-9a-f]{12}", "docker\\d+"]
interface_name_include = []
ap_format = "{ssid^10}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|---------
`primary_only` | Whether to show only the primary active connection or all active connections. | No | `false`
`ap_format` | Access point string formatter. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{ssid}"`
`device_format` | Device string formatter. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{icon}{ap} {ips}"`
`connection_format` | Connection string formatter. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{devices}"`
`interface_name_exclude` | A list of regex patterns for device interface names to ignore. | No | `""`
`interface_name_include` | A list of regex patterns for device interface names to include (only interfaces that match at least one are shown). | No | `""`

#### AP format string

 Key | Value | Type
-----|-------|-----
`{ssid}` | The SSID for this AP | String
`{strength}` | The signal strength in percent for this AP | Integer
`{freq}` | The frequency of this AP in MHz | String

#### Device format string

 Key | Value | Type
-----|-------|-----
`{icon}` | The icon matching the device type | String
`{typename}` | The name of the device type | String
`{name}` | The name of the device interface | String
`{ap}` | The connected AP if available, formatted with the AP format string | String
`{ips}` | The list of IPs for this device | String

#### Connection format string

 Key | Value | Type
-----|-------|-----
`{devices}` | The list of devices, each formatted with the device format string | String
`{id}` | ??? | String

#### Icons Used

- `net_bridge`
- `net_modem`
- `net_vpn`
- `net_wired`
- `net_wireless`
- `unknown`

###### [↥ back to top](#list-of-available-blocks)

## Notify

Displays the current state of your notification daemon.

Note: For `dunst` this block uses DBus to get instantaneous updates, which is only possible in dunst v1.6.0 and higher.

TODO: support `mako`

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{state}"`

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{state}` | Current state of the notification daemon in icon form | String

#### Icons Used

- `bell`
- `bell-slash`

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
`no_icon` | Disable the mail icon. **Deprecated**: set `icons_format=""` to disable hide the icon. | No | `false`
`interval` | Update interval in seconds. | No | `10`

#### Icons Used

- `mail`

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
`show_power_draw` | Display GPU power draw in watts. | No | `false`

#### Icons Used

- `gpu`

###### [↥ back to top](#list-of-available-blocks)

## Pacman

Creates a block which displays the pending updates available on pacman or an AUR helper.

Requires fakeroot to be installed (only required for pacman).

Tip: You can grab the list of available updates using `fakeroot pacman -Qu --dbpath /tmp/checkup-db-yourusername/`. If you have the CHECKUPDATES_DB env var set on your system then substitute that dir instead of /tmp/checkup-db-yourusername.

Tip: On Arch Linux you can setup a `pacman` hook to signal i3status-rs to update after packages have been upgraded, so you won't have stale info in your pacman block. Create `/usr/share/libalpm/hooks/i3status.hook` with the below contents:

Note: `pikaur` may hang the whole block if there is no internet connectivity. In that case, try a different AUR helper.
```ini
[Trigger]
Operation = Upgrade
Type = Package
Target = *

[Action]
When = PostTransaction
Exec = /usr/bin/pkill -SIGUSR1 i3status-rs
```

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
critical_updates_regex = "(linux|linux-lts|linux-zen)"
# pop-up a menu showing the available updates. Replace wofi with your favourite menu command.
on_click = "fakeroot pacman -Qu --dbpath /tmp/checkup-db-yourusername/ | wofi --show dmenu"
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
warning_updates_regex = "(linux|linux-lts|linux-zen)"
# If ZFS is available, we know that we can and should do an upgrade, so we show
# the status as critical.
critical_updates_regex = "(zfs|zfs-lts)"
```

pacman and AUR helper config:

```toml
[[block]]
block = "pacman"
interval = 600
format = "{pacman} + {aur} = {both} updates available"
format_singular = "{both} update available"
format_up_to_date = "system up to date"
critical_updates_regex = "(linux|linux-lts|linux-zen)"
# aur_command should output available updates to stdout (ie behave as echo -ne "update\n")
aur_command = "yay -Qua"
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
`aur_command` | AUR command to check available updates, which outputs in the same format as pacman. e.g. `yay -Qua` | if `{both}` or `{aur}` are used. | `None`
`hide_when_uptodate` | Hides the block when there are no updates available | `false`

### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{count}` | Number of pacman updates available (**deprecated**: use `{pacman}` instead) | Integer
`{pacman}`| Number of updates available according to `pacman` | Integer
`{aur}` | Number of updates available according to `<aur_command>` | Integer
`{both}` | Cumulative number of updates available according to `pacman` and `<aur_command>` | Integer

#### Icons Used

- `update`

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
notifier = "swaynag"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`length` | Timer duration in minutes. | No | `25`
`break_length` | Break duration in minutes. | No | `5`
`message` | Message displayed by notifier when timer expires. | No | `Pomodoro over! Take a break!`
`break_message` | Message displayed by notifier when break is over. | No | `Break over! Time to work!`
`notifier` | Notifier to use: `i3nag`, `swaynag`, `notifysend`, `none` | No | `none`
`notifier_path` | Override binary/path to run for the notifier | No | Defaults to `i3-nagbar`, `swaynag`, or `notify-send` depending on the value of `notifier` above.

#### Deprecated Options
Key | Values | Required | Default
----|--------|----------|--------
`use_nag` | i3-nagbar enabled. | No | `false`
`nag_path` | i3-nagbar binary path. | No | `i3-nagbar`

#### Icons Used

- `pomodoro`
- `pomodoro_started`
- `pomodoro_stopped`
- `pomodoro_paused`
- `pomodoro_break`

###### [↥ back to top](#list-of-available-blocks)

## Rofication

Creates a block with shows the number of pending notifications in rofication-daemon. A different color is used is there are critical notications. Left clicking the block opens the GUI.

#### Examples

```toml
[[block]]
block = "rofication"
interval = 1
socket_path = "/tmp/rofi_notification_daemon"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`interval` | Refresh rate in seconds. | No | `1`
`format` | A string to customise the output of this block. See below for placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{num}"`
`socket_path` | Socket path for the rofication daemon. | No | "/tmp/rofi_notification_daemon"

### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{num}` | Number of pending notifications | Integer

#### Icons Used

- `bell`
- `bell-slash`

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
format = "{output_description} {volume}"
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
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `{volume}`
`name` | PulseAudio device name, or the ALSA control name as found in the output of `amixer -D yourdevice scontrols`. | No | PulseAudio: `@DEFAULT_SINK@` / ALSA: `Master`
`device` | ALSA device name, usually in the form "hw:X" or "hw:X,Y" where `X` is the card number and `Y` is the device number as found in the output of `aplay -l`. | No | `default`
`device_kind` | PulseAudio device kind: `source` or `sink`. | No | `sink`
`natural_mapping` | When using the ALSA driver, display the "mapped volume" as given by `alsamixer`/`amixer -M`, which represents the volume level more naturally with respect for the human ear. | No | `false`
`step_width` | The percent volume level is increased/decreased for the selected audio device when scrolling. Capped automatically at 50. | No | `5`
`max_vol` | Max volume in percent that can be set via scrolling. Note it can still be set above this value if changed by another application. | No | `None`
`on_click` | Shell command to run when the sound block is clicked. | No | None
`show_volume_when_muted` | Show the volume even if it is currently muted. | No | `false`
`headphones_indicator` | Change icon when headphones are plugged in (pulseaudio only) | No | `false`

### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{volume}` | Current volume in percent | Integer
`{output_name}` | PulseAudio or ALSA device name | String
`{output_description}` | PulseAudio device description, will fallback to `output_name` if no description is available and will be overwritten by mappings (mappings will still use `output_name`) | String

#### Icons Used

- `microphone_muted`
- `microphone_empty` (1 to 20%)
- `microphone_half` (21 to 70%)
- `microphone_full` (over 71%)
- `volume_muted`
- `volume_empty` (1 to 20%)
- `volume_half` (21 to 70%)
- `volume_full` (over 71%)
- `headphones`

###### [↥ back to top](#list-of-available-blocks)

## Speed Test

Creates a block which uses [`speedtest-cli`](https://github.com/sivel/speedtest-cli) to measure your ping, download, and upload speeds.

#### Examples

Display speed in bits per second using 3 digits (defaults)

```toml
[[block]]
block = "speedtest"
interval = 1800
```

Display speed in bytes per second using 4 digits

```toml
[[block]]
block = "speedtest"
interval = 1800
format = "{ping}{speed_down:4*B}{speed_up:4*B}"
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{ping}{speed_down}{speed_up}"`
`interval` | Update interval in seconds. | No | `1800`

### Available Format Keys

 Key | Value | Type | Unit
-----|-------|------|------
`{ping}` | Ping delay | Float | Seconds
`{speed_down}` | Download speed | Float | Bits per second
`{speed_up}` | Upload speed | Float | Bits per second

#### Icons Used

- `ping`
- `net_down`
- `net_up`

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
`data_location`| Directory in which taskwarrior stores its data files. | No | "~/.task"`

Note: data_location is used to get instant notifications (changes in files inside that directory will trigger a check) only. The actual counts come from executing taskwarrior.

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{count}` | The number of pending tasks | Integer
`{filter_name}` | The name of the current filter | String

#### Icons Used

- `tasks`

###### [↥ back to top](#list-of-available-blocks)

## Temperature

Creates a block which displays the system temperature, based on `libsensors` library. The block has two modes: "collapsed", which uses only colour as an indicator, and "expanded", which shows the content of a `format` string.

Requires `libsensors` and appropriate kernel modules for your hardware.

The average, minimum, and maximum temperatures are computed using all sensors displayed by `sensors`, or optionally filtered by `chip` and `inputs`.

Note that the colour of the block is always determined by the maximum temperature across all sensors, not the average. You may need to keep this in mind if you have a misbehaving sensor.

#### Examples

```toml
[[block]]
block = "temperature"
collapsed = false
interval = 10
format = "{min} min, {max} max, {average} avg"
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
`chip` | Narrows the results to a given chip name. If driver = `"sensors"` then `*` may be used as a wildcard. If driver = `"sysfs"` then narrows to chips whose '"/sys/class/hwmon/hwmon*/name"' is a substring of the given chip name or vice versa. `sysfs` can not match to the bus such as `*-isa-*` or `*-pci-*`). | No | None
`inputs` | Narrows the results to individual inputs reported by each chip. | No | None
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{average} avg, {max} max"`

#### Deprecated options

Key | Values | Required | Default
----|--------|----------|--------
`driver` | One of `"sensors"` or `"sysfs"`. | No | `"sensors"`

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{min}` | Minimum temperature among all sensors | Integer
`{average}` | Average temperature among all sensors | Integer
`{max}` | Maximum temperature among all sensors | Integer

#### Icons Used

- `thermometer`

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

#### Icons Used

- `time`

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

#### Icons Used

- `toggle_off`
- `toggle_on`

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

#### Used Icons

- `uptime`

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
format = "{weather} ({location}) {temp}, {wind} m/s {direction}"
service = { name = "openweathermap", api_key = "XXX", city_id = "5398563", units = "metric" }
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | `"{weather} {temp}"`
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
`lang` | Language code. See [here](https://openweathermap.org/current#multi). Currently only affects `weather_verbose` key. | No | `en`

One of `city_id`, `place` or `coordinates` is required. If more than one are supplied, `city_id` takes precedence over `place` which takes place over `coordinates`.

The options `api_key`, `city_id`, `place` can be omitted from configuration,
in which case they must be provided in the environment variables
`OPENWEATHERMAP_API_KEY`, `OPENWEATHERMAP_CITY_ID`, `OPENWEATHERMAP_PLACE`.

#### Available Format Keys

 Key | Value | Type
-----|-------|-----
`{location}` | Location name (exact format depends on the service) | String
`{temp}` | Temperature | Integer
`{apparent}` | Australian Apparent Temperature | Integer
`{humidity}` | Humidity | Integer
`{weather}` | Textual brief description of the weather, e.g. "Raining" | String
`{weather_verbose}` | Textual verbose description of the weather, e.g. "overcast clouds" | String
`{wind}` | Wind speed | Float
`{wind_kmh}` | Wind speed. The wind speed in km/h. | Float
`{direction}` | Wind direction, e.g. "NE" | String

#### Used Icons

- `weather_sun` (when weather is reported as "Clear")
- `weather_rain` (when weather is reported as "Rain" or "Drizzle")
- `weather_clouds` (when weather is reported as "Clouds", "Fog" or "Mist")
- `weather_thunder` (when weather is reported as "Thunderstorm")
- `weather_snow` (when weather is reported as "Snow")
- `weather_default` (in all other cases)

###### [↥ back to top](#list-of-available-blocks)

## Xrandr

Creates a block which shows screen information (name, brightness, resolution). With a click you can toggle through your active screens and with wheel up and down you can adjust the selected screens brightness. Regarding brightness control, xrandr changes the brightness of the display using gamma rather than changing the brightness in hardware, so if that is not desirable then consider using the `backlight` block instead.

NOTE: Some users report issues (e.g. [here](https://github.com/greshake/i3status-rust/issues/274), [here](https://github.com/greshake/i3status-rust/issues/668) and [here](https://github.com/greshake/i3status-rust/issues/1364)) when using this block. The cause is currently unknown, however setting a higher update interval may help.

#### Examples

```toml
[[block]]
block = "xrandr"
icons = true
resolution = true
```

#### Options

Key | Values | Required | Default
----|--------|----------|--------
`format` | A string to customise the output of this block. See below for available placeholders. Text may need to be escaped, refer to [Escaping Text](#escaping-text). | No | Depends on `icons` and `resolution`. With default `icons` and `resolution` the default value is `"{display} {brightness_icon} {brightness}"``
`step_width` | The steps brightness is in/decreased for the selected screen (When greater than 50 it gets limited to 50). | No | `5`
`interval` | Update interval in seconds. | No | `5`

Placeholder         | Value                        | Type   | Unit
--------------------|------------------------------|--------|---------------
`{display}`         | The name of a monitor        | Text   | -
`{brightness}`      | The brightness of a monitor  | Number | %
`{brightness_icon}` | A static icon                | Icon   | -
`{resolution}`      | The resolution of a monitor  | Text   | -
`{res_icon}`        | A static icon                | Icon   | -

#### Deprecated options

Key | Values | Required | Default
----|--------|----------|--------
`icons` | Show icons for brightness and resolution. | No | `true`
`resolution` | Shows the screens resolution. | No | `false`

#### Used Icons

- `xrandr`
- `backlight_full`
- `resolution`

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

# Formatting

All blocks that have a `format` field can be reformatted by changing their format strings.

The field can be set as a string (`format = "{my_format}"`) or as a section:
```toml
[[block]]
block = "my_block"
[block.format]
full = "{my_full_format}"
short = "{my_short_format}"
```
Your `i3` or `sway` will switch all blocks over to the `short` variant whenever there isn't enough space on your screen for the `full` status bar.

## Syntax

The syntax for placeholders is

```
{<name>[:[0]<min width>][^<max width>][;[ ][_]<min prefix>][*[_]<unit>][#<bar max value>]}
```

### `<name>`

This is just a name of a placeholder. Each block that uses formatting will list them under "Available Format Keys" section of their config.

### `[0]<min width>`

Sets the minimum width of the content (in characters). If starts with a zero, `0` symbol will be used to pad the content. A space is used otherwise. Floats and Integers are shifted to the right, while Strings are to the left. Defaults to `0` for Strings, `2` for Integers and `3` for Floats.

#### Examples (spaces are shown as '□' to make the differences more obvious)

`"{var:3}"`

The value of `var` | Output
-------------------|--------
`"abc"`            | `"abc"`
`"abcde"`          | `"abcde"`
`"ab"`             | `"ab□"`
`1`                | `"□□1"`
`1234`             | `"1234"`
`1.0`              | `"1.0"`
`12.0`             | `"□12"`
`123.0`            | `"123"`
`1234.0`           | `"1234"`

### `<max width>`

Sets the maximum width of the content (in characters). Applicable only for Strings.

#### Examples

`"{var^3}"`

The value of `var` | Output
-------------------|--------
`"abc"`            | `"abc"`
`"abcde"`          | `"abc"`
`"ab"`             | `"ab"`

### `[ ][_]<min prefix>`

Float values are formatted following [engineering notation](https://en.wikipedia.org/wiki/Engineering_notation). This option sets the minimal SI prefix to use. The default value is `1` (no prefix) for bytes/bits and `n` (for nano) for everything else. Possible values are `n`, `u`, `m`, `1`, `K`, `M`, `G` and `T`.

Prepend an underscore `_` to hide the prefix (i.e. don't display it).

Prepend a space ` ` to add a space between the value and prefix.

#### Examples

`"{var:3;n}"`

The value of `var` | Output
-------------------|--------
`0.0001`           | "100u"
`0.001`            | "1.0m"
`0.01`             | " 10m"
`0.1`              | "100m"
`1.0`              | "1.0"
`12.0`             | " 12"
`123.0`            | "123"
`1234.0`           | "1.2K"

`"{var:3; 1}"`

The value of `var` | Output
-------------------|--------
`0.0001`           | "0.0 "
`0.001`            | "0.0 "
`0.01`             | "0.0 "
`0.1`              | "0.1 "
`1.0`              | "1.0 "
`12.0`             | " 12 "
`123.0`            | "123 "
`1234.0`           | "1.2 K"

`"{var:3;_K}"`

The value of `var` | Output
-------------------|--------
`1.0`              | "0.0"
`12.0`             | "0.0"
`123.0`            | "0.1"
`1234.0`           | "1.2"
`12345.0`          | " 12"

### `[_]<unit>`

Some placeholders have a "unit". For example, `net` block displays speed in bytes per second by default. This option gives ability to convert one units into another. Ignored for strings. Prepend the unit with the underscore `_` to hide the unit (i.e. don't display it).

#### The list of units

 Unit |         Means        | Displays
------|----------------------|---------
 B    | Bytes                | B
 b    | Bits                 | b
 %    | Percents             | %
 deg  | Degrees              | °
 s    | Seconds              | s
 W    | Watts                | W
 Hz   | Hertz                | Hz

#### Example

`"{speed_down*b}"` - show the download speed in bits per second.

`"{speed_down*_b}"` - show the download speed in bits per second, but hide the "b".

`"{speed_down*_}"` - show the download speed in it's default units, but hide the units.

`"{speed_down*_b}Bi/s"` - show the download in bits per second, and display the unit as "Bi/s" instead of "b".

### `<bar max value>`

Every numeric placeholder (Integers and Floats) can be drawn as a bar. This option sets the value to be considered "100%". If this option is set, every other option will be ignored, except for `min width`, which will set the length of a bar.

#### Example

```toml
[[block]]
block = "sound"
format = "{volume:5#110} {volume:03}"
```

Here, `{volume:5#110}` means "draw a bar, 5 character long, with 100% being 110.

Output: https://imgur.com/a/CCNw04e
