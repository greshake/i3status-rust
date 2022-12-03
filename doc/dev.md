## Requirements for Compilation

- `rustc`, `cargo` and `gcc`
- `libssl-dev`
- `libsensors-dev`
- `libpulse-dev` (required for `pulseaudio` driver of sound block, compile with `--no-default-features` to opt-out)

Compilation is only tested with very recent stable versions of `rustc`. If you use a distro with older Rust packages, consider using [rustup](https://rustup.rs/) to install a newer toolchain.

## Build and Install from Source

```shell
$ git clone https://github.com/greshake/i3status-rust
$ cd i3status-rust
$ cargo install --path . --locked
$ ./install.sh
```

By default, this will install the binary to `~/.cargo/bin/i3status-rs`.
