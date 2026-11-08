# `xtask`

This crate holds the build scripts for the Procrastinator workspace,
following the [cargo xtask](https://github.com/matklad/cargo-xtask) convention.
The scripts are written in Rust,
so they need nothing beyond a Rust toolchain and the tools they invoke.

This crate is internal to the Procrastinator workspace and is not meant to be published or used as a dependency.

## Usage

The workspace defines a Cargo alias in `.cargo/config.toml`,
so the scripts can be run from anywhere in the workspace with:

```sh
cargo xtask <command>
```

## Commands

### `cargo xtask test`

Runs the full verification suite.
This is the exact check that CI runs on pull requests.

The suite stops at the first failing step.
It requires [`rg`](https://github.com/BurntSushi/ripgrep)
and [`taplo`](https://github.com/tamasfe/taplo) to be available on your `PATH`.

### `cargo xtask bench`

Runs the benchmarks in `procrastinator_bench` with the release profile
and prints a JSON report to standard output.
See the README of `procrastinator_bench` for details on the benchmarks and the report format.