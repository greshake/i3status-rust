Custom Block Examples
========

This is a collection of more or less useful `custom` or `custom_dbus` block configs for use with `i3status-rust`.

Most of these are specifically designed for `sh` or compatible shells. Incompatible shells like `fish` might not work, in which case a `shell = "sh"` should be added to the block.

Feel free to add to the list below by sending a PR. Additional scripts can be added to the `scripts` subdirectory. Using blocks with external scripts naturally requires adjusting the script paths to your system.

## Custom Blocks

- [Hostname](#hostname)
- [HTTP Status Code](#http-status-code)
- [Intel GPU Usage](#intel-gpu-usage)
- [Kernel](#kernel)
- [Liquid cooling system status](#Liquid-cooling-system-status)
- [Monitors](#monitors)
- [Ping/RTT](#pingrtt)
- [Public IP](#public-ip)
- [Screenshot](#screenshot)
- [Switch GTK Theme](#switch-gtk-theme)
- [System (Suspend/Shutdown/Reboot)](#system-suspendshutdownreboot)
- [User](#user)
- [XKCD](#xkcd)
- [Spotify TUI](#spt)
- [Nextcloud](#nextcloud)

### Hostname

Show Hostname

```toml
[[block]]
block = "custom"
command = "cat /etc/hostname"
interval = "once"
```

### User

Show current user

```toml
[[block]]
block = "custom"
command = "whoami"
interval = "once"
```

### Kernel

Show current kernel and release

```toml
[[block]]
block = "custom"
command = "echo `uname` `uname -r | tr - . | cut -d. -f1-2`"
interval = "once"
```

### Public IP

Show public IP. Use `curl -4` or `curl -6` to get IPv4 or IPv6 respectively.

```toml
[[block]]
block = "custom"
command = "echo '\uf0ac ' `curl bot.whatismyipaddress.com`" # assumes fontawesome icons
interval = 60
```

### HTTP Status Code

Periodically check http status code for a given URL and set block status depending on the HTTP status code. Requires `curl` and [`http-status-code.sh`](scripts/http-status-code.sh).

```toml
[[block]]
block = "custom"
json = true
command = "~/Projects/i3status-rust/examples/scripts/http-status-code.sh https://example.com"
interval = 60
```

### Ping/RTT

Check ping periodically. `-c4` means average over 4 pings. Update on click.

```toml
[[block]]
block = "custom"
json = true
command = ''' echo "{\"icon\":\"ping\",\"text\":\"`ping -c4 1.1.1.1 | tail -n1 | cut -d'/' -f5`\"}" '''
interval = 60
on_click = "<command>"
```

### System (Suspend/Shutdown/Reboot)

Opens a `dmenu`/`rofi` menu to choose between suspend/poweroff/reboot. Uses `systemd`.

```toml
[[block]]
block = "custom"
command = "echo \uf011" # assumes fontawesome icons
on_click = "systemctl `echo -e 'suspend\npoweroff\nreboot' | dmenu`"
interval = "once"
```

### XKCD

Opens a random xkcd comic in the default browser. Requires working `xdg-open`.

```toml
[[block]]
block = "custom"
command = "echo xkcd"
on_click = "xdg-open 'https://c.xkcd.com/random/comic/'"
interval = "once"
```

### Screenshot

Take a screenshot from an interactively selected area (requires `scrot`), save it, fix it up (requires `imagemagick`) and copy to clipboard (requires `xclip`).

Optionally upload to imgbb and copy public link (requires `curl`, `jq`, `xclip`). See [`scripts/screenshot.sh`](scripts/screenshot.sh) for details and config.

```toml
[[block]]
block = "custom"
command = "echo \uf030" # assumes fontawesome icons
on_click = "~/Projects/i3status-rust/examples/scripts/screenshot.sh"
interval = "once"
```

### Monitors

List connected monitors by name, main monitor marked by `*`. Update on signal `SIGRTMIN+4`, e.g. for updating on `udev` events for monitor changes.

```toml
[[block]]
block = "custom"
command = "xrandr --listmonitors | tail -n+2 | tr '+' ' ' | cut -d' ' -f 4 | tr '\n' ' '"
interval = "once"
signal = 4
```

### Intel GPU Usage

Shows usage of the Render/3D pipeline of intel GPUs in percent. Requires `intel_gpu_top` installed and added to `/etc/sudoers` with `NOPASSWD` (Instructions see [here](https://unix.stackexchange.com/questions/18830/how-to-run-a-specific-program-as-root-without-a-password-prompt)). Video decode pipeline would be `awk '{print $14 "%"}'`.

```toml
[[block]]
block = "custom"
command = ''' sudo intel_gpu_top -l | head -n4 | tail -n1 | awk '{print $8 "%"}' '''
interval = 5
```

### Switch GTK Theme

Switch between a dark and a light GTK theme using `gsettings`.

```toml
[[block]]
block = "custom"
cycle = ["gsettings set org.gnome.desktop.interface gtk-theme Adapta; echo \U0001f311", "gsettings set org.gnome.desktop.interface gtk-theme None; echo \U0001f315"]
on_click = "<command>"
interval = "once"
```

And if you're feeling adventurous, here's a super sketchy version that also adjusts your `i3status-rs` and `i3bar` color scheme as well. The [`theme-switch.sh`](scripts/theme-switch.sh) script will likely need a lot of adjustments for your system before this works:

```toml
[[block]]
block = "custom"
command = "cat ~/.config/i3status-rust/mode.txt"
on_click = "~/Projects/i3status-rust/examples/scripts/theme-switch.sh"
interval = "once"
```

### Pi-hole status

Displays the status of Pi-hole server and number of ads blocked. Requires `curl`, `jq` and `xdg-open`.

```toml
[[block]]
block = "custom"
command = ''' curl --max-time 3 --silent 'http://pi.hole/admin/api.php?summary' | jq '{icon:"pi_hole", state: "\(.status | sub("enabled";"Good") | sub("disabled";"Warning"))", text: "\(.status | sub("enabled";"Up") | sub("disabled";"Down")) \(.ads_blocked_today)"}' '''
json = true
on_click = "xdg-open http://pi.hole"
interval = 180
```

**Note:**
Replace `http://pi.hole` with a correct url to your Pi-hole instance. Define icon override for `pi_hole`.
```toml
[icons.overrides]
pi_hole = ""
```

### Liquid cooling system status

Displays liquid temperature (celsius), fan and pump RPM. Requires: `liquidctl`.

![image](https://user-images.githubusercontent.com/20397027/118128928-7dfe8d00-b436-11eb-96b1-b40f62676933.png)

_Example for NZXT Kraken X series:_
```toml
[[block]]
block = "custom"
command = ''' liquidctl --match 'NZXT Kraken X' status | grep -e speed -e temp | awk '{printf "%s ", substr($0, 28,4)}' | awk '{printf " %s %s /%s", substr($0,0,4), substr($0,5,5), substr($0,10,6)}' '''
interval = 5
```

### Spotify TUI

Display song with [Spotify TUI](https://github.com/Rigellute/spotify-tui)

```toml
[[block]]
block = "custom"
command = "spt playback --format"
on_click = "spt playback --toggle"
interval = 3
```

### Nextcloud

Show Nextcloud GUI (if `nextcloud` is already running in background)

```toml
[[block]]
block = "custom"
command = "echo \uf0c2 Nextcloud" # icon is for nerdfont, replace if other
on_click = "nextcloud"
```
