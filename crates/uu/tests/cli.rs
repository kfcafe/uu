use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn uu() -> assert_cmd::Command {
    cargo_bin_cmd!("uu")
}

// -- Help & version ----------------------------------------------------------

#[test]
fn help_shows_all_subcommands() {
    uu().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains("ports"));
}

#[test]
fn version_flag() {
    uu().arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("uu "));
}

// -- Install dry-run ---------------------------------------------------------

#[test]
fn install_dry_run_cargo() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Rust"))
        .stderr(predicate::str::contains("cargo install --path ."));
}

#[test]
fn install_dry_run_cargo_workspace_uses_member_path() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/uu\"]",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("crates/uu")).unwrap();
    fs::write(
        dir.path().join("crates/uu/Cargo.toml"),
        "[package]\nname = \"univ-utils\"",
    )
    .unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo install --path crates/uu"));
}

#[test]
fn install_dry_run_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Go"))
        .stderr(predicate::str::contains("go install ./..."));
}

#[test]
fn install_dry_run_node_npm() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Node.js"))
        .stderr(predicate::str::contains("npm install"));
}

#[test]
fn install_dry_run_node_yarn() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();
    fs::write(dir.path().join("yarn.lock"), "").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("yarn install"));
}

#[test]
fn install_dry_run_python() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("pyproject.toml"), "").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Python"))
        .stderr(predicate::str::contains("install ."));
}

#[test]
fn install_dry_run_cmake() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("CMakeLists.txt"), "").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cmake -B build"))
        .stderr(predicate::str::contains("cmake --install build"));
}

#[test]
fn install_dry_run_with_extra_args() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

    uu().args(["install", "-n", "--", "--release"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo install --path . --release"));
}

#[test]
fn install_dry_run_workspace_with_extra_args() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/uu\"]",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("crates/uu")).unwrap();
    fs::write(
        dir.path().join("crates/uu/Cargo.toml"),
        "[package]\nname = \"univ-utils\"",
    )
    .unwrap();

    uu().args(["install", "-n", "--", "--release"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "cargo install --path crates/uu --release",
        ));
}

#[test]
fn install_default_dry_run_uses_hook_for_node() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();
    fs::create_dir(dir.path().join("tools")).unwrap();
    fs::write(dir.path().join("tools/uu-post-install.sh"), "#!/bin/sh\n").unwrap();

    uu().args(["install", "-n", "--default"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("npm install"))
        .stderr(predicate::str::contains("bash tools/uu-post-install.sh"));
}

#[test]
fn install_default_dry_run_uses_hook_for_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();
    fs::create_dir(dir.path().join("tools")).unwrap();
    fs::write(dir.path().join("tools/uu-post-install"), "#!/bin/sh\n").unwrap();

    uu().args(["install", "-n", "--default"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("go install ./..."))
        .stderr(predicate::str::contains("./tools/uu-post-install"));
}

#[test]
fn install_default_dry_run_cargo_keeps_builtin_defaulting() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

    uu().args(["install", "-n", "--default"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo install --path ."))
        .stderr(predicate::str::contains("would default"));
}

#[test]
fn install_no_project_fails() {
    let dir = tempdir().unwrap();

    uu().args(["install"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recognized project"))
        .stderr(predicate::str::contains("supported project files"));
}

// -- Run dry-run -------------------------------------------------------------

#[test]
fn run_dry_run_cargo() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "").unwrap();

    uu().args(["run", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo run"));
}

#[test]
fn run_dry_run_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();

    uu().args(["run", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("go run ."));
}

// -- Test dry-run ------------------------------------------------------------

#[test]
fn test_dry_run_cargo() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "").unwrap();

    uu().args(["test", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo test"));
}

#[test]
fn test_dry_run_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();

    uu().args(["test", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("go test ./..."));
}

// -- Clean dry-run -----------------------------------------------------------

#[test]
fn clean_dry_run_node_shows_size() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();

    // Create a fake node_modules with some content
    let nm = dir.path().join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("fake.js"), "x".repeat(1024)).unwrap();

    uu().args(["clean", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("node_modules/"))
        .stderr(predicate::str::contains("would"));
}

#[test]
fn clean_dry_run_nothing_to_clean() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();
    // No node_modules directory exists

    uu().args(["clean", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("nothing to clean"));
}

// -- Ports -------------------------------------------------------------------

#[test]
fn ports_runs_without_error() {
    // Just verify it doesn't crash — output depends on system state
    uu().args(["ports"]).assert().success();
}
