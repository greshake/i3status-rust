# i3status-rust

![demo1](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/example_bar.png)

`i3status-rs` is a feature-rich and resource-friendly replacement for i3status, written in pure Rust. It provides a way to display "blocks" of system information (time, battery status, volume, etc) on the [i3](https://i3wm.org/) or [sway](http://swaywm.org/) bar.

## Requirements

Most blocks assume you are running Linux, and some have their own system requirements; refer to the block documentation.

Optional:

* Font Awesome 4.x/5.x/6.x is required when using the icons config `name = "awesome"`, `name = "awesome5"` or `name = "awesome6"`.
* For icons config `name = material`, a patched version of Google's MaterialIcons-Regular.ttf is required which includes \u{0020} (space), sets a descent ands lower all glyphs to properly align. It can be found [here](https://gist.github.com/draoncc/3c20d8d4262892ccd2e227eefeafa8ef/raw/3e6e12c213fba1ec28aaa26430c3606874754c30/MaterialIcons-Regular-for-inline.ttf).
* Powerline Fonts are required for all themes using the powerline arrow char.

## Getting Started

Stable releases are packaged on some distributions:

* On Arch Linux: `sudo pacman -Syu i3status-rust`

* On Fedora 31+: `sudo dnf install i3status-rs`. For older releases and CentOS, you can install from the [COPR](https://copr.fedorainfracloud.org/coprs/atim/i3status-rust/).

* On Void Linux: `xbps-install -S i3status-rust`

* On NixOS: `nix-env -iA nixos.i3status-rust`

* With [Home Manager](https://github.com/nix-community/home-manager): `programs.i3status-rust.enable = true` [see available options](https://nix-community.github.io/home-manager/options.html#opt-programs.i3status-rust.enable)

Otherwise refer to [manual install](https://github.com/greshake/i3status-rust/blob/master/doc/dev.md) docs

## Configuration

After installing `i3status-rust`, edit the [example configuration](https://raw.githubusercontent.com/greshake/i3status-rust/master/examples/config.toml) to your liking.
The default location is `$XDG_CONFIG_HOME/i3status-rust/config.toml`.

There are some top-level configuration variables:

Key | Description | Required | Default
----|-------------|----------|--------
`icons` | The icon set that should be used. | No | `none`
`icons_format` | A string to customise the appearance of each icon. Can be used to edit icons' spacing or specify a font that will be applied only to icons via pango markup. For example, set it to `" <span font_family='NotoSans Nerd Font'>{icon}</span> "` to set font of the icons to be 'NotoSans Nerd Font' | No | `" {icon} "`
`theme` | The predefined theme that should be used. You can also add your own overrides. | No | `plain`
`scrolling` | The direction of scrolling, either `natural` or `reverse` | No | `reverse`
`block` | All blocks that will exist in your bar. | No | none

Documentation | Latest release (v0.22) | Git master (v0.30)
--------------|------------------------|--------------------
Blocks        | [click](https://github.com/greshake/i3status-rust/blob/v0.22.0/doc/blocks.md) | [click](https://greshake.github.io/i3status-rust/i3status_rs/blocks/index.html)
Formatting    | [click](https://github.com/greshake/i3status-rust/blob/v0.22.0/doc/blocks.md#formatting) | [click](https://greshake.github.io/i3status-rust/i3status_rs/formatting/index.html)
Themes and Icons | [click](https://github.com/greshake/i3status-rust/blob/v0.22.0/doc/themes.md) | [click](https://github.com/greshake/i3status-rust/blob/master/doc/themes.md)

## Integrate it into i3

Next, edit your i3 bar configuration to use `i3status-rust`. For example:

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

Note that the name of the Font Awesome font may have changed in version 5.  
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

## Signalling

i3bar has a "power savings" feature that pauses the bar via SIGSTOP when it is hidden or obscured by a fullscreen container. If this causes [issues](https://github.com/i3/i3/issues/4110) with your bar, try running i3status-rs with the `--never-stop` argument, which changes the signal sent by i3 from SIGSTOP to SIGCONT.

i3status-rs can be signalled to force an update of all blocks by sending it the SIGUSR1 signal.

i3status-rs can also be restarted in place (useful for testing changes to the config file) by sending it the SIGUSR2 signal.

## Contributing

We welcome new contributors! Take a gander at [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the GPLv3. See the [LICENSE.md](LICENSE.md) file for details.
