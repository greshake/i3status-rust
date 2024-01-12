## Requirements for Compilation

- `rustc`, `cargo` and `gcc`
- `libssl-dev`
- `libsensors-dev`
- `libpulse-dev` (required for `pulseaudio` driver of sound block, compile with `--no-default-features` to opt-out)
- `libnotmuch-dev` (required for optional `notmuch` block, compile with `--features notmuch` to opt-in)
- `libpipewire-0.3-dev` and `clang` (required for optional `pipewire` block, compile with `--features pipewire` to opt-in)

Compilation is only tested with very recent stable versions of `rustc`. If you use a distro with older Rust packages, consider using [rustup](https://rustup.rs/) to install a newer toolchain.

On systems using alternative (non-glibc) C standard libraries like `musl`, `cargo` must be configured to not link the libc statically. Otherwise, blocks needing to link to system libraries like `temperature`, `sound` (for pulseaudio) and maybe others will not be usable due to segmentation faults. To configure `cargo` for this, just add this to your `~/.cargo/config.toml`:

```toml
[build]
rustflags = ["-C", "target-feature=-crt-static"]
```

## Build and Install from Source

```shell
$ git clone https://github.com/greshake/i3status-rust
$ cd i3status-rust
$ cargo install --path . --locked
$ ./install.sh
```

By default, this will install the binary to `~/.cargo/bin/i3status-rs`, runtime files to `~/.local/share/i3status-rust` and manpage to `~/.local/share/man/man1/i3status-rs.1`

## Packaging

Runtime files from `files` directory are expected to be installed in `/usr/share/i3status-rust` or `$XDG_DATA_HOME/i3status-rust`.

Manual page at `man/i3status-rs.1` can be generated with `cargo xtask generate-manpage` (`pandoc` binary is required).
