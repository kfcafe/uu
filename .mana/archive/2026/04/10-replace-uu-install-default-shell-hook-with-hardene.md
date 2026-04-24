---
id: '10'
title: Replace uu install --default shell hook with hardened built-in/default manifest path
slug: replace-uu-install-default-shell-hook-with-hardene
status: closed
priority: 1
created_at: '2026-04-24T05:26:42.488352Z'
updated_at: '2026-04-24T05:35:22.063630Z'
labels:
- uu
- security
- install
- default
closed_at: '2026-04-24T05:35:22.063630Z'
close_reason: Acceptance met and full verify command passes.
verify: cargo fmt --check && cargo clippy -p univ-utils -- -D warnings && cargo test -p univ-utils
checkpoint: '47f85dbe124c712bc1ae869a5a210cf6617fcfdb'
verify_hash: '311437d4b26c30b704c76740c4ae386e57a53b23811b068b02734fca330e582a'
claimed_by: imp
claimed_at: '2026-04-24T05:29:32.600424Z'
is_archived: true
history:
- attempt: 1
  started_at: '2026-04-24T05:35:21.296872Z'
  finished_at: '2026-04-24T05:35:21.997528Z'
  duration_secs: 0.7
  result: pass
  exit_code: 0
  output_snippet: 'verify passed: cargo fmt --check && cargo clippy -p univ-utils -- -D warnings && cargo test -p univ-utils; no declared path overlap'
