# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Verifying changes

- `cargo xtask test` is the full gate (it is all CI runs). In order: 
  `cargo test --workspace --all-targets`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check`,
  `cargo test --workspace --doc`,
  `cargo doc --workspace --no-deps`,
  a non-ASCII check via `rg`,
  `taplo fmt --check`.
  Needs `rg` and `taplo` on PATH.
- `cargo xtask bench` runs the release-mode benchmark (fat LTO, slow to build).
- Unit tests live in `crates/procrastinator/src/tests.rs`.

## Lints and style

- Lints are configured in `[workspace.lints]` in the root `Cargo.toml`: clippy `restriction`, `pedantic` and `cargo` groups are on and warnings are errors. So no `unwrap()`, no unchecked arithmetic (use `strict_add` etc.), no `print!`, no `unsafe`, `pub(crate)` instead of unreachable `pub`, every type implements `Debug`. Read the lint table before assuming something is allowed.
- Suppress a lint only with `#[expect(lint, reason = "...")]`, never `#[allow]`.
- Rustdoc lints are `deny`: intra-doc links must resolve, no bare URLs, no unescaped backticks.
- Item order within a module is enforced by `clippy.toml` (`mod`, `use`, consts, types, traits, then impls/fns).
- Conventions: one item per `use` line (no brace groups); prefer `core::`/`alloc::` paths over `std::`; bounds in `where` clauses; destructure `self` exhaustively (`let Self { a, b: _ } = self;`); invariant-violation panics start with `"bug: "`.
- No characters outside Latin-1 except letters anywhere in tracked files: no em-dashes, arrows, smart quotes or emoji in code, comments or docs.
- TOML files are formatted with `taplo` (default settings).

## Repo layout gotchas

- `.gitignore` is a whitelist (`*` then `!` entries). A new file of any kind other than `crates/*/src/**/*.rs` is untracked until it is added to `.gitignore`.
- The repo is a Jujutsu (`jj`) repo colocated with git. `.jj/` contains a full working-copy duplicate with `target/`; exclude `.jj/` from searches.
- Do not run any VCS operations (commit, describe, push, branch). The user handles version control.

## Architecture notes

- `set_input` and `gc` block until every `Snapshot` is dropped (`Shared::wait_exclusive`). Holding a snapshot on the same thread while mutating the `Db` deadlocks.
- Field order in `Shared` matters: `barrier` must drop after `data`.

## Intentional design decisions (do not flag as bugs)

- Cancellation must not panic or unwind (no Salsa-style `catch_unwind`). Returning `Option`/flags from every query was rejected for ergonomics. Cancellation is opt-in by calling `Snapshot::should_cancel`, with an additional option to abort query evaluation more quickly using `CancellationPolicy::stale`.
- `gc()` does not trigger cancellation, because gc only empties caches and never changes query outputs. `set_input` does cancel.
- Cyclic queries are bugs in query logic. If one occurs, a stack overflow should abort the process---as with regular infinite recursion.
- Queries can be computed twice in parallel. Results will be equal so this is never incorrect, though evaluation may on occasion be duplicated.
- `new_input` does not cancel or bump the revision, since new inputs cannot affect pre-existing memos.
- Evaluate proposals against the crate's goals: small, accessible, no macros, no `unsafe`, no panic-based control flow.