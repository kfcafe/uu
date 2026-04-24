---
id: '9'
title: Make uu install --default work across supported ecosystems
slug: make-uu-install-default-work-across-supported-ecos
status: closed
priority: 2
created_at: '2026-04-24T05:12:13.013947Z'
updated_at: '2026-04-24T05:15:50.521168Z'
acceptance: '`cargo fmt --check && cargo test -p univ-utils install` passes; dry-run tests prove non-Cargo `--default` schedules a post-install hook and Cargo `--default` still reports `would default`.'
labels:
- uu
- install
- default
closed_at: '2026-04-24T05:15:50.521168Z'
close_reason: Verified full targeted package checks pass.
verify: cargo fmt --check && cargo test -p univ-utils install
is_archived: true
history:
- attempt: 1
  started_at: '2026-04-24T05:15:50.007057Z'
  finished_at: '2026-04-24T05:15:50.499699Z'
  duration_secs: 0.492
  result: pass
  exit_code: 0
  output_snippet: 'verify passed: cargo fmt --check && cargo test -p univ-utils install'
outputs:
  text: |-
    running 11 tests
    test cmd::install::tests::cmake_has_three_phases ... ok
    test cmd::install::tests::dotnet_install ... ok
    test cmd::install::tests::node_uses_detected_manager ... ok
    test cmd::install::tests::python_prefers_uv ... ok
    test cmd::install::tests::swift_install ... ok
    test cmd::install::tests::zig_install ... ok
    test cmd::install::tests::cargo_steps ... ok
    test cmd::install::tests::cargo_workspace_steps_use_uu_crate_path ... ok
    test cmd::install::tests::default_hook_prefers_direct_executable ... ok
    test cmd::install::tests::install_skips_default_hook_without_default_flag ... ok
    test cmd::install::tests::install_adds_default_hook_when_present ... ok

    test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 120 filtered out; finished in 0.00s


    running 13 tests
    test install_default_dry_run_cargo_keeps_builtin_defaulting ... ok
    test install_dry_run_node_npm ... ok
    test install_dry_run_cmake ... ok
    test install_dry_run_go ... ok
    test install_dry_run_cargo ... ok
    test install_default_dry_run_uses_hook_for_node ... ok
    test install_default_dry_run_uses_hook_for_go ... ok
    test install_dry_run_cargo_workspace_uses_member_path ... ok
    test install_dry_run_with_extra_args ... ok
    test install_dry_run_workspace_with_extra_args ... ok
    test install_dry_run_node_yarn ... ok
    test install_dry_run_python ... ok
    test install_no_project_fails ... ok

    test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.27s
kind: epic
paths:
- crates/uu/src/cmd/install.rs
- crates/uu/src/main.rs
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

Goal: extend the recently added `uu install --default` behavior so it is not Rust/Cargo-only. Current state in `/Users/asher/uu`: `ProjectArgs` has `make_default`; `crates/uu/src/cmd/install.rs` has Cargo-specific `apply_cargo_default()` that copies Cargo-installed binaries to the user-preferred PATH entry, while non-Cargo projects only get optional `tools/uu-post-install` hooks. Implement a generic default hook path that all supported ProjectKind variants can use after install, while preserving the Cargo binary-copy behavior. Update tests/docs and verify.

Implementation constraints:
- Do not add dependencies without approval.
- Preserve existing user-local default behavior and dry-run output.
- Avoid destructive global installs beyond what install commands already do.
- Keep changes focused to install behavior/docs/tests unless inspection proves another file is required.
