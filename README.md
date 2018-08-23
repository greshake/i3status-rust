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

The Rust language and the `cargo` package manager are required to build the application.

We also require Libdbus 1.6 or higher. On some older systems this may require installing `libdbus-1-dev`. See [#194](https://github.com/greshake/i3status-rust/issues/194) if you are having dbus-related compilation issues.

Compilation is only tested with very recent stable versions of the `rustc`. If you use a distro with old Rust packages (looking at you, Ubuntu!), fall back to rustup or find a precompiled package for your distro.

Some blocks have their own system requirements; these are mentioned in the [block documentation](blocks.md).

Optional:

* Font Awesome, for `icons="awesome"`. Version 5 of the font is causing some issues (see [#130](https://github.com/greshake/i3status-rust/issues/130)), so for now we recommend version 4. If you have access to the AUR, check out [`ttf-font-awesome-4`](https://aur.archlinux.org/packages/ttf-font-awesome-4/).
* `gperftools` For dev builds, needed to profile block performance and bottlenecks.
* [`powerline-fonts`](https://www.archlinux.org/packages/community/x86_64/powerline-fonts/) For all themes using the powerline arrow char. Recommended. See [`powerline on GitHub`](https://github.com/powerline/powerline/tree/develop/font)

# How to use it
1. If you are using Arch Linux, you can install from the AUR: [`i3status-rust-git`](https://aur.archlinux.org/packages/i3status-rust-git/) and proceed to step 3. Otherwise, clone the repository: `git clone https://github.com/XYunknown/i3status-rust.git`
2. run `cd i3status-rust && cargo build --release`
3. Edit `example_config.toml` to your liking and put it to a sensible place (e.g. `~/.config/i3/status.toml`)
4. Edit your i3 bar configuration to use `i3status-rust`. For example:

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

5. Reload i3: `i3 reload`
