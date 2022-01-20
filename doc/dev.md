## Requirements for Compilation

The Rust compiler `rustc`, `cargo` package manager, C compiler `gcc`, `libsensors-dev` and `libssl-dev` packages are required to build the binary.

We also require Libdbus 1.6 or higher. On some older systems this may require installing `libdbus-1-dev`. 

Compilation is only tested with very recent stable versions of `rustc`. If you use a distro with older Rust packages, consider using [rustup](https://rustup.rs/) to install a newer toolchain.

## Build and Install from Source

```shell
$ git clone https://github.com/greshake/i3status-rust
$ cd i3status-rust
$ cargo install --path .
$ ./install.sh
```

By default, this will install the binary to `~/.cargo/bin/i3status-rs`.
