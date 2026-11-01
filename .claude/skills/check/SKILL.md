---
name: check
description: Run the full procrastinator verification gate (cargo xtask test, same as CI) and diagnose failures. Use after making code changes, before telling the user work is done.
---

Run `cargo xtask test` from the repo root. It runs, stopping at the first failure:

1. `cargo test --workspace --all-targets`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo fmt --all --check`
4. `rg "[^\x{00}-\x{ff}\p{L}]"`: fails with "Non-ASCII, non-Latin characters" and lists the offending lines
5. `cargo doc --workspace --no-deps`: rustdoc lints are deny
6. `taplo fmt --check`

Before running, check that `rg` and `taplo` are on PATH. If `rg` is missing, step 4 is silently skipped, so tell the user the character check did not run. If `taplo` is missing, step 6 fails with a spawn error rather than a formatting error.

When a step fails:
- Clippy: fix the code to satisfy the lint (the workspace enables the `restriction` group, so expect lints like `unwrap_used`, `arithmetic_side_effects`, `indexing_slicing`). Only fall back to `#[expect(lint, reason = "...")]` when the lint is genuinely wrong for that spot, and say so to the user. Never use `#[allow]` and never loosen `[workspace.lints]` without asking.
- fmt: run `cargo fmt --all` (and `taplo fmt` for TOML), then re-run.
- Non-ASCII: replace the characters with ASCII equivalents (`--` or `-` for dashes, `->` for arrows, straight quotes).
- Tests: report the failing test and the assertion before changing anything; don't edit a test just to make it pass.

Re-run `cargo xtask test` until it passes, then report the result. If you stopped without a full pass, say exactly which step still fails.
