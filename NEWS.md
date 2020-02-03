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
