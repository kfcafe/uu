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
    fs::create_dir_all(dir.path().join("crates/uu/src")).unwrap();
    fs::write(
        dir.path().join("crates/uu/Cargo.toml"),
        "[package]\nname = \"univ-utils\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("crates/uu/src/main.rs"), "fn main() {}\n").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo install --path crates/uu"));
}

#[test]
fn install_dry_run_cargo_workspace_with_multiple_bins_fails() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]",
    )
    .unwrap();

    for name in ["a", "b"] {
        let crate_dir = dir.path().join("crates").join(name);
        fs::create_dir_all(crate_dir.join("src")).unwrap();
        fs::write(
            crate_dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        fs::write(crate_dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    }

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("multiple installable members"))
        .stderr(predicate::str::contains("a"))
        .stderr(predicate::str::contains("b"));
}

#[test]
fn install_dry_run_cargo_workspace_without_bin_fails() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/lib\"]",
    )
    .unwrap();
    let crate_dir = dir.path().join("crates/lib");
    fs::create_dir_all(crate_dir.join("src")).unwrap();
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(crate_dir.join("src/lib.rs"), "pub fn x() {}\n").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no installable workspace member was found",
        ));
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
    fs::create_dir_all(dir.path().join("crates/uu/src")).unwrap();
    fs::write(
        dir.path().join("crates/uu/Cargo.toml"),
        "[package]\nname = \"univ-utils\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(dir.path().join("crates/uu/src/main.rs"), "fn main() {}\n").unwrap();

    uu().args(["install", "-n", "--", "--release"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "cargo install --path crates/uu --release",
        ));
}

#[test]
fn install_default_dry_run_ignores_shell_hooks_for_node() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{ "name": "pkg", "bin": { "mycli": "cli.js" } }"#,
    )
    .unwrap();
    fs::create_dir(dir.path().join("tools")).unwrap();
    fs::write(dir.path().join("tools/uu-post-install.sh"), "#!/bin/sh\n").unwrap();

    uu().args(["install", "-n", "--default"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("npm install"))
        .stderr(predicate::str::contains("would default"))
        .stderr(predicate::str::contains("mycli"))
        .stderr(predicate::str::contains("uu-post-install").not());
}

#[test]
fn install_default_dry_run_reports_unsupported_for_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();
    fs::create_dir(dir.path().join("tools")).unwrap();
    fs::write(dir.path().join("tools/uu-post-install"), "#!/bin/sh\n").unwrap();

    uu().args(["install", "-n", "--default"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("go install ./..."))
        .stderr(predicate::str::contains(
            "defaulting is not yet supported for Go projects",
        ))
        .stderr(predicate::str::contains("uu-post-install").not());
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
fn install_default_dry_run_uses_python_project_scripts() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project.scripts]\npycli = \"pkg:main\"\n",
    )
    .unwrap();

    uu().args(["install", "-n", "--default"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("would default"))
        .stderr(predicate::str::contains("pycli"));
}

#[test]
fn install_dry_run_make_monorepo_installs_child_projects() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Makefile"), "build:\n\t@echo build\n").unwrap();

    let sdk = dir.path().join("libs/sdk");
    fs::create_dir_all(&sdk).unwrap();
    fs::write(sdk.join("pyproject.toml"), "[project]\nname = \"sdk\"\n").unwrap();

    let cli = dir.path().join("libs/cli");
    fs::create_dir_all(&cli).unwrap();
    fs::write(
        cli.join("pyproject.toml"),
        "[project]\nname = \"cli\"\n[project.scripts]\ncli = \"cli:main\"\n",
    )
    .unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("project").and(predicate::str::contains("libs/cli")))
        .stderr(predicate::str::contains("uv tool install --force ."))
        .stderr(predicate::str::contains("project").and(predicate::str::contains("libs/sdk")))
        .stderr(predicate::str::contains("uv pip install ."))
        .stderr(predicate::str::contains("make install").not());
}

#[test]
fn install_dry_run_make_without_child_projects_uses_make_install() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Makefile"), "install:\n\t@echo install\n").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("would run").and(predicate::str::contains("make")))
        .stderr(
            predicate::str::contains("would run").and(predicate::str::contains("make install")),
        );
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

#[test]
fn clean_all_dry_run_cleans_direct_child_projects() {
    let dir = tempdir().unwrap();

    let node = dir.path().join("node-app");
    fs::create_dir(&node).unwrap();
    fs::write(node.join("package.json"), "{}").unwrap();
    fs::create_dir(node.join("node_modules")).unwrap();
    fs::write(node.join("node_modules/fake.js"), "x").unwrap();

    let rust = dir.path().join("rust-app");
    fs::create_dir(&rust).unwrap();
    fs::write(rust.join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

    fs::create_dir(dir.path().join("not-a-project")).unwrap();

    uu().args(["clean", "--all", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("node-app"))
        .stderr(predicate::str::contains("rust-app"))
        .stderr(predicate::str::contains("node_modules/"))
        .stderr(predicate::str::contains("cargo clean"));
}

#[test]
fn clean_all_dry_run_skips_when_no_child_projects() {
    let dir = tempdir().unwrap();

    uu().args(["clean", "--all", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("no projects found"));
}

#[test]
fn ports_runs_without_error() {
    // Just verify it doesn't crash — output depends on system state
    uu().args(["ports"]).assert().success();
}
