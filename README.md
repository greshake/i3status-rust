# i3status-rust
![demo1](https://raw.githubusercontent.com/XYunknown/i3status-rust/master/img/example_bar.png)

Very resourcefriendly and feature-rich replacement for i3status, written in pure Rust

**For available blocks and detailed documentation visit the [Wiki](https://github.com/greshake/i3status-rust/wiki)**

# About this project
This is a replacement for i3status, aiming to provide the most feature-complete and resource friendly implementation of the i3bar protocol available. We are currently looking for help in implementing more Blocks and Themes! It supports:
- flexibility through theming
- icons (optional)
- individual update intervals per block to reduce system calls
- click actions
- blocks can trigger updates asynchronously, which allows for things like dbus signaling, to avoid periodic refreshing of data that rarely changes (example: music block)

# Requirements
i3, rustc, libdbus-dev and cargo. Only tested on Arch Linux.

Optional:
* `alsa-utils` For the volume block
* `lm_sensors` For the temperature block
* [`speedtest-cli`](https://github.com/sivel/speedtest-cli) For the speedtest block
* `ttf-font-awesome` For the awesome icons. If you want to use the font icons on Arch, install ttf-font-awesome from the AUR.
* `gperftools` For dev builds, needed to profile block performance and bottlenecks.
* [`powerline-fonts`](https://www.archlinux.org/packages/community/i686/powerline-fonts/) For all themes using the powerline arrow char. Recommended. See [`powerline on GitHub`](https://github.com/powerline/powerline/tree/develop/font)

# How to use it
1. If you are using Arch Linux, you can install from the AUR: [`i3status-rust-git`](https://aur.archlinux.org/packages/i3status-rust-git/) and proceed to step 3. Otherwise, clone the repository: `git clone https://github.com/XYunknown/i3status-rust.git`
2. run `cd i3status-rust && cargo build --release`
3. Edit `example_config.toml` to your liking and put it to a sensible place (e.g. `~/.config/i3/status.toml`)
4. Edit your i3 config
      1. In your i3 config, put the path to the output binary as argument for `status_command`
      2. Add the path to your config file as first and only argument to i3status-rs. See `i3status-rs --help` for more. **NOTE: You need to specify *font* in the bar section manually to use iconic fonts!**

            Example of the `bar` section in the i3 config from my personal i3 config (Requires awesome-ttf-fonts). The colors block is optional, just my taste:

            ```
            bar {
                  font pango:DejaVu Sans Mono, Icons 12
                  position top
                  status_command <PATH_TO_i3STATUS>/i3status-rs <PATH_TO_CONFIG>/config.toml
                  colors {
                        separator #666666
                        background #222222
                        statusline #dddddd
                        focused_workspace #0088CC #0088CC #ffffff
                        active_workspace #333333 #333333 #ffffff
                        inactive_workspace #333333 #333333 #888888
                        urgent_workspace #2f343a #900000 #ffffff
                  }
            }
            ```
5. Reload i3: `i3 reload`

# Breaking changes

`i3status-rs` is very much still in development, so breaking changes before a 1.0.0 release will occur. Following are guides on how to update your configurations to match breaking changes.

## Battery block changed

The battery block now uses the device name (*usually* BAT0) instead of the number after 'BAT'. This makes the block compatible with device names not starting with 'BAT'. To see your battery device(s) execute `ls /sys/class/power_supply`

## Configuration changed

Recently, the configuration has been changed:

* Switched from JSON to TOML
* Inlined the themes and icons configurations into the new main configuration
* Removed the command-line arguments `--theme` and `--icons`

Update your configuration to match the structure of the current [`example_config.toml`](https://github.com/greshake/i3status-rust/blob/master/example_config.toml):

```toml
theme = "solarized-dark"
icons = "awesome"

[[block]]
block = "disk_space"
path = "/"
alias = "/"
info_type = "available"
unit = "GB"
interval = 20

[[block]]
block = "memory"
display_type = "memory"
format_mem = "{Mup}%"
format_swap = "{SUp}%"

[[block]]
block = "cpu"
interval = 1

[[block]]
block = "load"
interval = 1
format = "{1m}"

[[block]]
block = "sound"

[[block]]
block = "time"
interval = 60
format = "%a %d/%m %R"
```

Things to note:

* Every `[[block]]` has to contain a `block`-field to identify the block to create
* Both `theme` and `icons` can be defined as tables, see [`example_theme.toml`](https://github.com/greshake/i3status-rust/blob/master/example_theme.toml) and [`example_icon.toml`](https://github.com/greshake/i3status-rust/blob/master/example_icon.toml)
