# i3status-rust

![demo1](img/themes/solarized-dark.png)

`i3status-rs` is a feature-rich and resource-friendly replacement for i3status, written in pure Rust. It provides a way to display "blocks" of system information (time, battery status, volume, etc) on bars that support the [i3bar protocol](https://i3wm.org/docs/i3bar-protocol.html).

## Getting Started

Install from one of the packages below:

[![Packaging status](https://repology.org/badge/vertical-allrepos/i3status-rust.svg?columns=5&minversion=0.30.0&exclude_unsupported=1)](https://repology.org/project/i3status-rust/versions)

* For Fedora/CentOS, you can install from the [COPR](https://copr.fedorainfracloud.org/coprs/atim/i3status-rust/).

* For NixOS, you can also use [Home Manager](https://github.com/nix-community/home-manager): `programs.i3status-rust.enable = true` [see available options](https://nix-community.github.io/home-manager/options.xhtml#opt-programs.i3status-rust.enable)

* **NOTE:** Installation via `cargo` is not supported.

Otherwise refer to [manual install](doc/manual_install.md) docs.

## Configuration

After installing `i3status-rust`, edit the [example configuration](examples/config.toml) to your liking.
The default location is `$XDG_CONFIG_HOME/i3status-rust/config.toml`.

There are some optional global configuration variables, defined either at the top-level or in a [TOML table](https://github.com/toml-lang/toml/blob/main/toml.md#table).

`[icons]` table:
Key | Description | Default
----|-------------|----------
`icons` | The [icon set](doc/themes.md#available-icon-sets) that should be used. | `"none"`
`[icons.icons_overrides]` | Refer to `Themes and Icons` below. | None

`[theme]` table:
Key | Description | Default
----|-------------|----------
`theme` | The [theme](doc/themes.md#available-themes) that should be used. | `"plain"`
`[theme.theme_overrides]` | Refer to `Themes and Icons` below. | None

Global variables:
Key | Description | Default
----|-------------|----------
`icons_format` | A string to customise the appearance of each icon. Can be used to edit icons' spacing or specify a font that will be applied only to icons via pango markup. For example, `" <span font_family='NotoSans Nerd Font'>{icon}</span> "`. | `" {icon} "`
`invert_scrolling` | Whether to invert the direction of scrolling, useful for touchpad users. | `false`
`error_format` | A string to customise how block errors are displayed. See below for available placeholders. | `"$short_error_message\|X"`
`error_fullscreen_format` | A string to customise how block errors are displayed when clicked. See below for available placeholders. | `"$full_error_message"`

Available `error_format` and `error_fullscreen_format` placeholders:

Placeholder         | Value
--------------------|------
full_error_message  | The full error message
short_error_message | The short error message, if available

### Further documentation

#### Latest release

- [Blocks](https://docs.rs/i3status-rs/latest/i3status_rs/blocks/index.html)
- [Formatting](https://docs.rs/i3status-rs/latest/i3status_rs/formatting/index.html)
- [Themes and Icons](https://github.com/greshake/i3status-rust/blob/v0.32.0/doc/themes.md)

#### Master

- [Blocks](https://greshake.github.io/i3status-rust/i3status_rs/blocks/index.html)
- [Formatting](https://greshake.github.io/i3status-rust/i3status_rs/formatting/index.html)
- [Themes and Icons](doc/themes.md)

## Integrate it into i3/sway

Next, edit your bar configuration to use `i3status-rust`. For example:

```text
bar {
    font pango:DejaVu Sans Mono, FontAwesome 12
    position top
    status_command path/to/i3status-rs path/to/your/config.toml
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

In order to use the built-in support for the Font Awesome icon set, you will need to include it in the `font` parameter, as above. Check to make sure that "FontAwesome" will correctly identify the font by using `fc-match`, e.g.

```shell
$ fc-match FontAwesome
fontawesome-webfont.ttf: "FontAwesome" "Regular"
```

Note that the name of the Font Awesome font may have changed in version 5 or above.  
You can use `fc-list` to see the names of your available Awesome Fonts.

```shell
$ fc-list | grep -i awesome
/usr/share/fonts/TTF/fa-solid-900.ttf: Font Awesome 5 Free,Font Awesome 5 Free Solid:style=Solid
/usr/share/fonts/TTF/fa-regular-400.ttf: Font Awesome 5 Free,Font Awesome 5 Free Regular:style=Regular
```

In this example, you have to use `Font Awesome 5 Free` instead of the `FontAwesome 12` in the example configuration above.
You can verify the name again using `fc-match`

See [#130](https://github.com/greshake/i3status-rust/issues/130) for further discussion.

Finally, reload i3: `i3 reload`.

## Behavior

Each block has a `State` that defines its colors: one of "Idle", "Info", "Good", "Warning", "Critical" or "Error". The state is determined by the logic in each block, for example, the Music block state is "Info" when there is an active player.

When the state is "Error", a short error will be displayed in the block. The full message can be toggled by clicking on the block (overrides any click actions defined in the config). The block will be restarted after `error_interval` has elapsed.

i3bar has a "power savings" feature that pauses the bar via SIGSTOP when it is hidden or obscured by a fullscreen container. If this causes [issues](https://github.com/i3/i3/issues/4110) with your bar, try running i3status-rs with the `--never-stop` argument, which changes the signal sent by i3 from SIGSTOP to SIGCONT.

In addition to the per-block `signal` config option, i3status-rs can be signalled to force an update of all blocks by sending it the SIGUSR1 signal. It can also be restarted in place (useful for testing changes to the config file) by sending it the SIGUSR2 signal.

## Debugging

Run `i3status-rust` in a terminal to check the JSON it is outputting.  
In addition, some blocks have debug logs that can be enabled like so: `RUST_LOG=block=debug i3status-rs` where "block" is the block name.

## Contributing

We welcome new contributors! Take a gander at [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the GPLv3. See the [LICENSE](LICENSE) file for details.