outputs:
  text: |-
    running 135 tests
    test cmd::build::tests::cargo_build ... ok
    test cmd::build::tests::cmake_has_two_phases ... ok
    test cmd::build::tests::dotnet_build ... ok
    test cmd::build::tests::node_uses_run_build ... ok
    test cmd::build::tests::kotlin_gradle_build ... ok
    test cmd::build::tests::python_with_uv ... ok
    test cmd::build::tests::meson_has_two_phases ... ok
    test cmd::build::tests::go_build ... ok
    test cmd::build::tests::swift_build ... ok
    test cmd::build::tests::zig_build ... ok
    test cmd::check::tests::cargo_check ... ok
    test cmd::check::tests::cmake_check ... ok
    test cmd::check::tests::dotnet_check ... ok
    test cmd::check::tests::elixir_check ... ok
    test cmd::check::tests::go_check_compiles_without_running ... ok
    test cmd::check::tests::gradle_no_wrapper_check ... ok
    test cmd::check::tests::gradle_wrapper_check ... ok
    test cmd::check::tests::kotlin_maven_check ... ok
    test cmd::check::tests::make_check ... ok
    test cmd::check::tests::maven_check ... ok
    test cmd::check::tests::meson_check ... ok
    test cmd::check::tests::node_npm_typecheck ... ok
    test cmd::check::tests::node_yarn_typecheck ... ok
    test cmd::check::tests::python_unsupported ... ok
    test cmd::check::tests::ruby_unsupported ... ok
    test cmd::check::tests::swift_check ... ok
    test cmd::check::tests::zig_check ... ok
    test cmd::ci::tests::cmake_ci ... ok
    test cmd::ci::tests::cargo_ci_has_three_steps ... ok
    test cmd::ci::tests::dotnet_ci ... ok
    test cmd::ci::tests::elixir_ci_has_three_steps ... ok
    test cmd::ci::tests::go_ci_has_three_steps ... ok
    test cmd::ci::tests::gradle_wrapper_ci ... ok
    test cmd::ci::tests::make_ci ... ok
    test cmd::ci::tests::maven_ci ... ok
    test cmd::ci::tests::meson_ci ... ok
    test cmd::ci::tests::node_npm_ci ... ok
    test cmd::ci::tests::node_yarn_ci ... ok
    test cmd::ci::tests::ruby_ci ... ok
    test cmd::ci::tests::swift_ci ... ok
    test cmd::ci::tests::zig_ci ... ok
    test cmd::clean::tests::dir_size_of_nonexistent_is_zero ... ok
    test cmd::clean::tests::human_size_bytes ... ok
    test cmd::clean::tests::human_size_gigabytes ... ok
    test cmd::clean::tests::human_size_kilobytes ... ok
    test cmd::clean::tests::human_size_megabytes ... ok
    test cmd::clean::tests::native_clean_for_cargo ... ok
    test cmd::clean::tests::native_clean_for_dotnet ... ok
    test cmd::clean::tests::native_clean_for_swift ... ok
    test cmd::clean::tests::no_native_clean_for_node ... ok
    test cmd::dev::tests::dotnet_dev_uses_watch ... ok
    test cmd::dev::tests::extract_127_url ... ok
    test cmd::dev::tests::extract_vite_url ... ok
    test cmd::dev::tests::no_url_returns_none ... ok
    test cmd::dev::tests::node_single_dev ... ok
    test cmd::dev::tests::non_node_fallback ... ok
    test cmd::dev::tests::swift_dev_falls_back_to_run ... ok
    test cmd::dev::tests::zig_dev ... ok
    test cmd::doctor::tests::cargo_tools_include_basics ... ok
    test cmd::doctor::tests::dotnet_tools ... ok
    test cmd::doctor::tests::go_tools_include_gofmt ... ok
    test cmd::doctor::tests::gradle_wrapper_omits_gradle_binary ... ok
    test cmd::doctor::tests::node_tools_match_manager ... ok
    test cmd::doctor::tests::swift_tools ... ok
    test cmd::doctor::tests::zig_tools ... ok
    test cmd::fmt::tests::cargo_fmt ... ok
    test cmd::fmt::tests::cmake_unsupported ... ok
    test cmd::fmt::tests::dotnet_fmt ... ok
    test cmd::fmt::tests::elixir_format ... ok
    test cmd::fmt::tests::go_fmt ... ok
    test cmd::fmt::tests::gradle_wrapper_format ... ok
    test cmd::fmt::tests::make_unsupported ... ok
    test cmd::clean::tests::dir_size_counts_files ... ok
    test cmd::fmt::tests::maven_unsupported ... ok
    test cmd::fmt::tests::node_npm_format ... ok
    test cmd::fmt::tests::node_yarn_format ... ok
    test cmd::fmt::tests::swift_unsupported ... ok
    test cmd::fmt::tests::zig_fmt ... ok
    test cmd::install::tests::cargo_steps ... ok
    test cmd::install::tests::cmake_has_three_phases ... ok
    test cmd::install::tests::dotnet_install ... ok
    test cmd::install::tests::cargo_install_target_detects_single_workspace_member ... ok
    test cmd::install::tests::cargo_install_target_errors_without_installable_member ... ok
    test cmd::install::tests::node_uses_detected_manager ... ok
    test cmd::install::tests::cargo_install_target_supports_globbed_workspace_members ... ok
    test cmd::install::tests::python_prefers_uv ... ok
    test cmd::install::tests::swift_install ... ok
    test cmd::install::tests::zig_install ... ok
    test cmd::lint::tests::cargo_lint ... ok
    test cmd::lint::tests::cmake_unsupported ... ok
    test cmd::lint::tests::dotnet_lint ... ok
    test cmd::install::tests::cargo_workspace_steps_use_uu_crate_path ... ok
    test cmd::lint::tests::elixir_lint ... ok
    test cmd::lint::tests::go_lint ... ok
    test cmd::lint::tests::gradle_wrapper_lint ... ok
    test cmd::lint::tests::make_unsupported ... ok
    test cmd::lint::tests::maven_unsupported ... ok
    test cmd::lint::tests::meson_unsupported ... ok
    test cmd::lint::tests::node_npm_lint ... ok
    test cmd::lint::tests::node_yarn_lint ... ok
    test cmd::lint::tests::ruby_lint ... ok
    test cmd::lint::tests::swift_unsupported ... ok
    test cmd::lint::tests::zig_unsupported ... ok
    test cmd::ports::tests::parse_ipv4_listen_line ... ok
    test cmd::ports::tests::parse_ipv6_listen_line ... ok
    test cmd::install::tests::cargo_install_target_errors_for_multiple_workspace_members ... ok
    test cmd::ports::tests::parse_non_numeric_pid_returns_none ... ok
    test cmd::ports::tests::parse_localhost_binding ... ok
    test cmd::ports::tests::parse_short_line_returns_none ... ok
    test cmd::install::tests::node_bin_names_reads_object_form ... ok
    test cmd::run::tests::cargo_run ... ok
    test cmd::run::tests::cmake_is_unsupported ... ok
    test cmd::run::tests::dotnet_run ... ok
    test cmd::run::tests::kotlin_run ... ok
    test cmd::run::tests::go_run ... ok
    test cmd::run::tests::node_uses_start ... ok
    test cmd::run::tests::swift_run ... ok
    test cmd::run::tests::zig_run ... ok
    test cmd::test_cmd::tests::cargo_test ... ok
    test cmd::test_cmd::tests::cmake_uses_ctest ... ok
    test cmd::test_cmd::tests::dotnet_test ... ok
    test cmd::test_cmd::tests::go_test_all_packages ... ok
    test cmd::test_cmd::tests::kotlin_test ... ok
    test cmd::install::tests::node_bin_names_uses_package_name_for_string_form ... ok
    test cmd::test_cmd::tests::node_yarn_test ... ok
    test cmd::test_cmd::tests::python_with_uv_runs_pytest ... ok
    test cmd::test_cmd::tests::swift_test ... ok
    test cmd::test_cmd::tests::zig_test ... ok
    test runner::tests::append_args_extends_last_step ... ok
    test runner::tests::append_args_noop_on_empty ... ok
    test runner::tests::step_display_simple ... ok
    test runner::tests::step_display_quotes_spaces ... ok
    test cmd::install::tests::python_script_names_reads_project_scripts ... ok
    test cmd::doctor::tests::python_no_uv_uses_pip ... ok
    test cmd::doctor::tests::python_uv_prefers_uv ... ok

    test result: ok. 135 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s


    running 25 tests
    test help_shows_all_subcommands ... ok
    test clean_dry_run_nothing_to_clean ... ok
    test install_default_dry_run_cargo_keeps_builtin_defaulting ... ok
    test install_dry_run_cargo ... ok
    test install_default_dry_run_reports_unsupported_for_go ... ok
    test clean_dry_run_node_shows_size ... ok
    test install_default_dry_run_ignores_shell_hooks_for_node ... ok
    test install_default_dry_run_uses_python_project_scripts ... ok
    test install_dry_run_node_yarn ... ok
    test install_dry_run_go ... ok
    test install_dry_run_node_npm ... ok
    test install_dry_run_cmake ... ok
    test install_dry_run_cargo_workspace_uses_member_path ... ok
    test install_dry_run_cargo_workspace_with_multiple_bins_fails ... ok
    test install_dry_run_cargo_workspace_without_bin_fails ... ok
    test run_dry_run_cargo ... ok
    test install_dry_run_with_extra_args ... ok
    test run_dry_run_go ... ok
    test install_dry_run_workspace_with_extra_args ... ok
    test test_dry_run_cargo ... ok
    test install_dry_run_python ... ok
    test version_flag ... ok
    test test_dry_run_go ... ok
    test install_no_project_fails ... ok
    test ports_runs_without_error ... ok

    test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.36s
