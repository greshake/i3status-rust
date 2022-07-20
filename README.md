# i3status-rust

![demo1](https://raw.githubusercontent.com/greshake/i3status-rust/master/img/themes/solarized-dark.png)

`i3status-rs` is a feature-rich and resource-friendly replacement for i3status, written in pure Rust. It provides a way to display "blocks" of system information (time, battery status, volume, etc) on the [i3](https://i3wm.org/) or [sway](http://swaywm.org/) bar.

## Getting Started

Install from one of the packages below:

[![Packaging status](https://repology.org/badge/vertical-allrepos/i3status-rust.svg?columns=5)](https://repology.org/project/i3status-rust/versions)

* For Fedora/CentOS, you can install from the [COPR](https://copr.fedorainfracloud.org/coprs/atim/i3status-rust/).

* For NixOS, you can also use [Home Manager](https://github.com/nix-community/home-manager): `programs.i3status-rust.enable = true` [see available options](https://nix-community.github.io/home-manager/options.html#opt-programs.i3status-rust.enable)

Otherwise refer to [manual install](https://github.com/greshake/i3status-rust/blob/master/doc/dev.md) docs

## Configuration

After installing `i3status-rust`, edit the [example configuration](https://raw.githubusercontent.com/greshake/i3status-rust/master/examples/config.toml) to your liking.
The default location is `$XDG_CONFIG_HOME/i3status-rust/config.toml`.

There are some optional top-level configuration variables:

Key | Description | Default
----|-------------|----------
`icons` | The [icon set](https://github.com/greshake/i3status-rust/blob/master/doc/themes.md#available-icon-sets) that should be used. | `none`
`icons_format` | A string to customise the appearance of each icon. Can be used to edit icons' spacing or specify a font that will be applied only to icons via pango markup. For example, set it to `" <span font_family='NotoSans Nerd Font'>{icon}</span> "` to set font of the icons to be 'NotoSans Nerd Font' | `" {icon} "`
`theme` | The [theme](https://github.com/greshake/i3status-rust/blob/master/doc/themes.md#available-themes) that should be used. | `plain`
`scrolling` | The direction of scrolling, either `natural` or `reverse`. | `reverse`
`block` | All blocks that will exist in your bar. | none

Documentation | Latest release (v0.22) | Git master (v0.30)
--------------|------------------------|--------------------
Blocks        | [click](https://github.com/greshake/i3status-rust/blob/v0.22.0/doc/blocks.md) | [click](https://greshake.github.io/i3status-rust/i3status_rs/blocks/index.html)
Formatting    | [click](https://github.com/greshake/i3status-rust/blob/v0.22.0/doc/blocks.md#formatting) | [click](https://greshake.github.io/i3status-rust/i3status_rs/formatting/index.html)
Themes and Icons | [click](https://github.com/greshake/i3status-rust/blob/v0.22.0/doc/themes.md) | [click](https://github.com/greshake/i3status-rust/blob/master/doc/themes.md)

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

## Debugging

Run `i3status-rust` in a terminal to check the JSON it is outputting.  
In addition, some blocks have debug logs that can be enabled like so: `RUST_LOG=block=debug i3status-rs` where "block" is the block name.

## Contributing

We welcome new contributors! Take a gander at [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the GPLv3. See the [LICENSE.md](LICENSE.md) file for details.
