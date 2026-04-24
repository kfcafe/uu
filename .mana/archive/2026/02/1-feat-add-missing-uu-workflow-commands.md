---
id: '1'
title: 'feat: add missing uu workflow commands'
slug: feat-add-missing-uu-workflow-commands
status: closed
priority: 2
created_at: 2026-02-27T04:04:59.230112Z
updated_at: 2026-02-27T04:50:02.559969Z
closed_at: 2026-02-27T04:50:02.559969Z
close_reason: 'Auto-closed: all children completed'
dependencies:
- '1.7'
verify: cargo fmt --check && cargo clippy -- -D warnings && cargo test && cargo run -p uu -- ci -n
fail_first: true
is_archived: true
tokens: 4669
tokens_updated: 2026-02-27T04:04:59.235473Z
---

## Goal
Add the missing workflow subcommands to `uu` so it can act as a more complete zero-config project helper.

New commands to add:
- `uu build`
- `uu fmt` (alias: `format`)
- `uu lint`
- `uu check` (alias: `typecheck`)
- `uu ci`
- `uu doctor` (alias: `info`)

## Constraints
- Follow existing patterns used by `install/run/test`:
  - detect project via `crates/uu/src/runner.rs::detect_project()`
  - produce a list of `Step`s (program + args)
  - execute via `runner::run_steps(kind, steps, dry_run)`
- Each new command should support `-n/--dry-run` and passthrough args after `--` where it makes sense.
- No new dependencies without explicit approval (avoid adding serde_json/toml parsers unless asked).
- Update docs and tests alongside code changes.

## Scope / deliverables
- New command modules in `crates/uu/src/cmd/` (one file per subcommand)
- `crates/uu/src/main.rs` updated to expose subcommands and any new argument structs
- `crates/uu/tests/cli.rs` updated/extended with coverage for the new commands (dry-run tests are fine)
- `README.md` updated:
  - add new command sections and examples
  - document any ecosystems where a command is intentionally unsupported

## Done when
- `uu --help` shows all new subcommands
- `cargo fmt --check && cargo clippy -- -D warnings && cargo test` passes
- `cargo run -p uu -- ci -n` exits 0 in this repo (Cargo project)
