# i3status-rust 0.14.0

## New Blocks and Features

* New KDEConnect block (#717 by @ammgws)

* New CustomDBus block (#687 by @ammgws)

* New Network Manager block (#641 by @kennylevinsen). This block existed previously but was undocumented until it was overhauled completely by @kennylevinsen)

* New Taskwarrior block (#600 by @flying7eleven)

* New GitHub block (#425 by @jlevesy)

* Keyboard Layout block now supports `sway` (#670 by @ammgws), and also has a new `format` config option (#593 by @thiagokokada)

* IBus block now allows mapping of displayed engine to user configured value (#576 by @ammgws)

* Weather block now supports `humidity` and `apparent` (Australian Apparent Temperature) format specifiers (#640 by @ryanswilson59, @ammgws). Location can now also be set by name rather than ID using the new `place` option (#635 by @ammgws). Alternatively, the location can be guessed from your current IP address (#690 by @ammgws)

* Focused Window block new `show_marks` option to show marks instead of title (#532 by @ammgws)

* Net and Speedtest blocks now take `speed_min_unit` and `speed_digits` parameters to format speeds (#704, #707 by @GladOSkar, @ammgws). 

* Net block `ssid` config option now supports `iwctl` and `wpa_cli` (#625, #721 by @ammgws). Can now show bitrate for wired devices (#612 by @ammgws). New `ipv6` option (#647 by @ammgws)

* Pacman block now supports a `critical_updates_regex` parameter to control block state (#613 by @PicoJr), and now supports AUR as well (#658 by @PicoJr)

* Music block has a new `smart_trim` config option (#654 by @jgbyrne). Artist/title separator can now be customised with the `separator` option (#655 by @ammgws)

* Sound block now supports a `format` parameter (#618 by @jedahan). Along with that a format qualifier `output_name` was added which will show the name of the sink whose volume is being reported (#712 by @ammgws). ALSA driver: new `device` and `natural_mapping` options (#622 by @ammgws)

* CPU block now has `per_core` support for `{frequency}`, `{utilization}` (@grim7reaper)

* Block `interval` config can now take `"once"` in order to run blocks only one time (#684 by @PicoJr)

* Update font awesome icons to version 5 (#619 by @carloabelli)

* Add support for progress bars to some blocks (#578 by @carloabelli)

* Themes can now be read from standalone files (#611 by @atheriel & @PicoJr)

* New command line option `--never-pause` which will ignore any attempts by i3 to pause the bar when hidden/full-screen (#701 by @ammgws)

* If no config file path is supplied then we default to XDG_CONFIG_HOME/i3status-rust

## Bug Fixes and Improvements

* Net block fixed to support ppp vpn (#570 by @MiniGod). Device is now auto selected by default (#626 by @ammgws). Fixed error in `use_bits` calculation (#704 by @ammgws). Use /sys/class/net/<device>/carrier instead of operstate in is_up() (#605 by @happycoder97, @ammgws)

* Music block artist parsing from metadata fixed (#561 by @Riey)

* Fix panics for blocks without update intervals (#582 by @ammgws)

* Nvidia block: make threshold configurable, swap idle/good (#615 by @ammgws). Also fixed utilisation to have a fixed width (#566 by @TheJP)

* Backlight block now reads from actual_brightness as per kernel docs (#631 by @ammgws), with a special case for amdgpu backlights which don't follow the spec (#643 by @ammgws)

* Battery block now has a fallback for determining power consumption (#653 by @ammgws), and the time remaining is now only displayed when useful (#591 by @debugloop)

* Time block fixed to only register left mouse button clicks (#628 by @ammgws)

* Toggle block fixed to only toggle if command exited successfully (#648 by @ammgws)

* Fix missing icons for `bat_half` in the none theme (#719 by @varunkashyap) 

* Fix panic in CPU block if >32 CPUs present (#639 @snicket2100)

* Fix panic in Memory block when wrong placeholder given (#616 by @ammgws)

* Fix missing `good_bg` and `good_fg` theme overrides (#630 by @carloabelli)

* Unified handling of stdin and stdout to prevent broken pipe errors (#594 by @Celti)

* Travis CI will now run clippy for all features and targets (#682 by @rotty)

* Dependent crates have been updated to their latest versions (#729 by @ammgws, @ignatenkobrain)

## Documentation

* Document `info`, `good`, `warning`, `critical` parameters for the Battery block (@ammgws)

* Document `interval` for Notmuch, Uptime blocks (@ammgws)

* Fix error in Pomodoro block docs (#646 by @kAworu)

* Add profiling.md (#649 by @PicoJr)

* Adds a man page #556

# i3status-rust 0.13.1

* Dependent crates have been updated to their latest versions to make downstream
  packaging easier. This will become part of the normal release process in the
  future. (#540 by @rotty, #551 by @atheriel)

# i3status-rust 0.13.0

## New Blocks and Features

* The Net block now takes a `use_bits` parameter to display speeds in bit-based
  instead of byte-based units. (#496 by @hlmtre)

* The Pacman block now supports a `format` parameter. (#473 by @ifreund)

* The top-level config now takes a `scrolling` parameter that can be used to
  turn on `"natural"` mouse scrolling in the bar. (#494 by @bakhtiyarneyman)

* The Brightness block will now fall back to using D-Bus for changing the
  brightness if it cannot modify it via `sysfs`. (#499 by @majewsky)

* The Bluetooth block now allows for setting a text `label` parameter to keep
  track of devices. (#528 by @jeffw387)

## Bug Fixes and Improvements

* Fixes a panic that could sometimes manifest when restarting Pulseaudio. (#484
  by @ammgws)

* Fixes errors in the Pango markup we generate. (#518 by @ammgws)

* Fixes a potential panic when the Focused Window block was the only one in the
  configuration. (#535 by @ammgws)

* Fixes potential issues due to not ignoring `stdin` and `stdout` when spawning
  child processes. (#530 by @Celti)

* Improvements to the spacing around icons and IP addresses in the Net block.
  (#505 and #507 by @ammgws)

* Bumps several dependencies to fix security issues and reduce the number of
  transitive dependencies, which should improve build times. (#491, #492, #493,
  #510, #523 by @ammgws)

* Updates the installation documentation for Fedora. The project is now in the
  official repos! (#488 by @tim77)

* Simplifies the `udev` rule in the Brightness block docs. (#481 by @hellow554)

* Fixes a typo in the theme documentation. (#485 by @peeweep)

* Adds mention in the documentation that the Focused Window block is compatible
  with Sway. (#497 by @NilsIrl)

* Adds documentation for the optional Notmuch mail block. (#527 by @ammgws)

* Travis CI will now compile the project with all features enabled, which would
  have caught several bugs long ago. (#539 by @rotty)

# i3status-rust 0.12.0

## New Blocks and Features

* Wireguard devices are now correctly identified as VPNs in the net block. (#419
  by @vvrein)

* The keyboard layout block now has a `kbddbus` driver. (#451 by @sashomasho)

* Adds a new Pomodoro block. (#453 by @ghedamat)

## Bug Fixes and Improvements

* Fixes a panic in the iBus block due to the use of Perl regex features. (#443
  by @ammgws)

* Fixes more 32-bit build issues (e.g. for armv6 and i686). (#449 and #450 by
  @jcgruenhage)

* We now enforce `cargo fmt` on the codebase and in Travis CI. (#457 by
  @atheriel and @kennylevinsen, #474 by @ifreund)

* Improves parsing of `setxkbmap` output. (#458 by @sashomasho)

* Improvements to character width calculations in the rotating text widget.
  (#437 by @ammgws)

* Adds Fedora, NixOS, and Void Linux installation info to the `README`. (@tim77
  and @atheriel)

* The Font Awesome icons now use `bat_quarter` and `bat_three_quarters` for
  battery ranges. (#393 by @Ma27)

* Adds documentation for `hide_missing` and `hide_inactive` in the net block.
  (#476 by @bascht)

# i3status-rust 0.11.0

## New Blocks and Features

* Adds a new Docker block, which can display information about containers
  overseen by the Docker daemon. (#413 by @jlevesy)

* Adds a new Notmuch block for querying information from a Notmuch mail
  database. This block is currently an optional feature and must be enabled with
  `cargo build --features notmuch`. (#215 by @bobthemighty and @atheriel)

* The Weather block will now obey the `OPENWEATHERMAP_API_KEY` and
  `OPENWEATHERMAP_CITY_ID` environment variables. (#410 by @nicholasfagan)

* The Net block can now display wifi signal strength. (#418 by @bnjbvr)

* The project now has improved crate metadata, a proper `CONTRIBUTING.md` file,
  and will put release notes in a `NEWS.md` file. (by @atheriel)

## Bug Fixes and Improvements

* Updates the `nix` crate to fix broken builds on aarch64 with musl libc (#402).

* Fixes builds on i686. (#406 by @Gottox)

* Fixes a potential crash due to missing wind speed or direction in the Weather
  block. (#407 by @bramvdbogaerde).

* Fixes omission of UPower batteries that do not have a `battery_` prefix. (#423
  by @freswa)

* Fixes our use of now-deprecated dynamic trait and range syntax language
  features. (#428 by @duac)

* Prunes some transient dependencies. (#434 by @ohk2kt3t4 and @ammgws)

* Fixes our use of a deprecated flag in our `rustfmt` configuration. (#438 by
  @ammgws)

* Internal refactoring to reduce merge conflicts when adding new blocks. (by
  @atheriel)

# i3status-rust 0.10.0

* First tagged release.
