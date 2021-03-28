# Profiling a block

Blocks can be profiled using [pprof](https://github.com/google/pprof) and thanks to the crate [cpuprofiler](https://crates.io/crates/cpuprofiler).

In order to profile a block, the project must be compiled in _debug_ mode with the _profiling_ feature enabled:

```
cargo build --features profiling
```

In order to profile a given block e.g. `load` block:

```
./target/debug/i3status-rs --profile load --profile-runs 10000 <config>
```

where `<config>` is a path to a toml config file.

It will generate a `load.profile` which can be visualized using `pprof`.
