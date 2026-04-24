---
id: '8'
title: Fix uu install for Cargo workspace roots
slug: fix-uu-install-for-cargo-workspace-roots
status: open
priority: 2
created_at: '2026-04-24T05:12:00.960996Z'
updated_at: '2026-04-24T05:24:53.316441Z'
notes: |-
  ---
  2026-04-24T05:17:16.918675+00:00
  Implemented Cargo install path selection for workspace roots by detecting a workspace manifest plus crates/uu/Cargo.toml and switching install to `cargo install --path crates/uu`; package roots still use `--path .`. Kept non-Cargo post-install hook behavior intact and skipped hooks for Cargo `--default` as before. Added focused unit and CLI tests for workspace dry-runs and extra args, plus helper coverage for path detection. Verified with `cargo fmt && cargo test -p univ-utils install && cargo clippy -p univ-utils -- -D warnings`.

  ---
  2026-04-24T05:24:53.316431+00:00
  User rejected the repo-specific `crates/uu` special-case. Requirement clarified: `uu install` must remain project-agnostic. Need to replace the narrow fix with generic Cargo workspace handling (e.g. discover installable member packages/binaries from workspace metadata or manifest inspection) rather than hardcoding this repo layout.
verify: cargo test -p univ-utils install
verify_timeout: 120
kind: job
---

`uu install` currently maps Rust projects to `cargo install --path .`, which fails at workspace roots with a virtual manifest. Update install behavior so Cargo workspace roots like this repo install from `crates/uu` instead. Keep scope to the Rust install path selection and tests. Inspect existing install command generation in `crates/uu/src/cmd/install.rs`, implement a minimal path selection change for this workspace shape, and add/update focused tests covering workspace-root install steps.
