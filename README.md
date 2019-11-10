# i3status-rust

![demo1](https://raw.githubusercontent.com/XYunknown/i3status-rust/master/img/example_bar.png)

`i3status-rs` is a feature-rich and resource-friendly replacement for i3status, written in pure Rust. It provides a way to display "blocks" of system information (time, battery status, volume, etc) on the [i3](https://i3wm.org/) bar. It is also compatible with [sway](http://swaywm.org/).

For a list of available blocks, see the [block documentation](blocks.md). Further information can be found on the [Wiki](https://github.com/greshake/i3status-rust/wiki).

## Requirements

The Rust language and the `cargo` package manager are required to build the binary.

We also require Libdbus 1.6 or higher. On some older systems this may require installing `libdbus-1-dev`. See [#194](https://github.com/greshake/i3status-rust/issues/194) if you are having dbus-related compilation issues.

Compilation is only tested with very recent stable versions of `rustc`. If you use a distro with older Rust packages, consider using [rustup](https://rustup.rs/) to install a newer toolchain.

Most blocks assume you are running Linux, and some have their own system requirements; these are mentioned in the [block documentation](blocks.md).

Optional:

* Font Awesome is required for `icons="awesome"`. Version 5 of the font is causing some issues (see [#130](https://github.com/greshake/i3status-rust/issues/130)), so for now we recommend version 4. If you have access to the AUR, check out [`ttf-font-awesome-4`](https://aur.archlinux.org/packages/ttf-font-awesome-4/).
* Powerline Fonts are required for all themes using the powerline arrow char.
* `gperftools` is required for building with the `"profiling"` feature flag (disabled by default).

## Getting Started

Stable releases are packaged on some distributions:

- On Arch Linux, you can install from the AUR: [`i3status-rust`](https://aur.archlinux.org/packages/i3status-rust/) or [`i3status-rust-git`](https://aur.archlinux.org/packages/i3status-rust-git/).

- On Fedora or CentOS, you can install from the [COPR](https://copr.fedorainfracloud.org/coprs/atim/i3status-rust/).

- On Void Linux: `xbps-install -S i3status-rust`

- On NixOS: `nix-env -iA nixos.i3status-rust`

Otherwise, you can install from source:

```shell
$ git clone https://github.com/greshake/i3status-rust
$ cd i3status-rust && cargo build --release
# Optional:
$ cp target/release/i3status-rs ~/bin/i3status-rs
```

Now you need to create a configuration. Edit the [example configuration](https://raw.githubusercontent.com/greshake/i3status-rust/master/example_config.toml) to your liking and put it to a sensible place (e.g. `~/.config/i3/status.toml`).

Next, edit your i3 bar configuration to use `i3status-rust`. For example:

```
bar {
    font pango:DejaVu Sans Mono, FontAwesome 12
    position top
    status_command path/to/i3status-rs path/to/config.toml
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

``` shell
$ fc-match FontAwesome
fontawesome-webfont.ttf: "FontAwesome" "Regular"
```

(Note that the name of the Font Awesome font may have changed in version 5. See [#130](https://github.com/greshake/i3status-rust/issues/130) for some discussion.)

Finally, reload i3: `i3 reload`.

## Contributing

We welcome new contributors! Take a gander at [CONTRIBUTING.md](CONTRIBUTING.md).

## License

This project is licensed under the GPLv3. See the [LICENSE.md](LICENSE.md) file for details.
