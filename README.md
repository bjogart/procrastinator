# Procrastinator

Procrastinator is a tiny Rust crate for on-demand incremental computation.
You write your program as pure functions called _queries_,
and Procrastinator memoizes their results and tracks their dependencies.
When an input changes, only affected queries are re-evaluated,
and only once their result is needed.

This repository contains the crate,
its benchmarks,
and the scripts used to develop it.
To learn how to use Procrastinator,
start with the [crate README](crates/procrastinator/README.md).

## Repository layout

| Crate                                                 | Description                                                                       |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- |
| [`procrastinator`](crates/procrastinator)             | The library itself.                                                               |
| [`procrastinator_bench`](crates/procrastinator_bench) | Benchmarks that run synthetic query graphs and report counts and timings as JSON. |
| [`xtask`](crates/xtask)                               | Development scripts, run with `cargo xtask`.                                      |

Only `procrastinator` is meant for use outside this repository.
The other crates are internal to the workspace.

## Development

The workspace pins its Rust version in `rust-toolchain.toml`,
so `rustup` installs the right toolchain automatically.

Run the same checks as CI with:

```sh
cargo xtask test
```

It requires [`rg`](https://github.com/BurntSushi/ripgrep)
and [`taplo`](https://github.com/tamasfe/taplo) to be available on your `PATH`.

Run the benchmarks with:

```sh
cargo xtask bench
```

The benchmarks build with fat LTO, so expect a slow first build.

A few things to know before you send a change:

- CI is intentionally strict.
  Most Clippy lints are enabled, including the `restriction` group, and warnings are errors.
  Read the lint table in the root `Cargo.toml` to see what is allowed.
- Lints are suppressed with `#[expect(lint, reason = "...")]`, not with `#[allow]`.
- Tracked files may only contain Latin-1 characters and letters.
- `.gitignore` is a whitelist.
  A new file is ignored unless it matches an entry in `.gitignore`.
  Rust sources under `crates/*/src/` are already covered.

## Contributing

Contributions, questions, and pull requests are welcome!

Proposals are evaluated against the goals of Procrastinator:

- It stays small.
- It stays accessible to both users and contributors.
- It uses no `unsafe` code and no macros in user-facing APIs.
- It uses no non-local control flow:
  for example, cancellation does not rely on panics or unwinding.

## License

Procrastinator is licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT license](LICENSE-MIT)

at your option.

Unless you explicitly state otherwise,
any contribution intentionally submitted for inclusion in this repository by you,
as defined in the Apache-2.0 license,
shall be dual licensed as above,
without any additional terms or conditions.
