---
id: '11'
title: Publish updated univ-utils crate to crates.io
slug: publish-updated-univ-utils-crate-to-cratesio
status: closed
priority: 1
created_at: '2026-04-24T06:13:30.007023Z'
updated_at: '2026-04-24T06:18:04.453213Z'
notes: |-
  ---
  2026-04-24T06:14:43.153102+00:00
  Preflight status: package version bumped locally from 0.1.2 to 0.1.3 because crates.io already has univ-utils 0.1.2. Full verify passed: `cargo fmt --check && cargo clippy -p univ-utils -- -D warnings && cargo test -p univ-utils`. `cargo package -p univ-utils` required `--allow-dirty` due to uncommitted version bump/mana files; `cargo package -p univ-utils --allow-dirty --list` and `cargo package -p univ-utils --allow-dirty` both passed. Package contains only crate files plus normal cargo metadata (21 files). Awaiting explicit approval before `cargo publish -p univ-utils --allow-dirty`.
labels:
- release
- crates.io
- uu
closed_at: '2026-04-24T06:18:04.453213Z'
close_reason: GitHub push and crates.io publish succeeded.
verify: cargo package -p univ-utils
is_archived: true
history:
- attempt: 1
  started_at: '2026-04-24T06:18:03.075143Z'
  finished_at: '2026-04-24T06:18:04.436181Z'
  duration_secs: 1.361
  result: pass
  exit_code: 0
  output_snippet: 'verify passed: cargo package -p univ-utils'
kind: epic
paths:
- crates/uu/Cargo.toml
- Cargo.toml
- crates/uu/src/cmd/install.rs
- crates/uu/tests/cli.rs
- README.md
autonomy_disposition:
  kind: eligible
  review: unknown
  approval: unknown
  verify: satisfied
  visibility: unknown
  attempt_pressure: within_budget
  risk: unknown
  provenance: mixed
  continuation_budget: 3
---

Goal: prepare and publish the updated `univ-utils` crate after the hardened `uu install --default` changes.

Scope:
- Inspect current git status/diff so only intended files are included.
- Confirm package version in `crates/uu/Cargo.toml` and whether it needs bumping before publish.
- Run release preflight checks: `cargo fmt --check && cargo clippy -p univ-utils -- -D warnings && cargo test -p univ-utils`.
- Run `cargo package -p univ-utils` to validate crate contents.
- Stop and ask for explicit approval immediately before running `cargo publish -p univ-utils`.

Constraints:
- Do not publish without explicit final approval.
- Do not commit unless asked.
- Do not include unrelated dirty/untracked files in package assumptions; inspect package contents if needed.

Acceptance:
- Preflight and package validation pass.
- User approves publish and `cargo publish -p univ-utils` succeeds, or task is left ready-to-publish with clear blocker.
