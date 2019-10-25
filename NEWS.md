# i3status-rust 0.11.0

## New Blocks and Features

* Adds a new Docker block, which can display information about containers
  overseen by the Docker daemon. (#413 by @jlevesy)

* Adds a new Notmuch block for querying information from a Notmuch mail
  database. This block is currently an optional feature and must be enabled with
  `cargo build --feature notmuch`. (#215 by @bobthemighty and @atheriel)

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