kind: epic
paths:
- crates/uu/src/cmd/install.rs
- crates/uu/tests/cli.rs
- README.md
attempt_log:
- num: 1
  outcome: success
  notes: Acceptance met and full verify command passes.
  agent: imp
  started_at: '2026-04-24T05:29:32.600424Z'
  finished_at: '2026-04-24T05:35:22.063630Z'
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

Goal: replace the current `uu install --default` repo-controlled shell hook behavior with a hardened, built-in/default-metadata mechanism.

Problem statement:
- Current implementation appends `tools/uu-post-install` or `tools/uu-post-install.sh` when `--default` is passed for non-Cargo ecosystems.
- User explicitly rejected this boundary: `--default` should not automatically run arbitrary repo-controlled shell scripts.
- Desired semantics: `--default` performs only inspectable, built-in, user-local defaulting operations, or clearly reports that defaulting is unsupported for the detected ecosystem.

Plan / decomposition:
1. Remove automatic hook execution from `crates/uu/src/cmd/install.rs`:
   - Delete or disable `post_install_steps()` shell hook behavior.
   - Ensure `uu install --default -n` never prints/runs `tools/uu-post-install` or `bash tools/uu-post-install.sh`.
2. Keep Cargo built-in defaulting:
   - Preserve `apply_cargo_default(dry_run)` and its user-local destination resolution.
   - Preserve workspace-aware `cargo install --path crates/uu` behavior already present.
3. Add hardened built-in adapters where metadata is reliable:
   - Node: parse `package.json` `bin` field for command names; after existing package-manager install/link behavior, default those command names without executing repo scripts.
   - Python: parse `pyproject.toml` `[project.scripts]` command names; default those command names after install/tool install where possible.
   - Go: only default command names when they can be derived safely from installed binaries or explicit package metadata; otherwise mark unsupported rather than guessing.
4. Unsupported ecosystems:
   - For `--default`, after normal install, print a clear warning/error-style message such as `defaulting is not yet supported for <label>; install completed` depending on chosen UX.
   - Do not fail normal install unless the built-in defaulting operation itself was requested and cannot complete safely; inspect existing error style before choosing.
5. Tests:
   - Update CLI tests that currently expect hook execution for Node/Go.
   - Add negative dry-run coverage proving hook files are ignored for `--default`.
   - Add positive dry-run coverage for Cargo and at least one metadata-based ecosystem adapter.
6. Docs:
   - Update README to remove hook recommendation.
   - Document that `--default` uses hardened built-in adapters/declarative metadata only.

Scope boundaries:
- In scope: `crates/uu/src/cmd/install.rs`, `crates/uu/tests/cli.rs`, `README.md`.
- Out of scope unless required: adding dependencies, changing project-detect, broad refactors, CI/build config changes.
- Do not add arbitrary script execution under another name unless explicitly requested later.

Acceptance:
- `uu install --default -n` no longer auto-runs arbitrary shell hooks.
- Cargo defaulting still works.
- At least one non-Cargo default path uses built-in/declarative logic or produces a clear unsupported message.
- Verify: `cargo fmt --check && cargo clippy -p univ-utils -- -D warnings && cargo test -p univ-utils`.
